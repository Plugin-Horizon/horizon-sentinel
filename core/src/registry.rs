//! Every Sentinel of the server (`SentinelPlugin.currentSentinelNPCs` and
//! `SentinelEventHandler`): their settings and state, the updates, and what happens on hits,
//! deaths and chat. The mod's helpers drive it from Horizon's hooks.

use crate::brain::{Ctx, Runtime};
use crate::combat::{self, HitDecision};
use crate::config::Config;
use crate::data::{DropItem, Location, SentinelData};
use crate::list::Asker;
use crate::node::Node;
use crate::npc::{NpcBridge, NpcId};
use crate::view::EntityView;
use crate::world::World;
use std::collections::BTreeMap;

/// One NPC with the Sentinel trait.
#[derive(Clone, Debug)]
pub struct Sentinel {
    pub npc: NpcId,
    pub data: SentinelData,
    pub rt: Runtime,
    /// The body's UUID while spawned (`lastEntityUUID`).
    pub body: Option<u128>,
}

/// A hit the damage hook saw, to be checked at the next update (event targets need sight).
#[derive(Clone, Debug)]
struct PendingEvent {
    damager: EntityView,
    victim: EntityView,
}

/// What to do with a death.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeathOutcome {
    /// Replace the dead entity's drops with `drops` (a Sentinel), or clear them (an enemy of a
    /// Sentinel with `enemy drops` off).
    pub replace_drops: bool,
    pub drops: Vec<DropItem>,
    /// Experience to drop instead of vanilla's.
    pub xp: Option<i64>,
    /// Drop the death message (`death messages: false`).
    pub hide_death_message: bool,
}

#[derive(Default)]
pub struct Registry {
    pub cfg: Config,
    pub sentinels: BTreeMap<NpcId, Sentinel>,
    events: Vec<PendingEvent>,
    chats: Vec<(EntityView, String)>,
    pub debug: bool,
    /// Lines for players that the host sends at the next level tick (the damage hook has no host).
    pub notices: Vec<(u128, String)>,
}

impl Registry {
    pub fn new(cfg: Config) -> Self {
        Registry { cfg, ..Default::default() }
    }

    // ----- trait lifecycle (called by the NPC library) -----

    /// `onAttach`: `/trait sentinel` on an NPC.
    pub fn attach(&mut self, npc: NpcId) -> &mut Sentinel {
        let data = SentinelData::from_defaults(&self.cfg);
        self.sentinels.entry(npc).or_insert(Sentinel { npc, data, rt: Runtime::new(), body: None })
    }

    /// `load`: the trait subtree from the NPC's saves.
    pub fn load(&mut self, npc: NpcId, node: &Node) {
        let data = SentinelData::load(node, &self.cfg);
        self.sentinels.insert(npc, Sentinel { npc, data, rt: Runtime::new(), body: None });
    }

    /// `save`.
    pub fn save(&self, npc: NpcId) -> Option<Node> {
        self.sentinels.get(&npc).map(|s| s.data.save(&self.cfg))
    }

    /// `onRemove`: the trait is taken off (`/trait remove sentinel`).
    pub fn remove(&mut self, npc: NpcId, bridge: &mut dyn NpcBridge) {
        if let Some(s) = self.sentinels.remove(&npc) {
            if !s.data.invincible {
                bridge.set_protected(npc, true);
            }
        }
    }

    /// `onSpawn`: a new body for the NPC. Returns the health to give it and whether Citizens
    /// should protect it (`invincible`); None when it is no Sentinel. The level tick also calls
    /// this when it sees a body it doesn't know, so a library that never calls it still works.
    pub fn spawned(&mut self, npc: NpcId, body: &EntityView) -> Option<(f64, bool)> {
        let s = self.sentinels.get_mut(&npc)?;
        s.body = Some(body.uuid);
        s.data.stats.times_spawned += 1;
        s.rt.blocking = false;
        s.rt.respawn_in = None;
        s.rt.reset_fight();
        Some((s.data.health, s.data.invincible))
    }

    /// Whether `body` is a body the Sentinel doesn't know yet (it respawned or was spawned).
    pub fn is_new_body(&self, npc: NpcId, body: u128) -> bool {
        self.sentinels.get(&npc).is_some_and(|s| s.body != Some(body))
    }

