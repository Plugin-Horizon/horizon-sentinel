//! Sentinel as an arismoko native trait mod (`mod_api`). The logic is the unchanged `sentinel-core`
//! crate (core/); this file wires it to hooks and host handles, and implements its `World` (one
//! NPC body) and `NpcBridge` (Citizens) over arismoko's API. The NPC library is Citizens, declared
//! as a `[dependencies]` entry in horizon-mod.toml. See PORT_NOTES.md for every assumed signature.
//!
//! Every `TODO(arismoko-api: ...)` names a host method arismoko lacks, and what is lost meanwhile.
//! `VERIFY` marks a signature read from a hook declaration or doc rather than from compiled code.
use citizens::api;
use citizens::npc::Location as CitLocation;
use citizens::yaml::{Node as CitNode, Style};
use mod_api::{
    ClipBlock, ClipFluid, Command, CommandArgumentValue, CommandSource, Compound, Config,
    ConfigKind, ConfigLoadingEvent, ConfigReloadingEvent, EntityRef, EquipmentSlot, Flow, Hand,
    HostSide, Hooks, Identifier, ItemStack, LevelRandomHost, LevelRef, LevelTickPostEvent,
    LivingDamagePreEvent, LivingDeathEvent, LivingDropsEvent, LivingIncomingDamageEvent,
    LivingKnockbackEvent, LivingRef, MobRef, PlayerRef, RegisterCommandsEvent, RegisterConfigsEvent,
    RunModCommandEvent, ServerChatEvent, ServerHost, ServerStoppingEvent, ServerTickEvent,
    ServerTickPhase, SoundSource, Suggestion, TabCompleteEvent, Tag, Text,
};
use sentinel_core::combat::HitDecision;
use sentinel_core::commands::{self, Effect, Sender};
use sentinel_core::data::Location;
use sentinel_core::node::Node;
use sentinel_core::npc::{NpcBridge, NpcId, TRAIT_NAME};
use sentinel_core::registry::DeathOutcome;
use sentinel_core::view::{EntityView, GameMode, ItemView, NpcInfo, Vec3};
use sentinel_core::world::{Projectile, World};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

const ID: &str = sentinel_core::MOD_ID;
/// Sentinel's own config in the server folder, read instead of the TOML whenever it exists.
const ORIGINAL_CONFIG: &str = "plugins/Sentinel/config.yml";

