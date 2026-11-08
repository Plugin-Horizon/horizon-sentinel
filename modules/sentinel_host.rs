// Sentinel's part of the game (`crate::game::sentinel_host`): the `World` the core asks, over
// Horizon's `Game` for one Sentinel body, and the entity views the core decides on.
//
// A child module of `game`, so it reaches `Game`'s fields and `pub(super)` helpers (mobs, the
// LAN hub's guests, `clip_ray`, the `native_*` helpers).
//
// UNVERIFIED (blind port, horizon a589a3dd9, no build): field and method names are read from the
// source, not compiled. Each place that works around a missing helper says so and is in GAPS.md.

use super::Game;
use horizon_client::sentinel::core::{
    self,
    npc::{NpcBridge, NpcId},
    view::{EntityView, GameMode, ItemView, NpcInfo, Vec3},
    world::{Projectile, World},
};
use horizon_client::mod_api::EquipmentSlot;

/// Plain words of a text component stored as NBT (`minecraft:lore` lines): a string, a compound
/// with `text` and `extra`, or a list of those.
fn plain_text(tag: &horizon_client::mod_api::Tag) -> String {
    use horizon_client::mod_api::Tag;
    match tag {
        Tag::String(s) => {
            let s = s.to_string_lossy();
            // Stored as JSON text by older data.
            match s.trim_start().starts_with('{').then(|| serde_json::from_str::<serde_json::Value>(&s).ok()).flatten() {
                Some(v) => json_text(&v),
                None => s.to_string(),
            }
        }
        Tag::Compound(m) => {
            let mut out = String::new();
            for (k, v) in m.iter() {
                if k.to_string_lossy() == "text" {
                    out.push_str(&plain_text(v));
                }
            }
            for (k, v) in m.iter() {
                if k.to_string_lossy() == "extra" {
                    out.push_str(&plain_text(v));
                }
            }
            out
        }
        Tag::List(l) => l.iter().map(plain_text).collect(),
        _ => String::new(),
    }
}

fn json_text(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(a) => a.iter().map(json_text).collect(),
        serde_json::Value::Object(o) => {
            let mut s = o.get("text").map(json_text).unwrap_or_default();
            if let Some(extra) = o.get("extra") {
                s.push_str(&json_text(extra));
            }
            s
        }
        _ => String::new(),
    }
}

fn item_view(game: &Game<'_>, stack: &horizon_client::items::ItemStack) -> ItemView {
    let mut v = ItemView::of(&stack.item);
    // `minecraft:lore` is a list of text components (`Host::component`, no `lore_plain` needed).
    if let Ok(Some(horizon_client::mod_api::Tag::List(lines))) = game.native_component(stack, "minecraft:lore") {
        v.lore = lines.iter().map(plain_text).collect();
    }
    v.display_name = stack.custom_name();
    v.sharpness = stack.enchantment_level("minecraft:sharpness");
    v.power = stack.enchantment_level("minecraft:power");
    v
}