    /// `onDespawn`.
    pub fn despawned(&mut self, npc: NpcId) {
        if let Some(s) = self.sentinels.get_mut(&npc) {
            s.rt.targets.clear();
            s.rt.avoids.clear();
            s.rt.chasing = None;
        }
    }

    pub fn is_sentinel(&self, npc: NpcId) -> bool {
        self.sentinels.contains_key(&npc)
    }

    fn by_body(&self, uuid: u128) -> Option<NpcId> {
        self.sentinels.values().find(|s| s.body == Some(uuid)).map(|s| s.npc)
    }

    fn asker(&self, owner: Option<u128>, nearby: &[EntityView]) -> Asker {
        let mut squads = std::collections::HashMap::new();
        for e in nearby {
            if let Some(n) = e.npc.as_ref().and_then(|n| self.sentinels.get(&n.id)) {
                if let Some(q) = &n.data.squad {
                    squads.insert(e.id, q.clone());
                }
            }
        }
        Asker { owner, squads }
    }

    // ----- ticking -----

    /// Counts one tick. With a `dimension` (a level's tick): `stats_ticksSpawned` of the
    /// Sentinels spawned there, and the ones due for an update now (`update rate`). Without (the
    /// server's tick): the respawn timers of dead Sentinels.
    pub fn tick(&mut self, bridge: &mut dyn NpcBridge, dimension: Option<&str>) -> Vec<NpcId> {
        let rate = self.cfg.update_rate.max(1);
        let mut due = Vec::new();
        let mut respawn = Vec::new();
        for s in self.sentinels.values_mut() {
            let spawned = bridge.is_spawned(s.npc);
            if let Some(dim) = dimension {
                if !spawned || bridge.dimension_of(s.npc).as_deref() != Some(dim) {
                    continue;
                }
                s.data.stats.ticks_spawned += 1;
                s.rt.ticks_until_update -= 1;
                if s.rt.ticks_until_update <= 0 {
                    s.rt.ticks_until_update = rate;
                    due.push(s.npc);
                }
            } else if spawned {
                continue;
            } else if let Some(t) = s.rt.respawn_in.as_mut() {
                // Sentinel checks every 10 ticks; counting each tick is the same within 10.
                *t -= 1;
                if *t <= 0 {
                    s.rt.respawn_in = None;
                    respawn.push((s.npc, s.data.spawn_point.clone()));
                }
            }
        }
        for (npc, at) in respawn {
            let at = at.or_else(|| bridge.stored_location(npc));
            if at.is_some() {
                bridge.spawn(npc, at.as_ref());
            }
        }
        due
    }

    /// `runUpdate` of one Sentinel whose body is `me`; `guarded` is the guarded player or NPC
    /// when the host found it. Hands targets on to squad mates afterwards.
    pub fn update(&mut self, npc: NpcId, me: &EntityView, guarded: Option<EntityView>, owner: Option<u128>, w: &mut dyn World) {
        let nearby = w.nearby(me.pos, self.cfg.defaults.reach.max(20.0));
        let asker = self.asker(owner, &nearby);
        let events = std::mem::take(&mut self.events);
        let chats = std::mem::take(&mut self.chats);
        let Some(s) = self.sentinels.get_mut(&npc) else {
            self.events = events;
            self.chats = chats;
            return;
        };
        s.body = Some(me.uuid);
        let mut cx = Ctx { data: &mut s.data, cfg: &self.cfg, me, asker: &asker, guarded };
        // Hits and chat since the last update that make someone a target or an avoid.
        for ev in &events {
            if ev.damager.uuid == me.uuid {
                continue;
            }
            let sees = s.rt.can_see(&cx, &ev.damager, w) && !s.rt.is_ignored(&cx, &ev.damager);
            if sees && cx.data.all_targets.is_damage_event_target(&ev.damager, &ev.victim, &asker) {
                s.rt.add_target(&cx, ev.damager.uuid);
            }
            if sees && cx.data.all_avoids.is_damage_event_target(&ev.damager, &ev.victim, &asker) {
                s.rt.add_avoid(&cx, ev.damager.uuid);
            }
            let guarding = cx.data.guarding().or(cx.guarded.as_ref().map(|g| g.uuid));
            let victim_seen = s.rt.can_see(&cx, &ev.victim, w) && !s.rt.is_ignored(&cx, &ev.victim);
            if victim_seen && cx.data.all_targets.is_reverse_event_target(ev.damager.uuid, guarding) {
                s.rt.add_target(&cx, ev.victim.uuid);
            }
            if victim_seen && cx.data.all_avoids.is_reverse_event_target(ev.damager.uuid, guarding) {
                s.rt.add_avoid(&cx, ev.victim.uuid);
            }
        }
        for (player, message) in &chats {
            if !s.rt.can_see(&cx, player, w) {
                continue;
            }
            if cx.data.all_targets.is_message_target(message) && !s.rt.is_ignored(&cx, player) {
                s.rt.add_target(&cx, player.uuid);
            }
            if cx.data.all_avoids.is_message_target(message) && !s.rt.is_ignored(&cx, player) {
                s.rt.add_avoid(&cx, player.uuid);
            }
        }
        s.rt.update(&mut cx, w);
        let squad = s.data.squad.clone();
        let adds = std::mem::take(&mut s.rt.squad_adds);
        let removes = std::mem::take(&mut s.rt.squad_removes);
        self.events = events;
        self.chats = chats;
        if let Some(q) = squad {
            for mate in self.sentinels.values_mut() {
                if mate.npc == npc || mate.data.squad.as_deref() != Some(q.as_str()) {
                    continue;
                }
                for u in &adds {
                    let t = mate.data.enemy_target_time;
                    let e = mate.rt.targets.entry(*u).or_insert(crate::brain::Current { ticks_left: t, has_los: true });
                    e.ticks_left = t;
                }
                for u in &removes {
                    mate.rt.targets.remove(u);
                }
            }
        }
    }