fn log(line: &str) {
    eprintln!("[sentinel] {line}");
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ----- state the hooks share (they run one at a time on the server thread) -----

/// NPCs whose settings changed (`/sentinel ...`), written at the next save pass.
static DIRTY: Mutex<BTreeSet<NpcId>> = Mutex::new(BTreeSet::new());
/// The entity views of the last level tick. The damage, knockback and chat hooks decide with these,
/// since they get no host to read the world from.
static VIEWS: Mutex<BTreeMap<i32, EntityView>> = Mutex::new(BTreeMap::new());
/// The amount a Sentinel victim's hit comes to by Sentinel's armor rule, for `living_damage_pre`.
static FINAL_DAMAGE: Mutex<Vec<(i32, f32, Instant)>> = Mutex::new(Vec::new());
/// Death outcomes waiting for the dying entity's drop roll (`living_death` -> `living_drops`).
static DEATHS: Mutex<Vec<(u32, DeathOutcome)>> = Mutex::new(Vec::new());
/// Arrows a Sentinel shot: dimension, entity id, the game time they are removed at.
static ARROWS: Mutex<Vec<(String, u32, i64)>> = Mutex::new(Vec::new());

fn view(id: i32) -> Option<EntityView> {
    lock(&VIEWS).get(&id).cloned()
}

fn view_by_uuid(uuid: u128) -> Option<EntityView> {
    lock(&VIEWS).values().find(|v| v.uuid == uuid).cloned()
}

/// Replaces the cached views of `dimension` with `views` (the other dimensions keep theirs).
fn cache_views(dimension: &str, views: &[EntityView]) {
    let mut map = lock(&VIEWS);
    map.retain(|_, v| v.dimension != dimension);
    for v in views {
        map.insert(v.id, v.clone());
    }
}

fn set_final_damage(victim: i32, amount: f32) {
    let now = Instant::now();
    let mut f = lock(&FINAL_DAMAGE);
    f.retain(|(v, _, at)| *v != victim && now.duration_since(*at).as_millis() < 500);
    f.push((victim, amount, now));
}

/// The amount set for `victim` in the last half second, used up.
fn take_final_damage(victim: i32) -> Option<f32> {
    let mut f = lock(&FINAL_DAMAGE);
    let i = f.iter().position(|(v, _, at)| *v == victim && at.elapsed().as_millis() < 500)?;
    Some(f.remove(i).1)
}

fn note_death(entity: u32, outcome: DeathOutcome) {
    let mut d = lock(&DEATHS);
    d.retain(|(e, _)| *e != entity);
    d.push((entity, outcome));
    // A death whose loot never rolled (removed without drops) must not pile up.
    if d.len() > 64 {
        d.remove(0);
    }
}

fn take_death(entity: u32) -> Option<DeathOutcome> {
    let mut d = lock(&DEATHS);
    let i = d.iter().position(|(e, _)| *e == entity)?;
    Some(d.remove(i).1)
}

/// Whether Citizens has its registry up (it does once the world is open, even with no NPCs).
fn library_ready() -> bool {
    api::with_world(|_| ()).is_some()
}

// ----- the NPC library: Citizens through `citizens::api` -----
//
// Assumed: the horizon-citizens `api` module as the horizon-rust bridge used it (2026-11-06):
// `by_entity`, `entity_of`, `npc`, `trait_data`, `set_trait_data`, `with_trait`, `subscribe`,
// `spawn`, `despawn`, `remove`, `teleport`, `set_protected`, `selected`, `with_world`; the NPC
// types `citizens::npc::Location`, `citizens::yaml::{Node, Style}` and `citizens::NpcEvent`.
// VERIFY against the arismoko citizens port when it lands.

/// Citizens' node -> Sentinel's (Citizens reads saves.yml with the vendored yaml).
fn from_citizens(n: &CitNode) -> Node {
    match n {
        // A plain `null` / `~` is no value, not the text "null".
        CitNode::Scalar { .. } if n.is_null() => Node::Null,
        CitNode::Scalar { value, style: Style::Plain } => Node::Scalar(value.clone()),
        CitNode::Scalar { value, .. } => Node::Str(value.clone()),
        CitNode::Seq { items, .. } => Node::List(items.iter().map(from_citizens).collect()),
        CitNode::Map { entries, .. } => {
            Node::Map(entries.iter().map(|e| (e.key.clone(), from_citizens(&e.value))).collect())
        }
    }
}

/// Sentinel's node -> Citizens'.
fn to_citizens(n: &Node) -> CitNode {
    match n {
        Node::Null => CitNode::null(),
        // A plain scalar keeps its text (numbers and bools stay unquoted).
        Node::Scalar(s) => CitNode::Scalar { value: s.clone(), style: Style::Plain },
        // A string that reads like a number gets quotes, as SnakeYAML writes a `String`.
        Node::Str(s) if s.parse::<f64>().is_ok() || matches!(s.as_str(), "true" | "false") => {
            CitNode::Scalar { value: s.clone(), style: Style::Single }
        }
        Node::Str(s) => CitNode::str(s),
        Node::List(items) => CitNode::seq(items.iter().map(to_citizens).collect()),
        Node::Map(entries) => {
            let mut m = CitNode::map();
            for (k, v) in entries {
                m.set(k, to_citizens(v));
            }
            m
        }
    }
}

fn to_location(l: &Location) -> CitLocation {
    CitLocation { world: l.world.clone(), x: l.x, y: l.y, z: l.z, yaw: l.yaw, pitch: l.pitch, body_yaw: l.yaw }
}

fn from_location(l: &CitLocation) -> Location {
    Location { world: l.world.clone(), world_id: None, x: l.x, y: l.y, z: l.z, yaw: l.yaw, pitch: l.pitch }
}

/// Citizens, through its api. Stateless: the library keeps the NPCs.
#[derive(Default)]
struct CitizensBridge;

impl NpcBridge for CitizensBridge {
    fn npc_of_entity(&self, entity: i32) -> Option<NpcId> {
        api::by_entity(u32::try_from(entity).ok()?)
    }
    fn entity_of(&self, npc: NpcId) -> Option<i32> {
        api::entity_of(npc).and_then(|e| i32::try_from(e).ok())
    }
    fn dimension_of(&self, npc: NpcId) -> Option<String> {
        api::npc(npc)?.dimension
    }
    fn name(&self, npc: NpcId) -> Option<String> {
        api::npc(npc).map(|n| n.name)
    }
    fn owner(&self, npc: NpcId) -> Option<u128> {
        // The `owner` trait (`traits.owner.uuid`). GAP in horizon-citizens' api: no `owner(id)`.
        let data = from_citizens(&api::trait_data(npc, "owner")?);
        sentinel_core::list::parse_uuid(data.str("uuid")?)
    }
    fn is_spawned(&self, npc: NpcId) -> bool {
        api::entity_of(npc).is_some()
    }
    fn spawn(&mut self, npc: NpcId, at: Option<&Location>) -> bool {
        if let Some(at) = at {
            api::teleport(npc, to_location(at));
        }
        api::spawn(npc)
    }
    fn destroy(&mut self, npc: NpcId) {
        api::remove(npc);
    }
    fn set_spawned_flag(&mut self, npc: NpcId, spawned: bool) {
        if spawned {
            api::spawn(npc);
        } else {
            // Sets `spawned: false` in the saves (the body is gone already after a death).
            api::despawn(npc);
        }
    }
    fn set_protected(&mut self, npc: NpcId, protected: bool) {
        api::set_protected(npc, protected);
    }
    fn set_stored_location(&mut self, npc: NpcId, at: &Location) {
        // On a despawned NPC this moves its saved location only.
        api::teleport(npc, to_location(at));
    }
    fn stored_location(&self, npc: NpcId) -> Option<Location> {
        api::npc(npc)?.location.as_ref().map(from_location)
    }
    fn selected(&self, player: u128) -> Option<NpcId> {
        api::selected(player)
    }
    fn waypoints(&self, _npc: NpcId) -> Vec<Vec3> {
        // GAP in horizon-citizens' api: the waypoints of an NPC (`Npc::waypoints` is internal).
        Vec::new()
    }
    fn pause_waypoints(&mut self, _npc: NpcId, _paused: bool) {
        // GAP in horizon-citizens' api: pausing its waypoint walking during a fight.
    }
    fn mark_dirty(&mut self, npc: NpcId) {
        lock(&DIRTY).insert(npc);
    }
}

/// Runs `f` with the NPC library, if it is up.
fn with_bridge<R>(f: impl FnOnce(&mut CitizensBridge) -> R) -> Option<R> {
    library_ready().then(|| f(&mut CitizensBridge))
}

// ----- players, bodies and items on arismoko's handles -----

/// An online player by UUID (`players().server_players()`, connection order).
fn player_by_uuid(host: &dyn ServerHost, uuid: u128) -> Option<PlayerRef> {
    host.players().server_players().into_iter().find(|p| p.uuid() == uuid)
}

/// The entity of a body by its entity id.
fn entity_by_id(host: &dyn ServerHost, id: i32) -> Option<EntityRef> {
    host.entity(u32::try_from(id).ok()?)
}

/// What Sentinel reads of an item stack for its weapon and armor rules: the item id (`item_key_id`).
// TODO(arismoko-api: stack_components): the custom name, the lore and the enchantment levels (Sharpness,
// Power) are not read, so the weapon rules see no names and no enchantment damage.
fn item_view(host: &dyn ServerHost, stack: &ItemStack) -> Option<ItemView> {
    Some(ItemView::of(&stack.item().id(host)))
}

/// The game mode the core names, from its numeric id (0 survival, 1 creative, 2 adventure, 3 spectator).
fn game_mode_of(id: i32) -> GameMode {
    match id {
        1 => GameMode::Creative,
        2 => GameMode::Adventure,
        3 => GameMode::Spectator,
        _ => GameMode::Survival,
    }
}

/// The worn armor slots, in the order `ArmorSlots` reads them.
const ARMOR_SLOTS: [EquipmentSlot; 4] = [EquipmentSlot::Head, EquipmentSlot::Chest, EquipmentSlot::Legs, EquipmentSlot::Feet];

/// Every living entity of `level` in place and every player there, as the core sees them.
fn build_views(host: &mut dyn ServerHost, level: LevelRef, dim: &str, bridge: &CitizensBridge) -> Vec<EntityView> {
    let mut out = Vec::new();
    for entity in level.all_entities(&*host).unwrap_or_default() {
        let Some(living) = host.living(entity) else { continue };
        let Some(kind) = entity.kind(&*host) else { continue };
        let id = entity.id() as i32;
        let uuid = entity.uuid(&mut *host).unwrap_or(0);
        let pos = entity.position(&*host).unwrap_or([0.0; 3]);
        let eye = entity.eye_position(&*host).unwrap_or(pos);
        let [yaw, pitch] = entity.rotation(&*host).unwrap_or([0.0; 2]);
        let velocity = entity.velocity(&*host).unwrap_or([0.0; 3]);
        let mob = host.mob(entity);
        let player = (kind == "minecraft:player").then(|| player_by_uuid(&*host, uuid)).flatten();
        let npc = bridge.npc_of_entity(id).map(|n| NpcInfo {
            id: n,
            name: bridge.name(n).unwrap_or_default(),
            sentinel: sentinel_core::registry().is_sentinel(n),
        });
        let armor = ARMOR_SLOTS
            .map(|slot| living.equipment(&*host, slot).as_ref().and_then(|s| item_view(&*host, s)));
        let v = EntityView {
            id,
            uuid,
            kind,
            dimension: dim.to_owned(),
            pos,
            eye,
            yaw,
            pitch,
            velocity,
            alive: living.alive(&*host).unwrap_or(false),
            health: f64::from(living.health(&*host).unwrap_or(0.0)),
            max_health: f64::from(living.max_health(&*host).unwrap_or(0.0)),
            // VERIFY: `player.name` returns an Option<String>.
            player_name: player.and_then(|p| p.name(&*host)),
            game_mode: player
                .and_then(|p| p.game_mode(&*host))
                .map_or(GameMode::Survival, |mode| game_mode_of(mode.id())),
            custom_name: entity.name(&*host),
            mob_has_target: mob.map(|m| m.target(&*host).is_some()),
            // The Invisible effect, read as the entity's invisible flag.
            invisible_effect: entity.is_invisible(&*host).unwrap_or(false),
            glowing: entity.glowing(&*host).unwrap_or(false),
            npc,
            main_hand: living.hand_item(&*host, Hand::Main).as_ref().and_then(|s| item_view(&*host, s)),
            off_hand: living.hand_item(&*host, Hand::Off).as_ref().and_then(|s| item_view(&*host, s)),
            armor,
            inventory: player
                .and_then(|p| p.inventory(&*host))
                .unwrap_or_default()
                .iter()
                .filter_map(|s| item_view(&*host, s))
                .collect(),
            ..Default::default()
        };
        out.push(v);
    }
    out
}

/// A Sentinel body runs the engine's navigation: Citizens spawns non-player NPCs with `NoAI`.
fn allow_ai(host: &mut dyn ServerHost, mob: MobRef) {
    mob.set_no_ai(host, false);
    // The horizon-rust port emptied the mob's target list: every target goal is removed here too.
    let goals: Vec<&'static str> = mob
        .target_goals(&*host)
        .unwrap_or_default()
        .iter()
        .map(|goal| goal.id)
        .collect();
    for id in goals {
        let _ = mob.remove_target_goal(host, id);
    }
}

// ----- one Sentinel body, as the core's `World` -----

/// One Sentinel body in its level. The config values it needs are read before the update
/// (`registry().update` holds the registry lock, so no method here may lock it again).
struct Body<'g> {
    host: &'g mut dyn ServerHost,
    level: LevelRef,
    dim: String,
    npc: NpcId,
    /// The body: a player-type NPC is a `minecraft:mannequin`, a living entity but no `Mob`.
    entity: EntityRef,
    living: LivingRef,
    /// Navigation and looking need a `Mob`; None for a mannequin body (it stands and hits).
    mob: Option<MobRef>,
    views: &'g [EntityView],
    bridge: CitizensBridge,
    random: u64,
    navigating: bool,
    /// `enhance los traces`. The glass-transparent trace is not ported (see `line_of_sight`).
    los_traces: bool,
    arrow_cleanup: i64,
    game_time: i64,
    /// Hits asked for during `registry().update`, applied after it returns: a hit runs the damage
    /// hooks (`living_incoming_damage`, `living_death`, ...), which lock the registry again, and a
    /// std `Mutex` locked twice on one thread deadlocks (the server thread froze in the first test).
    hurts: Vec<(EntityRef, &'static str, f32, Option<EntityRef>)>,
}

impl Body<'_> {
    fn body_entity(&self) -> EntityRef {
        self.entity
    }
    fn living(&self) -> LivingRef {
        self.living
    }
    fn uuid_of_body(&self) -> u128 {
        let id = self.entity.id() as i32;
        self.views.iter().find(|e| e.id == id).map(|e| e.uuid).unwrap_or(0)
    }
}