impl Game<'_> {
    fn sentinel_living_items(&self, id: u32) -> (Option<ItemView>, Option<ItemView>, [Option<ItemView>; 4]) {
        let Some(living) = self.native_entity(id).and_then(|e| self.native_living(e)) else {
            return (None, None, Default::default());
        };
        let slot = |s: EquipmentSlot| self.native_living_equipment(living, s).as_ref().map(|i| item_view(self, i));
        (
            slot(EquipmentSlot::MainHand),
            slot(EquipmentSlot::OffHand),
            [slot(EquipmentSlot::Head), slot(EquipmentSlot::Chest), slot(EquipmentSlot::Legs), slot(EquipmentSlot::Feet)],
        )
    }

    /// Every living entity of the level in place and every player, as the core sees them.
    pub(in crate::game) fn sentinel_views(&mut self, bridge: &dyn NpcBridge) -> Vec<EntityView> {
        let dim = self.dim.id().to_owned();
        let mut out = Vec::new();
        let ids: Vec<u32> = self.mobs.iter().filter(|m| !m.removed && m.alive()).map(|m| m.id).collect();
        for id in ids {
            let uuid = self.native_entity(id).and_then(|e| self.native_entity_uuid(e)).unwrap_or(0);
            let Some(m) = self.mobs.iter().find(|m| m.id == id) else { continue };
            let pos = [m.position.x, m.position.y, m.position.z];
            let eye = [m.position.x, m.eye_y(), m.position.z];
            let npc = bridge.npc_of_entity(id as i32).map(|n| NpcInfo {
                id: n,
                name: bridge.name(n).unwrap_or_default(),
                sentinel: core::registry().is_sentinel(n),
            });
            let mut v = EntityView {
                id: id as i32,
                uuid,
                kind: horizon_client::command::args::normalize(&m.kind.id),
                dimension: dim.clone(),
                pos,
                eye,
                yaw: m.head_yaw,
                pitch: m.head_pitch,
                velocity: [m.velocity.x, m.velocity.y, m.velocity.z],
                alive: true,
                health: f64::from(m.health),
                max_health: f64::from(m.attrs.max_health),
                custom_name: m.custom_name.clone(),
                npc,
                mob_has_target: Some(m.ai.target.is_some()),
                // UNVERIFIED: the mob's effect list; GAP for a typed `has_effect` on mobs.
                invisible_effect: m.invisible,
                glowing: m.glowing,
                ..Default::default()
            };
            let (main, off, armor) = self.sentinel_living_items(id);
            v.main_hand = main;
            v.off_hand = off;
            v.armor = armor;
            out.push(v);
        }
        for p in self.native_server_players() {
            let main_off_armor = if p.entity_id > 0 { self.sentinel_living_items(p.entity_id as u32) } else { (None, None, Default::default()) };
            let npc = bridge.npc_of_entity(p.entity_id).map(|n| NpcInfo {
                id: n,
                name: bridge.name(n).unwrap_or_default(),
                sentinel: core::registry().is_sentinel(n),
            });
            out.push(EntityView {
                id: p.entity_id,
                uuid: p.uuid,
                kind: "minecraft:player".into(),
                dimension: p.dimension.clone(),
                pos: p.position,
                // `NativeServerPlayer.eye_height`: 1.62 standing, 1.27 sneaking, less swimming.
                eye: [p.position[0], p.position[1] + f64::from(p.eye_height), p.position[2]],
                yaw: p.yaw,
                pitch: p.pitch,
                alive: p.health > 0.0,
                health: f64::from(p.health),
                max_health: 20.0,
                player_name: Some(p.name.clone()),
                game_mode: match p.game_mode {
                    1 => GameMode::Creative,
                    2 => GameMode::Adventure,
                    3 => GameMode::Spectator,
                    _ => GameMode::Survival,
                },
                npc,
                // A player's storage slots by UUID (`in_inventory:` targets); none while their
                // state is swapped in.
                inventory: self
                    .native_player_stacks(p.uuid)
                    .map(|slots| slots.iter().flatten().map(|i| item_view(self, i)).collect())
                    .unwrap_or_default(),
                main_hand: main_off_armor.0,
                off_hand: main_off_armor.1,
                armor: main_off_armor.2,
                ..Default::default()
            });
        }
        horizon_client::sentinel::cache_views(&dim, &out);
        out
    }

    /// The LAN hub connection of a player, for binding a mob's `Who::Player` to them.
    fn sentinel_guest_conn(&self, uuid: u128) -> Option<u32> {
        // UNVERIFIED: `Hub.guests` and `Guest.conn`.
        self.lan.as_ref()?.hub.guests.iter().find(|g| g.uuid == uuid).map(|g| g.conn)
    }
}

/// One Sentinel body in the level in place.
pub(in crate::game) struct Body<'g, 'a> {
    pub game: &'g mut Game<'a>,
    pub npc: NpcId,
    pub mob: u32,
    pub dim: String,
    pub views: &'g [EntityView],
    pub bridge: &'g mut dyn NpcBridge,
    pub random: u64,
    /// Whether the mob walks a path (`native_mob_navigating` at the start of its tick, kept up
    /// to date by this tick's own navigate and stop calls).
    pub navigating: bool,
}

impl Body<'_, '_> {
    fn uuid_arg(u: u128) -> String {
        core::list::format_uuid(u)
    }
}