    /// Clears the queued hits and chat of `dimension` once its due Sentinels have seen them
    /// (call after a level's updates); other levels keep theirs for their own updates.
    pub fn end_tick(&mut self, dimension: &str, any_updated: bool) {
        if any_updated {
            self.events.retain(|e| e.victim.dimension != dimension);
            self.chats.retain(|(p, _)| p.dimension != dimension);
        }
        // Without an update they wait, but not forever.
        if self.events.len() > 256 {
            self.events.drain(..128);
        }
        if self.chats.len() > 256 {
            self.chats.drain(..128);
        }
    }

    // ----- events -----

    /// A living entity is about to be hurt (`EntityDamageEvent` / `EntityDamageByEntityEvent` at
    /// all priorities): the decision on the amount, and Sentinel's bookkeeping (stats,
    /// fightback, run away, safe shot, drops tracking). `attacker` is the causing entity (the
    /// shooter for a projectile).
    pub fn on_hurt(
        &mut self,
        victim: &EntityView,
        attacker: Option<&EntityView>,
        damage_type: &str,
        amount: f32,
        bridge: &dyn NpcBridge,
    ) -> HitDecision {
        let victim_s = victim.npc.as_ref().map(|n| n.id).filter(|id| self.sentinels.contains_key(id));
        let attacker_s = attacker.and_then(|a| a.npc.as_ref()).map(|n| n.id).filter(|id| self.sentinels.contains_key(id));
        if let (Some(v), Some(a)) = (victim_s, attacker_s) {
            if v == a {
                return HitDecision::Cancel;
            }
        }
        if let Some(a) = attacker {
            if a.uuid != victim.uuid {
                self.events.push(PendingEvent { damager: a.clone(), victim: victim.clone() });
            }
        }
        let mut amount = amount;
        let mut decision = HitDecision::Pass;
        // The attacker is a Sentinel: safe shot, damage override, stats.
        if let (Some(a_id), Some(att)) = (attacker_s, attacker) {
            let asker = Asker { owner: bridge.owner(a_id), ..Default::default() };
            let cfg = &self.cfg;
            let s = self.sentinels.get_mut(&a_id).expect("checked");
            let cx = Ctx { data: &mut s.data, cfg, me: att, asker: &asker, guarded: None };
            if cx.data.safe_shot && !s.rt.should_target(&cx, victim) && !s.rt.targets.contains_key(&victim.uuid) {
                return HitDecision::Cancel;
            }
            let projectile = damage_type.contains("arrow") || damage_type.contains("trident") || damage_type.contains("fireball");
            if s.data.damage >= 0.0 {
                let d = combat::damage(&mut s.data, projectile, att.main_hand.as_ref()) as f32;
                amount = d;
                decision = HitDecision::Amount(d);
            }
            s.data.stats.damage_given += f64::from(amount);
            if !s.data.enemy_drops && !victim.is_player() {
                s.rt.needs_drops_clear.insert(victim.uuid);
            }
        }
        // The victim is a Sentinel.
        if let Some(v_id) = victim_s {
            let r = &self.cfg.random;
            let protect = r.protect_from_ignores;
            let asker = Asker { owner: bridge.owner(v_id), ..Default::default() };
            let cfg = &self.cfg;
            let s = self.sentinels.get_mut(&v_id).expect("checked");
            let cx = Ctx { data: &mut s.data, cfg, me: victim, asker: &asker, guarded: None };
            // `whenAttacksAreHappeningToMe`'s one-time notice: the owner who hits a freshly made
            // NPC they still have selected, while its only ignore is `owner`.
            if let Some(att) = attacker {
                if att.is_player()
                    && !s.rt.shown_init_warn
                    && cx.data.stats.ticks_spawned < 20 * 60 * 5
                    && cx.data.fightback
                    && bridge.owner(v_id) == Some(att.uuid)
                    && bridge.selected(att.uuid) == Some(v_id)
                {
                    s.rt.shown_init_warn = true;
                    if cx.data.all_ignores.total() == 1 && cx.data.all_ignores.targets.iter().any(|t| t == crate::target::OWNER) {
                        self.notices.push((
                            att.uuid,
                            "\u{a7}2[Sentinel] \u{a7}eNotice: if this is your first time setting up this NPC, don't forget to use \u{a7}b/sentinel removeignore owner\u{a7}e once you're done configuring the NPC, if you want it to be able to attack you.".to_owned(),
                        ));
                    }
                }
            }
            if let Some(att) = attacker {
                if (protect || cx.data.protect_from_ignores) && s.rt.is_ignored(&cx, att) {
                    return HitDecision::Cancel;
                }
                if cx.data.protect_from_range > 0.0 {
                    let r2 = cx.data.protect_from_range * cx.data.protect_from_range;
                    if att.dimension != victim.dimension || att.distance_sq(victim.pos) > r2 {
                        return HitDecision::Cancel;
                    }
                }
                if cx.data.guarding() == Some(att.uuid) {
                    if cfg.random.no_guard_damage {
                        return HitDecision::Cancel;
                    }
                    return decision;
                }
            }
            let blocked = attacker.is_some_and(|a| {
                s.rt.blocking
                    && s.data.stats.ticks_spawned >= s.rt.shield_axe_cooldown
                    && combat::shield_blocks(true, victim, a)
            });
            if blocked {
                if let Some(a) = attacker {
                    if a.main_hand.as_ref().is_some_and(|i| combat::is_axe(&i.id)) && (a.sprinting || !a.is_player()) {
                        s.rt.shield_axe_cooldown = s.data.stats.ticks_spawned + 20 * 5;
                    }
                }
            }
            let mut burn = s.rt.ticks_since_burn;
            let d = combat::incoming_to_sentinel(&s.data, &victim.armor, damage_type, amount, &mut burn, blocked);
            s.rt.ticks_since_burn = burn;
            match d {
                HitDecision::Cancel => return HitDecision::Cancel,
                HitDecision::Amount(a) => {
                    amount = a;
                    decision = HitDecision::Amount(a);
                }
                HitDecision::Pass => {}
            }
            s.data.stats.damage_taken += f64::from(amount);
            if let Some(att) = attacker {
                if att.uuid != victim.uuid {
                    let cx = Ctx { data: &mut s.data, cfg, me: victim, asker: &asker, guarded: None };
                    if cx.data.runaway {
                        s.rt.add_avoid(&cx, att.uuid);
                    }
                    if cx.data.fightback && !s.rt.is_ignored(&cx, att) {
                        s.rt.add_target(&cx, att.uuid);
                    }
                }
            }
        }
        // A Sentinel guarding the victim fights back for it.
        if let Some(att) = attacker {
            for s in self.sentinels.values_mut() {
                if s.data.guarding() == Some(victim.uuid) && s.data.fightback && att.uuid != s.body.unwrap_or(0) {
                    let t = s.data.enemy_target_time;
                    let e = s.rt.targets.entry(att.uuid).or_insert(crate::brain::Current { ticks_left: t, has_los: true });
                    e.ticks_left = t;
                }
            }
        }
        decision
    }