impl World for Body<'_> {
    fn nearby(&self, center: Vec3, range: f64) -> Vec<EntityView> {
        self.views
            .iter()
            .filter(|e| {
                e.dimension == self.dim
                    && (e.pos[0] - center[0]).abs() <= range
                    && (e.pos[1] - center[1]).abs() <= range
                    && (e.pos[2] - center[2]).abs() <= range
            })
            .cloned()
            .collect()
    }

    fn entity(&self, uuid: u128) -> Option<EntityView> {
        self.views.iter().find(|e| e.uuid == uuid).cloned()
    }

    fn line_of_sight(&self, from: Vec3, to: Vec3) -> bool {
        if from == to {
            return true;
        }
        // `level_clip` with the collider shapes: a ray that hits nothing is a miss. A failed read
        // counts as blocked. TODO(arismoko-api: see_through_blocks): with `enhance los traces` on,
        // Sentinel's glass, leaves and bars pass; here every collider blocks.
        let _ = self.los_traces;
        self.level
            .clip(&*self.host, from, to, ClipBlock::Collider, ClipFluid::None)
            .is_some_and(|hit| hit.miss)
    }

    fn min_y(&self) -> f64 {
        // VERIFY: handle `height` from the host's `level_height` (low, high).
        self.level.height(&*self.host).map_or(-64.0, |(low, _)| f64::from(low))
    }

    fn random(&mut self) -> f64 {
        // xorshift: the core only needs spread, not vanilla's sequence.
        self.random ^= self.random << 13;
        self.random ^= self.random >> 7;
        self.random ^= self.random << 17;
        (self.random >> 11) as f64 / (1u64 << 53) as f64
    }

    fn set_max_health(&mut self, max: f64) {
        // VERIFY: `living_set_attribute_base` with the max_health attribute id.
        let living = self.living();
        let _ = living.set_attribute_base(&mut *self.host, "minecraft:max_health", max);
        let current = living.health(&*self.host).unwrap_or(0.0);
        let _ = living.set_health(&mut *self.host, current.min(max as f32));
    }

    fn set_health(&mut self, health: f64) {
        let _ = self.living().set_health(&mut *self.host, health as f32);
    }

    fn hurt_self(&mut self, amount: f64) {
        self.hurts.push((self.body_entity(), "minecraft:out_of_world", amount as f32, None));
    }

    fn teleport_self(&mut self, to: Vec3) {
        let _ = self.body_entity().teleport_to(&mut *self.host, to, None);
    }

    fn stop_motion(&mut self) {
        let _ = self.body_entity().set_velocity(&mut *self.host, [0.0; 3]);
    }

    fn navigate_to(&mut self, to: Vec3, speed: f64, margin: f64) {
        // TODO(arismoko-api: mob_move_to_margin): the arrival margin is not settable; vanilla stops
        // within `margin` of the point. `margin` is ignored.
        let _ = margin;
        // TODO(arismoko-api: mannequin_navigation): a mannequin body has no PathNavigation.
        self.navigating = self.mob.is_some_and(|m| m.move_to(&mut *self.host, to, speed));
    }

    fn navigate_to_entity(&mut self, target: &EntityView, speed: f64) {
        // TODO(arismoko-api: mob_follow_entity): there is no follow; the body walks to the target's
        // position of this tick, and the brain asks again every tick.
        self.navigating = self.mob.is_some_and(|m| m.move_to(&mut *self.host, target.pos, speed));
    }

    fn cancel_navigation(&mut self) {
        if let Some(m) = self.mob {
            let _ = m.stop(&mut *self.host);
        }
        self.navigating = false;
    }

    fn is_navigating(&self) -> bool {
        self.navigating
    }

    fn face(&mut self, at: Vec3) {
        // TODO(arismoko-api: mannequin_look_at): a mannequin body does not turn to its target.
        if let Some(m) = self.mob {
            let _ = m.look_at(&mut *self.host, at);
        }
    }

    fn swing(&mut self) {
        let _ = self.living().swing(&mut *self.host, Hand::Main, true);
    }

    fn set_blocking(&mut self, blocking: bool) {
        // TODO(arismoko-api: living_set_blocking): only `is_blocking` is readable; raising a shield
        // is lost, so the block counts in the damage rule only if the body blocks by other means.
        let _ = blocking;
    }

    fn native_attack(&mut self, _target: &EntityView) -> bool {
        // TODO(arismoko-api: mob_attack): the vanilla attack of the body with what it holds is not
        // exposed. `false` makes Sentinel deal its own damage through `hurt`.
        false
    }

    fn hurt(&mut self, target: &EntityView, amount: f64) {
        let Some(victim) = entity_by_id(&*self.host, target.id) else { return };
        let me = self.body_entity();
        self.hurts.push((victim, "minecraft:mob_attack", amount as f32, Some(me)));
    }

    fn push(&mut self, target: &EntityView, velocity: Vec3) {
        // A player is pushed by setting their velocity to the one Sentinel computed (what `push` adds up to
        // for everything else, the delta being `velocity - target.velocity`).
        if target.kind == "minecraft:player" && target.npc.is_none() {
            if let Some(p) = player_by_uuid(&*self.host, target.uuid) {
                let _ = p.set_velocity(&mut *self.host, velocity);
            }
            return;
        }
        let delta = [
            velocity[0] - target.velocity[0],
            velocity[1] - target.velocity[1],
            velocity[2] - target.velocity[2],
        ];
        if let Some(e) = entity_by_id(&*self.host, target.id) {
            let _ = e.push(&mut *self.host, delta);
        }
    }

    fn launch(&mut self, kind: Projectile, from: Vec3, velocity: Vec3, item: Option<&str>) {
        // The projectile is summoned with its motion and owner, as vanilla saves them, so hits count
        // as the NPC's (`Owner` as an int array UUID).
        let me = self.uuid_of_body();
        let mut nbt = Compound::new();
        nbt.insert("Motion".into(), Tag::List(velocity.iter().map(|v| Tag::Double(v.to_bits())).collect()));
        nbt.insert(
            "Owner".into(),
            Tag::IntArray(vec![(me >> 96) as i32, (me >> 64) as i32, (me >> 32) as i32, me as i32]),
        );
        if let (Projectile::Potion, Some(id)) = (kind, item) {
            // TODO(arismoko-api: item_stack_from_id): the potion's contents are not copied.
            let mut stack = Compound::new();
            stack.insert("id".into(), Tag::String(id.into()));
            stack.insert("count".into(), Tag::Int(1));
            nbt.insert("Item".into(), Tag::Compound(stack));
        }
        if kind == Projectile::Arrow {
            // Picked up by nobody, as Sentinel's arrows.
            nbt.insert("pickup".into(), Tag::Byte(0));
        }
        let Some(id) = self.host.summon(&self.dim, kind.kind(), from, nbt) else { return };
        // `arrow cleanup time`: the arrow is removed after that many ticks.
        if kind == Projectile::Arrow && self.arrow_cleanup > 0 {
            lock(&ARROWS).push((self.dim.clone(), id, self.game_time + self.arrow_cleanup));
        }
    }

    fn lightning_effect(&mut self, at: Vec3) {
        // TODO(arismoko-api: visual_lightning): a summoned bolt sets fire and hurts; a visual-only
        // bolt is not available (the same gap as the horizon-rust port).
        let mut nbt = Compound::new();
        nbt.insert("Tags".into(), Tag::List(vec![Tag::String("sentinel_effect".into())]));
        let _ = self.host.summon(&self.dim, "minecraft:lightning_bolt", at, nbt);
    }

    fn make_glow(&mut self, target: &EntityView, ticks: i32) {
        // A timed effect; there is no host method for it, so the command does it.
        let line = format!(
            "effect give {} minecraft:glowing {} 1",
            sentinel_core::list::format_uuid(target.uuid),
            (ticks / 20).max(1)
        );
        let _ = self.host.commands().run_command(&line);
    }

    fn play_sound(&mut self, sound: &str, at: Vec3) {
        // VERIFY: `SoundSource::Hostile` (SoundSource.HOSTILE); queued, not heard.
        let _ = self.level.play_sound(&mut *self.host, sound, at, SoundSource::Hostile, 1.0, 1.0);
    }

    fn tell(&mut self, player: u128, text: &str) {
        if let Some(p) = player_by_uuid(&*self.host, player) {
            let _ = p.tell(&mut *self.host, Text::literal(text));
        }
    }

    fn take_held_item(&mut self) -> bool {
        let living = self.living();
        let Some(mut stack) = living.hand_item(&*self.host, Hand::Main) else { return false };
        let left = stack.count().saturating_sub(1);
        let next = if left == 0 { None } else { stack.set_count(left).ok().map(|()| stack) };
        living.set_hand_item(&mut *self.host, Hand::Main, next).is_ok_and(|ok| ok)
    }

    fn swap_held(&mut self, _pick: &dyn Fn(&str) -> bool) -> bool {
        // TODO(arismoko-api: citizens_inventory): an NPC's own inventory (Citizens' `Inventory`
        // trait) is not in citizens::api yet, so autoswitch finds nothing.
        false
    }

    fn waypoints(&self) -> Vec<Vec3> {
        self.bridge.waypoints(self.npc)
    }

    fn pause_waypoints(&mut self, paused: bool) {
        self.bridge.pause_waypoints(self.npc, paused);
    }
}