impl World for Body<'_, '_> {
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
        let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if len < 1e-6 {
            return true;
        }
        let dir = [d[0] / len, d[1] / len, d[2] / len];
        if !core::registry().cfg.random.enhance_los_traces {
            return self.game.clip_ray(from, dir, len).is_none();
        }
        // `enhance los traces`: Sentinel's own ray, which lets glass, leaves and bars pass. The
        // blocks on the ray come from `native_collision_boxes`; a block that has a shape and is
        // not see-through blocks (partial: see-through is judged by name, GAPS.md).
        let level = self.game.native_level(&self.dim);
        let mut last = [i32::MIN; 3];
        let steps = (len / 0.2).ceil() as u32;
        for i in 1..steps {
            let t = f64::from(i) * 0.2;
            let p = [from[0] + dir[0] * t, from[1] + dir[1] * t, from[2] + dir[2] * t];
            let b = [p[0].floor() as i32, p[1].floor() as i32, p[2].floor() as i32];
            if b == last {
                continue;
            }
            last = b;
            let cell = [
                f64::from(b[0]), f64::from(b[1]), f64::from(b[2]),
                f64::from(b[0]) + 1.0, f64::from(b[1]) + 1.0, f64::from(b[2]) + 1.0,
            ];
            let solid = self.game.native_collision_boxes(&self.dim, cell).iter().any(|c| {
                p[0] >= c[0] && p[0] <= c[3] && p[1] >= c[1] && p[1] <= c[4] && p[2] >= c[2] && p[2] <= c[5]
            });
            if !solid {
                continue;
            }
            let see_through = level
                .and_then(|l| self.game.native_level_block(l, b))
                .is_some_and(|name| ["glass", "leaves", "iron_bars"].iter().any(|k| name.contains(k)));
            if !see_through {
                return false;
            }
        }
        true
    }
    fn min_y(&self) -> f64 {
        f64::from(self.game.world.bottom_y())
    }
    fn random(&mut self) -> f64 {
        // xorshift: the core only needs spread, not vanilla's sequence.
        self.random ^= self.random << 13;
        self.random ^= self.random >> 7;
        self.random ^= self.random << 17;
        (self.random >> 11) as f64 / (1u64 << 53) as f64
    }
    fn set_max_health(&mut self, max: f64) {
        // GAP: no helper sets a mob's max health attribute; the field is written directly.
        if let Some(m) = self.game.mobs.iter_mut().find(|m| m.id == self.mob) {
            m.attrs.max_health = max as f32;
            if m.health > m.attrs.max_health {
                m.health = m.attrs.max_health;
            }
            m.needs_sync = true;
        }
    }
    fn set_health(&mut self, health: f64) {
        let dim = self.dim.clone();
        self.game.native_set_mob_health(&dim, self.mob, health as f32);
    }
    fn hurt_self(&mut self, amount: f64) {
        let dim = self.dim.clone();
        self.game.native_hurt_entity(&dim, self.mob, amount as f32, "minecraft:out_of_world", None);
    }
    fn teleport_self(&mut self, to: Vec3) {
        let dim = self.dim.clone();
        self.game.native_teleport_mob(&dim, self.mob, to);
    }
    fn stop_motion(&mut self) {
        if let Some(m) = self.game.mobs.iter_mut().find(|m| m.id == self.mob) {
            m.velocity.x = 0.0;
            m.velocity.y = 0.0;
            m.velocity.z = 0.0;
            m.needs_sync = true;
        }
    }
    // hooks-b-all NPC helpers (`native_mob_navigate`/`follow`/`stop`/`navigating`/`look_at`,
    // API npc_mob_navigation): the engine's own pathfinding, for any loaded level. The brain asks
    // again every tick, as the docs say a library must (the mob's goals may take over otherwise).
    fn navigate_to(&mut self, to: Vec3, speed: f64, margin: f64) {
        let dim = self.dim.clone();
        let reach = margin.max(0.0).ceil() as i32;
        self.navigating = self.game.native_mob_navigate(&dim, self.mob, to, reach, speed);
    }
    fn navigate_to_entity(&mut self, target: &EntityView, speed: f64) {
        let dim = self.dim.clone();
        if let Ok(id) = u32::try_from(target.id) {
            // A player by their entity id: no more binding the mob to the target's connection.
            self.navigating = self.game.native_mob_follow(&dim, self.mob, id, speed);
        }
    }
    fn cancel_navigation(&mut self) {
        let dim = self.dim.clone();
        self.game.native_mob_stop(&dim, self.mob);
        self.navigating = false;
    }
    fn is_navigating(&self) -> bool {
        self.navigating
    }
    fn face(&mut self, at: Vec3) {
        let dim = self.dim.clone();
        self.game.native_mob_look_at(&dim, self.mob, at);
    }
    fn swing(&mut self) {
        // `do_hurt_target` swings; Sentinel's own damage path needs the arm too.
        // GAP: `native_swing(dimension, entity, hand)` (ClientboundAnimatePacket).
    }
    fn set_blocking(&mut self, blocking: bool) {
        // GAP: raising a shield needs `native_living_use_item`; the block counts in the damage rule only.
        let _ = blocking;
    }
    fn native_attack(&mut self, target: &EntityView) -> bool {
        // hooks-b-all `native_mob_attack` (API npc_mob_attack): the mob's own `doHurtTarget`,
        // weapon, enchantments and knockback; the damage hooks hear it.
        let Ok(entity) = u32::try_from(target.id) else { return false };
        let dim = self.dim.clone();
        self.game.native_mob_look_at(&dim, self.mob, target.eye);
        self.game.native_mob_attack(&dim, self.mob, entity)
    }
    fn hurt(&mut self, target: &EntityView, amount: f64) {
        // hooks-b-all `native_hurt_entity` (API hurt_entity): Sentinel's own damage as the NPC's
        // `mob_attack`, without a `/damage` line and a UUID lookup.
        let Ok(entity) = u32::try_from(target.id) else { return };
        let dim = self.dim.clone();
        self.game.native_hurt_entity(&dim, entity, amount as f32, "minecraft:mob_attack", Some(self.mob));
    }
    fn push(&mut self, target: &EntityView, velocity: Vec3) {
        let dim = self.dim.clone();
        if target.kind == "minecraft:player" && target.npc.is_none() {
            // UNVERIFIED: `native_push_player` takes the acting player; GAP: push by UUID.
            return;
        }
        let v = [velocity[0] - target.velocity[0], velocity[1] - target.velocity[1], velocity[2] - target.velocity[2]];
        // `Host::entity_push` (`Entity.push`): not for a player, whose client moves it.
        if let Some(entity) = self.game.native_entity(target.id as u32) {
            self.game.native_entity_push(entity, v);
        }
        let _ = dim;
    }
    fn launch(&mut self, kind: Projectile, from: Vec3, velocity: Vec3, item: Option<&str>) {
        use horizon_client::mod_api::{Compound, Tag};
        let me = self.views.iter().find(|e| e.id == self.mob as i32).map(|e| e.uuid).unwrap_or(0);
        let mut nbt = Compound::new();
        nbt.insert("Motion".into(), Tag::List(velocity.iter().map(|v| Tag::Double(v.to_bits())).collect()));
        // `Owner` as an int array UUID, as vanilla saves it: hits count as the NPC's.
        let owner = [(me >> 96) as i32, (me >> 64) as i32, (me >> 32) as i32, me as i32];
        nbt.insert("Owner".into(), Tag::IntArray(owner.to_vec()));
        if let (Projectile::Potion, Some(id)) = (kind, item) {
            // UNVERIFIED: the potion's contents are not copied (GAP: stack to compound).
            let mut stack = Compound::new();
            stack.insert("id".into(), Tag::String(id.into()));
            stack.insert("count".into(), Tag::Int(1));
            nbt.insert("Item".into(), Tag::Compound(stack));
        }
        if kind == Projectile::Arrow {
            // Picked up by nobody, as Sentinel's arrows.
            nbt.insert("pickup".into(), Tag::Byte(0));
        }
        let dim = self.dim.clone();
        let id = self.game.native_summon(&dim, kind.kind(), from, nbt);
        // `arrow cleanup time`: the arrow is removed after that many ticks (`Host::remove_entity`
        // takes projectiles since API remove_projectiles).
        let ticks = core::registry().cfg.random.arrow_cleanup_time;
        if let (Projectile::Arrow, Some(id), true) = (kind, id, ticks > 0) {
            let now = self.game.native_level(&dim).and_then(|l| self.game.native_level_game_time(l)).unwrap_or(0);
            ARROWS.lock().unwrap_or_else(|e| e.into_inner()).push((dim, id, now as i64 + ticks));
        }
    }
    fn lightning_effect(&mut self, at: Vec3) {
        let dim = self.dim.clone();
        let mut nbt = horizon_client::mod_api::Compound::new();
        // GAP: a visual-only bolt (`strikeLightningEffect`); a summoned bolt sets fire and hurts.
        nbt.insert("Tags".into(), horizon_client::mod_api::Tag::List(vec![horizon_client::mod_api::Tag::String("sentinel_effect".into())]));
        self.game.native_summon(&dim, "minecraft:lightning_bolt", at, nbt);
    }
    fn make_glow(&mut self, target: &EntityView, ticks: i32) {
        self.game.native_run_command(&format!("effect give {} minecraft:glowing {} 1", Self::uuid_arg(target.uuid), (ticks / 20).max(1)));
    }
    fn play_sound(&mut self, sound: &str, at: Vec3) {
        // Source 5 = hostile (`SoundSource.HOSTILE`).
        // In the current level, as Horizon's level hook does (`native_play_sound_in_place`).
        self.game.native_play_sound_in_place(sound, at, 5, 1.0, 1.0);
    }
    fn tell(&mut self, player: u128, text: &str) {
        self.game.native_tell(player, horizon_client::text::Text::literal(text));
    }
    fn take_held_item(&mut self) -> bool {
        let Some(living) = self.game.native_entity(self.mob).and_then(|e| self.game.native_living(e)) else {
            return false;
        };
        let Some(mut stack) = self.game.native_living_equipment(living, EquipmentSlot::MainHand) else {
            return false;
        };
        stack.count -= 1;
        let next = (stack.count > 0).then_some(stack);
        self.game.native_living_set_equipment(living, EquipmentSlot::MainHand, next)
    }
    fn swap_held(&mut self, _pick: &dyn Fn(&str) -> bool) -> bool {
        // GAP: an NPC's own inventory (Citizens' `Inventory` trait) is the NPC library's; the
        // bridge has no access to it yet. Autoswitch needs it.
        false
    }
    fn waypoints(&self) -> Vec<Vec3> {
        self.bridge.waypoints(self.npc)
    }
    fn pause_waypoints(&mut self, paused: bool) {
        self.bridge.pause_waypoints(self.npc, paused);
    }
}