    /// The knockback a hit on a Sentinel gives: none with `allow knockback` off.
    pub fn allows_knockback(&self, victim: &EntityView) -> bool {
        victim
            .npc
            .as_ref()
            .and_then(|n| self.sentinels.get(&n.id))
            .is_none_or(|s| s.data.allow_knockback)
    }

    /// A living entity died (`EntityDeathEvent`): a Sentinel's own death (drops, xp, respawn),
    /// or an enemy's (forgotten, drops cleared when tracked).
    pub fn on_death(&mut self, dead: &EntityView, bridge: &mut dyn NpcBridge, mut random: impl FnMut() -> f64) -> DeathOutcome {
        let mut out = DeathOutcome::default();
        for s in self.sentinels.values_mut() {
            s.rt.targets.remove(&dead.uuid);
            s.rt.avoids.remove(&dead.uuid);
            if s.rt.chasing == Some(dead.uuid) {
                s.rt.chasing = None;
            }
            if s.rt.needs_drops_clear.remove(&dead.uuid) && !dead.is_player() {
                out.replace_drops = true;
                out.xp = Some(0);
            }
        }
        let Some(npc) = dead.npc.as_ref().map(|n| n.id).filter(|id| self.sentinels.contains_key(id)) else {
            return out;
        };
        let s = self.sentinels.get_mut(&npc).expect("checked");
        out.replace_drops = true;
        out.hide_death_message = !self.cfg.random.death_messages;
        if !self.cfg.random.workaround_drops {
            out.drops = combat::roll_drops(&s.data, &mut random);
        }
        out.xp = Some(s.data.death_xp);
        if let Some(p) = &s.data.spawn_point {
            bridge.set_stored_location(npc, p);
        }
        s.rt.reset_fight();
        s.body = None;
        match s.data.respawn_time {
            t if t < 0 => bridge.destroy(npc),
            0 => bridge.set_spawned_flag(npc, false),
            t => s.rt.respawn_in = Some(t),
        }
        out
    }