// ----- `/sentinel` -----

/// What `/sentinel` may ask of the game and the library.
struct Look<'g> {
    host: &'g dyn ServerHost,
    bridge: &'g CitizensBridge,
    level_name: String,
}

impl commands::Lookup for Look<'_> {
    fn body(&self, npc: NpcId) -> Option<EntityView> {
        view(self.bridge.entity_of(npc)?)
    }
    fn player_uuid(&self, name: &str) -> Option<u128> {
        self.host.players().player_by_name(name).map(|p| p.uuid())
    }
    fn player_name(&self, uuid: u128) -> Option<String> {
        // VERIFY: `player.name` returns an Option<String>.
        player_by_uuid(self.host, uuid)?.name(self.host)
    }
    fn owner(&self, npc: NpcId) -> Option<u128> {
        self.bridge.owner(npc)
    }
    fn npc_name(&self, npc: NpcId) -> Option<String> {
        self.bridge.name(npc)
    }
    fn npc_exists(&self, npc: NpcId) -> bool {
        self.bridge.name(npc).is_some()
    }
    /// The provider's answer for the node (`permission_value`), else the operator level against the node's
    /// default. None for an offline sender, so the sender's own level decides.
    fn grants(&self, player: u128, node: &str, default_level: u8) -> Option<bool> {
        let p = player_by_uuid(self.host, player)?;
        let level = p.permission_level(self.host).unwrap_or(0);
        Some(p.permission_value(self.host, node).unwrap_or(level >= default_level))
    }
    /// `Material.valueOf(name.toUpperCase())` succeeds: the name is a bare registry item (`stick`, not
    /// `minecraft:stick`, which is no Bukkit material name). The host holds the item registry.
    fn is_material(&self, name: &str) -> bool {
        !name.is_empty()
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && Identifier::parse(&format!("minecraft:{}", name.to_ascii_lowercase()))
                .ok()
                .and_then(|id| self.host.item_key(&id))
                .is_some()
    }
    fn world_name(&self, dimension: &str) -> String {
        let level = self.level_name.clone();
        match dimension {
            "minecraft:overworld" => level,
            "minecraft:the_nether" => format!("{level}_nether"),
            "minecraft:the_end" => format!("{level}_the_end"),
            other => other.split_once(':').map_or(other, |(_, p)| p).to_owned(),
        }
    }
}