/// Arrows a Sentinel shot: level, entity id, the game time they are removed at.
static ARROWS: std::sync::Mutex<Vec<(String, u32, i64)>> = std::sync::Mutex::new(Vec::new());

/// Removes the arrows of `dimension` whose time is up (`arrow cleanup time`).
pub(in crate::game) fn expire_arrows(game: &mut Game<'_>, dimension: &str) {
    if ARROWS.lock().unwrap_or_else(|e| e.into_inner()).is_empty() {
        return;
    }
    let now = game.native_level(dimension).and_then(|l| game.native_level_game_time(l)).unwrap_or(0) as i64;
    let due: Vec<u32> = {
        let mut a = ARROWS.lock().unwrap_or_else(|e| e.into_inner());
        let mut due = Vec::new();
        a.retain(|(dim, id, at)| {
            if dim == dimension && *at <= now {
                due.push(*id);
                false
            } else {
                true
            }
        });
        due
    };
    for id in due {
        game.native_remove_entity(dimension, id);
    }
}

/// A Sentinel's drop as a stack, with its custom name, lore and enchantments (the stack read as
/// `ItemStack.CODEC` through `item_from_nbt`); the plain stack when that fails.
pub(in crate::game) fn drop_stack(d: &core::data::DropItem) -> horizon_client::items::ItemStack {
    use horizon_client::mod_api::{Compound, Tag};
    let mut stack = Compound::new();
    stack.insert("id".into(), Tag::String(d.id.as_str().into()));
    stack.insert("count".into(), Tag::Int(d.count as i32));
    let mut components = Compound::new();
    if let Some(name) = &d.name {
        components.insert("minecraft:custom_name".into(), Tag::String(name.as_str().into()));
    }
    if !d.lore.is_empty() {
        components.insert(
            "minecraft:lore".into(),
            Tag::List(d.lore.iter().map(|l| Tag::String(l.as_str().into())).collect()),
        );
    }
    if !d.enchants.is_empty() {
        let mut levels = Compound::new();
        for (key, level) in &d.enchants {
            levels.insert(key.as_str().into(), Tag::Int(*level));
        }
        components.insert("minecraft:enchantments".into(), Tag::Compound(levels));
    }
    if !components.is_empty() {
        stack.insert("components".into(), Tag::Compound(components));
    }
    horizon_items::item_nbt::item_from_nbt(&Tag::Compound(stack)).unwrap_or(horizon_client::items::ItemStack {
        item: d.id.as_str().into(),
        count: d.count,
        damage: 0,
        extra: None,
    })
}