    /// A player chatted (`AsyncPlayerChatEvent`): `event:message:` targets.
    pub fn on_chat(&mut self, player: &EntityView, message: &str) {
        if self.sentinels.values().any(|s| !s.data.all_targets.by_event.is_empty() || !s.data.all_avoids.by_event.is_empty()) {
            self.chats.push((player.clone(), message.to_owned()));
        }
    }

    /// Whether an entity is a Sentinel's body whose sunburn is blocked (`block sunburn`).
    pub fn blocks_sunburn(&self, entity: &EntityView) -> bool {
        self.cfg.random.block_sunburn && entity.npc.as_ref().is_some_and(|n| self.sentinels.contains_key(&n.id))
    }

    /// Whether a projectile's owner is a Sentinel (explosion block damage, fire, arrow cleanup).
    pub fn owns_projectile(&self, owner_uuid: u128) -> bool {
        self.by_body(owner_uuid).is_some()
    }

    /// The Sentinel and location for `/sentinel spawnpoint` etc.
    pub fn get_mut(&mut self, npc: NpcId) -> Option<&mut Sentinel> {
        self.sentinels.get_mut(&npc)
    }

    /// `/sentinel kill` and the death handling of `workaround bukkit events`: treat the NPC as
    /// dead without a death event.
    pub fn kill(&mut self, npc: NpcId, body: &EntityView, bridge: &mut dyn NpcBridge) -> DeathOutcome {
        let mut b = body.clone();
        if b.npc.is_none() {
            b.npc = Some(crate::view::NpcInfo { id: npc, name: bridge.name(npc).unwrap_or_default(), sentinel: true });
        }
        self.on_death(&b, bridge, || 0.0)
    }