/// The `level-name` of `server.properties` (Paper's world folder), `world` by default.
fn level_name() -> String {
    std::fs::read_to_string("server.properties")
        .ok()
        .and_then(|text| text.lines().find_map(|l| l.strip_prefix("level-name=").map(|v| v.trim().to_owned())))
        .unwrap_or_else(|| "world".into())
}

/// Runs `/sentinel <args>` for a player (or the console when `player` is None) and carries out what
/// it asks; returns the lines to show.
fn sentinel_command(host: &mut dyn ServerHost, player: Option<u128>, op_level: u8, args: &str) -> Vec<String> {
    let bridge = CitizensBridge;
    if !library_ready() {
        return vec!["\u{a7}2[Sentinel] \u{a7}cNo NPC library is loaded (Sentinel needs Citizens).".into()];
    }
    let sender = Sender { player, op_level, selected: player.and_then(|p| bridge.selected(p)) };
    let out = {
        let look = Look { host: &*host, bridge: &bridge, level_name: level_name() };
        let mut reg = sentinel_core::registry();
        commands::run(&mut reg, &sender, args, &look)
    };
    let mut bridge = CitizensBridge;
    for effect in &out.effects {
        match effect {
            Effect::Dirty(npc) => bridge.mark_dirty(*npc),
            Effect::Protected(npc, protected) => bridge.set_protected(*npc, *protected),
            Effect::Health(npc, h) => {
                let living = bridge.entity_of(*npc).and_then(|id| entity_by_id(&*host, id)).and_then(|e| host.mob(e));
                if let Some(mob) = living {
                    let _ = mob.living().set_attribute_base(&mut *host, "minecraft:max_health", *h);
                    let _ = mob.living().set_health(&mut *host, *h as f32);
                }
            }
            Effect::Kill(npc) => {
                if let (Some(id), Some(dim)) = (bridge.entity_of(*npc), bridge.dimension_of(*npc)) {
                    // The death hook then runs Sentinel's death handling.
                    let _ = host.kill_mob(&dim, id as u32);
                }
            }
            Effect::Respawn(npc) => {
                let at = sentinel_core::registry().respawn_point(*npc, &bridge);
                bridge.spawn(*npc, at.as_ref());
            }
            // The config file is read again on its own within a second of a save.
            Effect::Reload => {}
        }
    }
    out.lines
}