/// Death outcomes waiting for the dying entity's loot roll (death.rs -> loot.rs).
static DEATHS: std::sync::Mutex<Vec<(u32, core::registry::DeathOutcome)>> = std::sync::Mutex::new(Vec::new());

pub(in crate::game) fn note_death(entity: u32, outcome: core::registry::DeathOutcome) {
    let mut d = DEATHS.lock().unwrap_or_else(|e| e.into_inner());
    d.retain(|(e, _)| *e != entity);
    d.push((entity, outcome));
    // A death whose loot never rolled (removed without drops) must not pile up.
    if d.len() > 64 {
        d.remove(0);
    }
}

pub(in crate::game) fn take_death(entity: u32) -> Option<core::registry::DeathOutcome> {
    let mut d = DEATHS.lock().unwrap_or_else(|e| e.into_inner());
    let i = d.iter().position(|(e, _)| *e == entity)?;
    Some(d.remove(i).1)
}

/// What `/sentinel` may ask the game.
struct Look<'g, 'a> {
    game: &'g Game<'a>,
    bridge: &'g dyn NpcBridge,
}

impl core::commands::Lookup for Look<'_, '_> {
    fn body(&self, npc: NpcId) -> Option<EntityView> {
        let id = self.bridge.entity_of(npc)?;
        horizon_client::sentinel::view(id)
    }
    fn player_uuid(&self, name: &str) -> Option<u128> {
        self.game.native_server_players().into_iter().find(|p| p.name.eq_ignore_ascii_case(name)).map(|p| p.uuid)
    }
    fn player_name(&self, uuid: u128) -> Option<String> {
        self.game.native_server_players().into_iter().find(|p| p.uuid == uuid).map(|p| p.name)
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
    fn grants(&self, player: u128, node: &str, default_level: u8) -> Option<bool> {
        // Horizon's answer: a permissions mod's if one is loaded, else the operator level.
        Some(self.game.native_has_permission(player, node, default_level))
    }
    fn world_name(&self, dimension: &str) -> String {
        // Paper names the overworld after `level-name` and the others `<level>_nether` /
        // `<level>_the_end`. GAP: the level name; `world` is Paper's default.
        let level = self.game.native_world_dir().and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned())).unwrap_or_else(|| "world".into());
        match dimension {
            "minecraft:overworld" => level,
            "minecraft:the_nether" => format!("{level}_nether"),
            "minecraft:the_end" => format!("{level}_the_end"),
            other => other.split_once(':').map_or(other, |(_, p)| p).to_owned(),
        }
    }
}