    /// The respawn location of a dead Sentinel.
    pub fn respawn_point(&self, npc: NpcId, bridge: &dyn NpcBridge) -> Option<Location> {
        self.sentinels.get(&npc).and_then(|s| s.data.spawn_point.clone()).or_else(|| bridge.stored_location(npc))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::list::TargetLabel;
    use crate::view::NpcInfo;

    #[derive(Default)]
    pub struct Bridge {
        pub spawned: std::collections::HashSet<NpcId>,
        pub destroyed: Vec<NpcId>,
        pub flags: Vec<(NpcId, bool)>,
        pub spawns: Vec<NpcId>,
        pub stored: Option<Location>,
        pub selected: Option<NpcId>,
        pub dim: Option<String>,
    }

    impl NpcBridge for Bridge {
        fn npc_of_entity(&self, _: i32) -> Option<NpcId> {
            None
        }
        fn entity_of(&self, _: NpcId) -> Option<i32> {
            None
        }
        fn dimension_of(&self, _: NpcId) -> Option<String> {
            self.dim.clone()
        }
        fn name(&self, n: NpcId) -> Option<String> {
            Some(format!("npc{n}"))
        }
        fn owner(&self, _: NpcId) -> Option<u128> {
            Some(1000)
        }
        fn is_spawned(&self, n: NpcId) -> bool {
            self.spawned.contains(&n)
        }
        fn spawn(&mut self, n: NpcId, _: Option<&Location>) -> bool {
            self.spawns.push(n);
            self.spawned.insert(n);
            true
        }
        fn destroy(&mut self, n: NpcId) {
            self.destroyed.push(n);
        }
        fn set_spawned_flag(&mut self, n: NpcId, s: bool) {
            self.flags.push((n, s));
        }
        fn set_protected(&mut self, _: NpcId, _: bool) {}
        fn set_stored_location(&mut self, _: NpcId, at: &Location) {
            self.stored = Some(at.clone());
        }
        fn stored_location(&self, _: NpcId) -> Option<Location> {
            self.stored.clone().or_else(|| Some(Location { world: "world".into(), ..Default::default() }))
        }
        fn selected(&self, _: u128) -> Option<NpcId> {
            self.selected
        }
        fn waypoints(&self, _: NpcId) -> Vec<[f64; 3]> {
            Vec::new()
        }
        fn pause_waypoints(&mut self, _: NpcId, _: bool) {}
        fn mark_dirty(&mut self, _: NpcId) {}
    }

    fn npc_body(id: NpcId, uuid: u128) -> EntityView {
        EntityView {
            id,
            uuid,
            kind: "minecraft:mannequin".into(),
            dimension: "minecraft:overworld".into(),
            alive: true,
            npc: Some(NpcInfo { id, name: format!("npc{id}"), sentinel: true }),
            ..Default::default()
        }
    }

    fn player(uuid: u128) -> EntityView {
        EntityView {
            id: 500,
            uuid,
            kind: "minecraft:player".into(),
            dimension: "minecraft:overworld".into(),
            alive: true,
            player_name: Some("P".into()),
            ..Default::default()
        }
    }

    #[test]
    fn respawn_time_zero_stays_dead() {
        let mut reg = Registry::default();
        let mut b = Bridge::default();
        reg.load(1, &crate::node::parse_yaml("respawnTime: 0\n").unwrap());
        let out = reg.on_death(&npc_body(1, 11), &mut b, || 0.0);
        assert!(out.replace_drops && out.drops.is_empty());
        assert_eq!(b.flags, vec![(1, false)]);
        for _ in 0..200 {
            reg.tick(&mut b, None);
        }
        assert!(b.spawns.is_empty());
    }

    #[test]
    fn respawn_after_its_time() {
        let mut reg = Registry::default();
        let mut b = Bridge::default();
        reg.load(1, &crate::node::parse_yaml("respawnTime: 40\n").unwrap());
        reg.on_death(&npc_body(1, 11), &mut b, || 0.0);
        for _ in 0..39 {
            reg.tick(&mut b, None);
        }
        assert!(b.spawns.is_empty());
        reg.tick(&mut b, None);
        assert_eq!(b.spawns, vec![1]);
    }

    #[test]
    fn updates_every_update_rate_ticks_in_its_level() {
        let mut reg = Registry::default();
        let mut b = Bridge { dim: Some("minecraft:overworld".into()), ..Default::default() };
        reg.load(1, &Node::map());
        b.spawned.insert(1);
        let mut due = 0;
        for _ in 0..30 {
            due += reg.tick(&mut b, Some("minecraft:overworld")).len();
            assert!(reg.tick(&mut b, Some("minecraft:the_nether")).is_empty());
            assert!(reg.tick(&mut b, None).is_empty());
        }
        assert_eq!(due, 3);
        assert_eq!(reg.sentinels[&1].data.stats.ticks_spawned, 30);
    }

    #[test]
    fn negative_respawn_destroys() {
        let mut reg = Registry::default();
        let mut b = Bridge::default();
        reg.load(1, &crate::node::parse_yaml("respawnTime: -1\n").unwrap());
        reg.on_death(&npc_body(1, 11), &mut b, || 0.0);
        assert_eq!(b.destroyed, vec![1]);
    }

    #[test]
    fn protected_cancels_hits_from_ignored() {
        let mut reg = Registry::default();
        let b = Bridge::default();
        reg.load(1, &crate::node::parse_yaml("protected: true\n").unwrap());
        TargetLabel::parse("player:^P$").add_to(&mut reg.sentinels.get_mut(&1).unwrap().data.all_ignores);
        let me = npc_body(1, 11);
        assert_eq!(reg.on_hurt(&me, Some(&player(7)), "minecraft:player_attack", 3.0, &b), HitDecision::Cancel);
    }

    #[test]
    fn fightback_targets_the_attacker() {
        let mut reg = Registry::default();
        let b = Bridge::default();
        reg.load(1, &Node::map());
        let me = npc_body(1, 11);
        let d = reg.on_hurt(&me, Some(&player(7)), "minecraft:player_attack", 3.0, &b);
        assert_eq!(d, HitDecision::Pass);
        let s = &reg.sentinels[&1];
        assert!(s.rt.targets.contains_key(&7));
        assert_eq!(s.data.stats.damage_taken, 3.0);
    }

    #[test]
    fn owner_hit_is_not_fought_back_after_attach() {
        let mut reg = Registry::default();
        let b = Bridge::default();
        reg.attach(1);
        let me = npc_body(1, 11);
        reg.on_hurt(&me, Some(&player(1000)), "minecraft:player_attack", 3.0, &b);
        assert!(!reg.sentinels[&1].rt.targets.contains_key(&1000));
    }

    #[test]
    fn safe_shot_spares_non_targets() {
        let mut reg = Registry::default();
        let b = Bridge::default();
        reg.load(1, &Node::map());
        let me = npc_body(1, 11);
        let bystander = player(7);
        assert_eq!(reg.on_hurt(&bystander, Some(&me), "minecraft:arrow", 5.0, &b), HitDecision::Cancel);
        TargetLabel::parse("players").add_to(&mut reg.sentinels.get_mut(&1).unwrap().data.all_targets);
        assert_eq!(reg.on_hurt(&bystander, Some(&me), "minecraft:arrow", 5.0, &b), HitDecision::Pass);
    }

    #[test]
    fn pvp_event_makes_the_attacker_a_target() {
        use crate::brain::tests::{body, fighter, Fake};
        let mut reg = Registry::default();
        let b = Bridge::default();
        reg.load(1, &Node::map());
        TargetLabel::parse("event:pvp").add_to(&mut reg.sentinels.get_mut(&1).unwrap().data.all_targets);
        let me = fighter(1, "Guard", [0.0, 64.0, 0.0]);
        let mut a = body(20, "minecraft:player", [3.0, 64.0, 0.0]);
        a.player_name = Some("A".into());
        let mut v = body(21, "minecraft:player", [4.0, 64.0, 0.0]);
        v.player_name = Some("V".into());
        reg.on_hurt(&v, Some(&a), "minecraft:player_attack", 2.0, &b);
        let mut w = Fake { entities: vec![me.clone(), a.clone(), v], ..Default::default() };
        reg.update(1, &me, None, None, &mut w);
        assert!(reg.sentinels[&1].rt.targets.contains_key(&20), "the attacker is a target");
        assert!(!reg.sentinels[&1].rt.targets.contains_key(&21));
        reg.end_tick("minecraft:overworld", true);
    }

    #[test]
    fn squad_mates_share_targets() {
        use crate::brain::tests::{body, fighter, Fake};
        let mut reg = Registry::default();
        reg.load(1, &crate::node::parse_yaml("squad: red\n").unwrap());
        reg.load(2, &crate::node::parse_yaml("squad: red\n").unwrap());
        TargetLabel::parse("zombie").add_to(&mut reg.sentinels.get_mut(&1).unwrap().data.all_targets);
        let me = fighter(1, "A", [0.0, 64.0, 0.0]);
        let z = body(30, "minecraft:zombie", [5.0, 64.0, 0.0]);
        let mut w = Fake { entities: vec![me.clone(), z], ..Default::default() };
        reg.update(1, &me, None, None, &mut w);
        assert!(reg.sentinels[&2].rt.targets.contains_key(&30));
    }

    #[test]
    fn invincible_takes_nothing() {
        let mut reg = Registry::default();
        let b = Bridge::default();
        reg.load(1, &crate::node::parse_yaml("invincible: true\n").unwrap());
        assert_eq!(reg.on_hurt(&npc_body(1, 11), None, "minecraft:fall", 3.0, &b), HitDecision::Cancel);
    }
}