/// The usages of `/sentinel`. Only the bare command and the free-text catch-all are registered: a
/// typed argument is parsed by the server before the listener runs and the raw line cannot be put
/// back in order from it (a literal follows an argument in `attackrate <s> ranged`).
/// TODO(arismoko-api: command_tree_completion): typed subcommand names are not completed by the
/// command tree as the horizon-rust port's usages did. `tab_complete` still completes targets.
const USAGES: [&str; 2] = ["sentinel", "sentinel <args:text>"];

// ----- config -----

fn read_config(text: &str) {
    match std::fs::read_to_string(ORIGINAL_CONFIG) {
        Ok(original) => {
            if let Err(e) = sentinel_core::load_sentinel_yaml(&original) {
                log(&format!("{ORIGINAL_CONFIG} not read, keeping the old config: {e}"));
            }
        }
        Err(_) => {
            if let Err(e) = sentinel_core::load_config(text) {
                log(&format!("config not read, keeping the old one: {e}"));
            }
        }
    }
}

// ----- the level and server ticks -----

/// Removes the arrows of `dim` whose time is up (`arrow cleanup time`).
fn expire_arrows(host: &mut dyn ServerHost, dim: &str, now: i64) {
    let due: Vec<u32> = {
        let mut a = lock(&ARROWS);
        let mut due = Vec::new();
        a.retain(|(d, id, at)| {
            if d.as_str() == dim && *at <= now {
                due.push(*id);
                false
            } else {
                true
            }
        });
        due
    };
    for id in due {
        if let Some(arrow) = host.entity(id) {
            let _ = arrow.discard(&mut *host);
        }
    }
}