impl Game<'_> {
    /// Runs `/sentinel <args>` for a player (or the console with `player` None) and carries out
    /// what it asks; returns the lines to show.
    pub(in crate::game) fn sentinel_command(&mut self, player: Option<u128>, level: u8, args: &str) -> Vec<String> {
        use core::commands::{run, Effect, Sender};
        let out = horizon_client::sentinel::with_bridge(|bridge| {
            let selected = player.and_then(|p| bridge.selected(p));
            let sender = Sender { player, op_level: level, selected };
            let out = {
                let look = Look { game: &*self, bridge: &*bridge };
                run(&mut core::registry(), &sender, args, &look)
            };
            for effect in &out.effects {
                match effect {
                    Effect::Dirty(npc) => bridge.mark_dirty(*npc),
                    Effect::Protected(npc, protected) => bridge.set_protected(*npc, *protected),
                    Effect::Health(npc, h) => {
                        if let (Some(id), Some(dim)) = (bridge.entity_of(*npc), bridge.dimension_of(*npc)) {
                            if let Some(m) = self.mobs.iter_mut().find(|m| m.id == id as u32) {
                                m.attrs.max_health = *h as f32;
                                m.health = *h as f32;
                                m.needs_sync = true;
                            }
                            let _ = dim;
                        }
                    }
                    Effect::Kill(npc) => {
                        if let (Some(id), Some(dim)) = (bridge.entity_of(*npc), bridge.dimension_of(*npc)) {
                            // The death hook then runs Sentinel's death handling.
                            self.native_kill_mob(&dim, id as u32);
                        }
                    }
                    Effect::Respawn(npc) => {
                        let at = core::registry().respawn_point(*npc, &*bridge);
                        bridge.spawn(*npc, at.as_ref());
                    }
                    Effect::Reload => {
                        // The config file is read again on its own within a second of a save.
                    }
                }
            }
            out.lines
        });
        out.unwrap_or_else(|| vec!["\u{a7}2[Sentinel] \u{a7}cNo NPC library is loaded (Sentinel needs Citizens).".into()])
    }
}