/// The level's tick, after its entities ticked: every Sentinel in it counts a tick, and those due
/// (`update rate`) run their update, as Sentinel's trait `run()` does.
fn level_tick(host: &mut dyn ServerHost, level: LevelRef) {
    let dim = level.dimension_id().to_owned();
    let now = level.game_time(&*host).unwrap_or(0);
    expire_arrows(host, &dim, now);
    if sentinel_core::registry().sentinels.is_empty() {
        return;
    }
    let notices = std::mem::take(&mut sentinel_core::registry().notices);
    for (player, line) in notices {
        if let Some(p) = player_by_uuid(&*host, player) {
            let _ = p.tell(&mut *host, Text::literal(line));
        }
    }
    if !library_ready() {
        return;
    }
    let mut bridge = CitizensBridge;
    let views = build_views(&mut *host, level, &dim, &bridge);
    cache_views(&dim, &views);
    let due = sentinel_core::registry().tick(&mut bridge, Some(dim.as_str()));
    let (los_traces, arrow_cleanup) = {
        let reg = sentinel_core::registry();
        (reg.cfg.random.enhance_los_traces, reg.cfg.random.arrow_cleanup_time)
    };
    for &npc in &due {
        let Some(eid) = bridge.entity_of(npc) else { continue };
        let Some(me) = views.iter().find(|v| v.id == eid).cloned() else { continue };
        let Some(entity) = entity_by_id(&*host, eid) else { continue };
        let Some(living) = host.living(entity) else { continue };
        let mob = host.mob(entity);
        if let Some(mob) = mob {
            allow_ai(&mut *host, mob);
        }
        let fresh = {
            let mut reg = sentinel_core::registry();
            if reg.is_new_body(npc, me.uuid) { reg.spawned(npc, &me) } else { None }
        };
        if let Some((_, invincible)) = fresh {
            bridge.set_protected(npc, invincible);
        }
        let guarded = {
            let reg = sentinel_core::registry();
            let Some(sen) = reg.sentinels.get(&npc) else { continue };
            let by_player = sen.data.guarding().and_then(|u| views.iter().find(|v| v.uuid == u).cloned());
            let by_npc = (sen.data.guarded_npc >= 0)
                .then(|| bridge.entity_of(sen.data.guarded_npc))
                .flatten()
                .and_then(|e| views.iter().find(|v| v.id == e).cloned());
            by_player.or(by_npc)
        };
        let owner = bridge.owner(npc);
        let seed = (me.uuid as u64) ^ (now as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        // VERIFY: `mob_is_done` is PathNavigation.isDone; a body navigates while it is not done.
        let navigating = mob.and_then(|m| m.is_done(&*host)).is_some_and(|done| !done);
        let mut body = Body {
            host: &mut *host,
            level,
            dim: dim.clone(),
            npc,
            entity,
            living,
            mob,
            views: &views,
            bridge: CitizensBridge,
            random: seed,
            navigating,
            los_traces,
            arrow_cleanup,
            game_time: now,
            hurts: Vec::new(),
        };
        if let Some((health, _)) = fresh {
            body.set_max_health(health);
            body.set_health(health);
        }
        sentinel_core::registry().update(npc, &me, guarded, owner, &mut body);
        // The registry is unlocked again: now the hits may run their hooks.
        for (victim, kind, amount, source) in std::mem::take(&mut body.hurts) {
            let _ = victim.hurt(&mut *host, kind, amount, None, source);
        }
    }
    sentinel_core::registry().end_tick(&dim, !due.is_empty());
}

// ----- the hooks -----

#[derive(Default)]
struct State {
    subscribed: bool,
    ticks: u64,
    /// What was last written to Citizens for each NPC, to write only changes.
    written: BTreeMap<NpcId, Node>,
}

#[derive(Default)]
struct Mod {
    state: Mutex<State>,
}

/// Subscribes to Citizens' NPC events once its registry is up.
fn install(st: &mut State) {
    if st.subscribed || !library_ready() {
        return;
    }
    st.subscribed = true;
    api::subscribe(|event| {
        use citizens::NpcEvent;
        match event {
            NpcEvent::Despawned { npc, .. } => sentinel_core::registry().despawned(*npc),
            NpcEvent::Removed { npc } => {
                sentinel_core::registry().sentinels.remove(npc);
            }
            _ => {}
        }
    });
}

/// Loads traits Citizens has that Sentinel doesn't, attaches new ones, drops removed ones.
fn sync(st: &mut State) {
    let with: BTreeSet<NpcId> = api::with_trait(TRAIT_NAME).into_iter().collect();
    let mut reg = sentinel_core::registry();
    for &npc in &with {
        if reg.is_sentinel(npc) {
            continue;
        }
        match api::trait_data(npc, TRAIT_NAME).map(|n| from_citizens(&n)) {
            Some(node) if !node.is_empty() => {
                reg.load(npc, &node);
                // The baseline is what a save would write right now, so saves.yml stays as it was.
                if let Some(baseline) = reg.save(npc) {
                    st.written.insert(npc, baseline);
                }
            }
            // `/trait sentinel` on an NPC: a new trait with the config's defaults.
            _ => {
                reg.attach(npc);
                lock(&DIRTY).insert(npc);
            }
        }
    }
    let gone: Vec<NpcId> = reg.sentinels.keys().copied().filter(|n| !with.contains(n)).collect();
    for npc in gone {
        reg.sentinels.remove(&npc);
        st.written.remove(&npc);
        // `/trait remove sentinel` leaves the NPC protected, as Sentinel's `onRemove`.
        api::set_protected(npc, true);
    }
}

/// Writes changed traits into Citizens' saves (stats change every tick, so every few seconds).
fn save(st: &mut State, all: bool) {
    let dirty: BTreeSet<NpcId> = std::mem::take(&mut *lock(&DIRTY));
    let reg = sentinel_core::registry();
    for &npc in reg.sentinels.keys() {
        if !all && !dirty.contains(&npc) && st.ticks % 100 != 0 {
            continue;
        }
        let Some(node) = reg.save(npc) else { continue };
        if st.written.get(&npc) == Some(&node) {
            continue;
        }
        if api::set_trait_data(npc, TRAIT_NAME, to_citizens(&node)) {
            st.written.insert(npc, node);
        }
    }
}

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register_configs(&self, _: HostSide, event: &mut RegisterConfigsEvent<'_>) {
        event.configs.push(Config::new(ID, ConfigKind::Common, sentinel_core::DEFAULT_TOML));
    }

    fn config_loading(&self, _: HostSide, event: &mut ConfigLoadingEvent<'_>) {
        if event.config.mod_id == ID {
            read_config(&event.config.text);
        }
    }

    fn config_reloading(&self, _: HostSide, event: &mut ConfigReloadingEvent<'_>) {
        if event.config.mod_id == ID {
            read_config(&event.config.text);
            log("config read again");
        }
    }

    fn register_commands(&self, _: &mut dyn ServerHost, event: &mut RegisterCommandsEvent<'_>) {
        for usage in USAGES {
            event.commands.push(Command::new(usage));
        }
    }

    /// `/sentinel` from a player or the console. Other sources (command blocks, functions) are refused.
    fn run_mod_command(&self, host: &mut dyn ServerHost, event: &mut RunModCommandEvent<'_>) -> Flow {
        if event.command.literals.first().map(String::as_str) != Some("sentinel") {
            return Flow::Continue;
        }
        let (player, op_level) = match &event.source {
            CommandSource::Player { player, permission } => (Some(player.uuid()), permission.level()),
            CommandSource::Console { permission } => (None, permission.level()),
            _ => {
                let _ = host.commands().feedback(Text::literal("Sentinel answers players and the console only."));
                return Flow::Cancel;
            }
        };
        let args = match event.command.argument("args") {
            Some(CommandArgumentValue::String(s)) => s.clone(),
            _ => String::new(),
        };
        for line in sentinel_command(host, player, op_level, args.trim()) {
            let _ = host.commands().feedback(Text::literal(line));
        }
        Flow::Cancel
    }

    /// `/sentinel addtarget|addignore|...` complete as Sentinel's `onTabComplete` does.
    // VERIFY: the event fields `player`, `command`, `range` and `suggestions` (native_tab_complete).
    fn tab_complete(&self, _host: &mut dyn ServerHost, event: &mut TabCompleteEvent<'_>) {
        let line = event.command.trim_start_matches('/');
        let mut parts = line.splitn(2, ' ');
        if parts.next() != Some("sentinel") {
            return;
        }
        let arguments: Vec<String> = parts.next().unwrap_or("").split_whitespace().map(str::to_owned).collect();
        let selected = CitizensBridge.selected(event.player);
        let words = commands::tab_complete(&sentinel_core::registry(), selected, &arguments).unwrap_or_default();
        for text in words {
            event.suggestions.push(Suggestion::new((event.range[0], event.range[1]), text));
        }
    }

    fn server_chat(&self, _host: &mut dyn ServerHost, event: &mut ServerChatEvent<'_>) -> Flow {
        if let Some(player) = view_by_uuid(event.player) {
            sentinel_core::registry().on_chat(&player, event.raw_text);
        }
        Flow::Continue
    }

    /// A hit on any living entity: Sentinel's protection, armor, fire limit and damage override.
    /// The hook has no host to read the world from, so the views of the last level tick stand in.
    fn living_incoming_damage(&self, _level: &mut dyn LevelRandomHost, event: &mut LivingIncomingDamageEvent<'_>) -> Flow {
        if event.cancelled || !library_ready() {
            return Flow::Continue;
        }
        let Some(victim) = view(event.entity) else { return Flow::Continue };
        let causing = event.source.causing;
        let attacker = if causing == 0 { None } else { view(causing) };
        let amount = event.container.new_damage();
        let decision = sentinel_core::registry().on_hurt(
            &victim,
            attacker.as_ref(),
            event.source.damage_type,
            amount,
            &CitizensBridge,
        );
        match decision {
            HitDecision::Cancel => event.cancelled = true,
            HitDecision::Amount(a) => {
                event.container.set_new_damage(a);
                // Sentinel's armor rule replaces vanilla's: `living_damage_pre` sets this amount.
                set_final_damage(victim.id, a);
            }
            HitDecision::Pass => {}
        }
        Flow::Continue
    }

    /// After vanilla's armor and enchantment protection: a Sentinel's own share stands.
    // VERIFY: the event's `entity` field (the hook declares no arguments in the arismoko docs).
    fn living_damage_pre(&self, _level: &mut dyn LevelRandomHost, event: &mut LivingDamagePreEvent<'_>) {
        if let Some(a) = take_final_damage(event.entity) {
            event.container.set_new_damage(a);
        }
    }

    /// A Sentinel's own death (drops, xp, respawn or stay dead), or an enemy's (forgotten).
    fn living_death(&self, _host: &mut dyn ServerHost, event: &mut LivingDeathEvent<'_>) -> Flow {
        let entity = event.entity as u32;
        let Some(dead) = view(entity as i32) else { return Flow::Continue };
        if !library_ready() {
            return Flow::Continue;
        }
        let mut seed = u64::from(entity).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let outcome = sentinel_core::registry().on_death(&dead, &mut CitizensBridge, || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f64 / (1u64 << 53) as f64
        });
        if outcome.replace_drops || outcome.xp.is_some() {
            // TODO(arismoko-api: living_experience_drop, death_message_hidden): the xp and the
            // hidden death message of the outcome are not applied (see `living_drops`).
            note_death(entity, outcome);
        }
        Flow::Continue
    }

    /// The drops of a dead Sentinel (its own list) or of a Sentinel's enemy (none, with `enemy drops`
    /// off). TODO(arismoko-api: item_key_by_id, item_from_snbt): a Sentinel's own drops (named,
    /// with lore and enchantments) cannot be built, so the vanilla list stays for it.
    fn living_drops(&self, _host: &mut dyn ServerHost, event: &mut LivingDropsEvent<'_>) -> Flow {
        let Some(outcome) = take_death(event.entity as u32) else { return Flow::Continue };
        if outcome.replace_drops && outcome.drops.is_empty() {
            event.drops.clear();
        }
        Flow::Continue
    }

    /// Sentinel's `allow knockback` off: a hit on the Sentinel pushes it nowhere.
    fn living_knockback(&self, _level: &mut dyn LevelRandomHost, event: &mut LivingKnockbackEvent<'_>) -> Flow {
        if let Some(victim) = view(event.entity) {
            if victim.npc.is_some() && !sentinel_core::registry().allows_knockback(&victim) {
                event.cancelled = true;
            }
        }
        Flow::Continue
    }

    fn server_tick(&self, _host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
        let mut guard = lock_state(self);
        let st = &mut *guard;
        match event.phase {
            ServerTickPhase::ServerTickStart { world_running: true, .. } => {
                // The NPC library's bridge: installs once its registry is up, syncs the traits
                // every 5 ticks and saves changes.
                install(st);
                if !st.subscribed {
                    return;
                }
                st.ticks += 1;
                if st.ticks % 5 == 1 {
                    sync(st);
                }
                save(st, false);
            }
            ServerTickPhase::ServerTickEnd { world_running: true, .. } => {
                // Respawn timers of dead Sentinels (`respawn time` > 0).
                if library_ready() {
                    sentinel_core::registry().tick(&mut CitizensBridge, None);
                }
            }
            _ => {}
        }
    }

    fn level_tick_post(&self, host: HostSide, event: &mut LevelTickPostEvent) {
        if let HostSide::Server(host) = host {
            level_tick(host, event.level);
        }
    }

    fn server_stopping(&self, _host: &mut dyn ServerHost, _event: &mut ServerStoppingEvent) {
        let mut guard = lock_state(self);
        if guard.subscribed {
            // Before Citizens writes saves.yml on its last pass (see PORT_NOTES.md: order).
            save(&mut guard, true);
        }
    }
}

fn lock_state(m: &Mod) -> MutexGuard<'_, State> {
    lock(&m.state)
}
