//! One Sentinel's behaviour between updates: current targets and avoids, chasing, attacking,
//! guarding, running away, healing, greeting (`SentinelTrait.runUpdate`,
//! `SentinelTargetingHelper`, `SentinelAttackHelper`, `SentinelWeaponHelper`). Translated from
//! Sentinel (MIT, see LICENSE); Bukkit calls became [`World`] calls.

use crate::combat::{self, WeaponKind};
use crate::config::Config;
use crate::data::SentinelData;
use crate::list::Asker;
use crate::view::{dist_sq, looking_towards, EntityView, Vec3};
use crate::world::{Projectile, World};
use std::collections::{HashMap, HashSet};

/// A target or avoid that is current (`SentinelCurrentTarget`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Current {
    /// Ticks until it is forgotten; 0 or less = until it dies or leaves.
    pub ticks_left: i64,
    pub has_los: bool,
}

/// The state of one Sentinel that is not saved.
#[derive(Clone, Debug, Default)]
pub struct Runtime {
    pub targets: HashMap<u128, Current>,
    pub avoids: HashMap<u128, Current>,
    /// Targets waiting out `reaction slowdown` (uuid, ticks left).
    pub pending: Vec<(u128, i64)>,
    /// The entity being chased (by UUID).
    pub chasing: Option<u128>,
    pub time_since_attack: i64,
    pub time_since_heal: i64,
    pub ticks_since_burn: i64,
    pub clever_ticks: i64,
    pub chased: bool,
    pub needs_safe_return: bool,
    pub other_paused: bool,
    pub pathing_to: Option<Vec3>,
    pub blocking: bool,
    pub shield_axe_cooldown: i64,
    pub greeted: HashSet<u128>,
    pub last_greet: HashMap<u128, i64>,
    /// Non-player victims whose drops are cleared when they die (`enemy drops` off).
    pub needs_drops_clear: HashSet<u128>,
    pub ticks_count_guard: i64,
    /// Ticks until a respawn, while dead and `respawn time` > 0.
    pub respawn_in: Option<i64>,
    pub ticks_until_update: i64,
    /// Targets added this update that squad mates should share (the registry hands them on).
    pub squad_adds: Vec<u128>,
    pub squad_removes: Vec<u128>,
    /// `hasShownFreshInitWarn`.
    pub shown_init_warn: bool,
}

/// Everything one update reads besides the world.
pub struct Ctx<'a> {
    pub data: &'a mut SentinelData,
    pub cfg: &'a Config,
    /// The NPC's body as it is now.
    pub me: &'a EntityView,
    pub asker: &'a Asker,
    /// The guarded player or NPC, when it is in reach of the host.
    pub guarded: Option<EntityView>,
}

const MAX_DIST: f64 = 100_000_000.0;

impl Runtime {
    pub fn new() -> Self {
        Runtime { needs_safe_return: true, ..Default::default() }
    }

    // ----- targeting (SentinelTargetingHelper) -----

    /// `canSee`: same level, within `range` eye to eye, facing it when `realistic`, and a clear
    /// line of sight unless `ignore_los`.
    pub fn can_see(&self, cx: &Ctx, e: &EntityView, w: &dyn World) -> bool {
        let me = cx.me;
        if me.dimension != e.dimension {
            return false;
        }
        if dist_sq(me.eye, e.eye) > cx.data.range * cx.data.range {
            return false;
        }
        if cx.data.realistic && !looking_towards(me.eye, me.yaw, me.pitch, e.pos, 90.0, 110.0) {
            return false;
        }
        cx.data.ignore_los || w.line_of_sight(me.eye, e.eye)
    }

    fn invisible(&self, cx: &Ctx, e: &EntityView) -> bool {
        cx.cfg.random.ignore_invisible_targets && !self.targets.contains_key(&e.uuid) && e.is_invisible()
    }

    fn is_guarded(&self, cx: &Ctx, e: &EntityView) -> bool {
        cx.data.guarding() == Some(e.uuid) || cx.guarded.as_ref().is_some_and(|g| g.uuid == e.uuid)
    }

    fn armor_stand_blocked(cx: &Ctx, e: &EntityView) -> bool {
        e.kind == "minecraft:armor_stand" && !cx.cfg.random.allow_armor_stand_targets
    }

    pub fn is_ignored(&self, cx: &Ctx, e: &EntityView) -> bool {
        e.untargetable()
            || self.invisible(cx, e)
            || e.uuid == cx.me.uuid
            || Self::armor_stand_blocked(cx, e)
            || self.is_guarded(cx, e)
            || cx.data.all_ignores.is_target(e, cx.asker)
    }

    pub fn is_targeted(&self, cx: &Ctx, e: &EntityView) -> bool {
        if self.invisible(cx, e) || e.uuid == cx.me.uuid || self.is_guarded(cx, e) || e.untargetable() {
            return false;
        }
        self.targets.contains_key(&e.uuid) || cx.data.all_targets.is_target(e, cx.asker)
    }

    fn is_avoided(&self, cx: &Ctx, e: &EntityView) -> bool {
        if self.invisible(cx, e) || e.uuid == cx.me.uuid || self.is_guarded(cx, e) {
            return false;
        }
        self.avoids.contains_key(&e.uuid) || cx.data.all_avoids.is_target(e, cx.asker)
    }

    pub fn should_target(&self, cx: &Ctx, e: &EntityView) -> bool {
        e.uuid != cx.me.uuid && !Self::armor_stand_blocked(cx, e) && self.is_targeted(cx, e) && !self.is_ignored(cx, e)
    }

    fn should_avoid(&self, cx: &Ctx, e: &EntityView) -> bool {
        e.uuid != cx.me.uuid && !Self::armor_stand_blocked(cx, e) && self.is_avoided(cx, e) && !self.is_ignored(cx, e)
    }

    /// `addTarget`: current for `enemy target time` (or after `reaction slowdown`), shared with
    /// the squad.
    pub fn add_target(&mut self, cx: &Ctx, uuid: u128) {
        if uuid == cx.me.uuid {
            return;
        }
        self.add_target_no_bounce(cx, uuid);
        if cx.data.squad.is_some() {
            self.squad_adds.push(uuid);
        }
    }

    pub fn add_target_no_bounce(&mut self, cx: &Ctx, uuid: u128) {
        if cx.data.reaction_slowdown == 0 {
            ensure(&mut self.targets, cx.data.enemy_target_time, uuid);
        } else if !self.pending.iter().any(|(u, _)| *u == uuid) {
            self.pending.push((uuid, cx.data.reaction_slowdown));
        }
    }

    pub fn add_avoid(&mut self, cx: &Ctx, uuid: u128) {
        if uuid != cx.me.uuid {
            ensure(&mut self.avoids, cx.cfg.random.run_away_time, uuid);
        }
    }

    pub fn remove_target(&mut self, squad: bool, uuid: u128) -> bool {
        let removed = self.targets.remove(&uuid).is_some();
        if removed && squad {
            self.squad_removes.push(uuid);
        }
        removed
    }

    /// `updateTargets`: forgets targets that died, left, went far, became ignored or timed out.
    fn update_targets(&mut self, cx: &Ctx, w: &mut dyn World, rate: i64) {
        // Reaction slowdown: targets whose delay ran out become current.
        let mut ready = Vec::new();
        self.pending.retain_mut(|(u, t)| {
            *t -= rate;
            if *t <= 0 {
                ready.push(*u);
                false
            } else {
                true
            }
        });
        for u in ready {
            ensure(&mut self.targets, cx.data.enemy_target_time, u);
        }
        let range_sq = cx.data.range * cx.data.range * 4.0;
        let chase_sq = cx.data.chase_range * cx.data.chase_range * 4.0;
        let ids: Vec<u128> = self.targets.keys().copied().collect();
        for id in ids {
            let gone = match w.entity(id) {
                None => true,
                Some(e) if e.untargetable() || e.dimension != cx.me.dimension => true,
                Some(e) => {
                    let d = e.distance_sq(cx.me.pos);
                    (d > range_sq && d > chase_sq) || self.is_ignored(cx, &e)
                }
            };
            if gone {
                self.targets.remove(&id);
                continue;
            }
            if let Some(t) = self.targets.get_mut(&id) {
                if t.ticks_left > 0 {
                    t.ticks_left -= rate;
                    if t.ticks_left <= 0 {
                        self.targets.remove(&id);
                    }
                }
            }
        }
        if let Some(c) = self.chasing {
            if !self.targets.contains_key(&c) {
                self.chasing = None;
                w.cancel_navigation();
            }
        }
    }

    fn update_avoids(&mut self, w: &dyn World, rate: i64) {
        let ids: Vec<u128> = self.avoids.keys().copied().collect();
        for id in ids {
            match w.entity(id) {
                Some(e) if e.alive => {}
                _ => {
                    self.avoids.remove(&id);
                    continue;
                }
            }
            if let Some(t) = self.avoids.get_mut(&id) {
                if t.ticks_left > 0 {
                    t.ticks_left -= rate;
                    if t.ticks_left <= 0 {
                        self.avoids.remove(&id);
                    }
                }
            }
        }
    }

    /// `nearestPathPoint`: the waypoint closest to the NPC; none while guarding.
    fn nearest_path_point(&self, cx: &Ctx, w: &dyn World) -> Option<Vec3> {
        if cx.data.guarding().is_some() || cx.data.guarded_npc >= 0 && cx.guarded.is_some() {
            return None;
        }
        w.waypoints()
            .into_iter()
            .min_by(|a, b| dist_sq(*a, cx.me.pos).total_cmp(&dist_sq(*b, cx.me.pos)))
    }

    /// `getGuardZone`: where targets are looked for.
    fn guard_zone(&self, cx: &Ctx, w: &dyn World) -> Vec3 {
        if let Some(g) = &cx.guarded {
            return g.pos;
        }
        if cx.data.chase_range > 0.0 {
            if let Some(p) = self.nearest_path_point(cx, w) {
                return p;
            }
        }
        cx.me.pos
    }

    /// `canPathTo`: within `chase range` of the nearest path point.
    fn can_path_to(&self, cx: &Ctx, w: &dyn World, to: Vec3) -> bool {
        let crsq = cx.data.chase_range * cx.data.chase_range;
        match self.nearest_path_point(cx, w) {
            Some(near) if crsq > 0.0 => dist_sq(near, to) <= crsq,
            _ => true,
        }
    }

    /// `findBestTarget`: the closest current or listed target in sight around the guard zone.
    fn find_best_target(&mut self, cx: &Ctx, w: &mut dyn World) -> Option<EntityView> {
        let pos = self.guard_zone(cx, w);
        let ignore_glow = cx.me.main_hand.as_ref().is_some_and(|i| i.id == "minecraft:spectral_arrow");
        let mut range_sq = cx.data.range * cx.data.range;
        let crsq = cx.data.chase_range * cx.data.chase_range;
        if let (Some(c), true) = (self.chasing, cx.data.retain_target) {
            if let Some(e) = w.entity(c) {
                if dist_sq(e.eye, pos) < crsq && self.should_target(cx, &e) && self.can_path_to(cx, w, e.pos) {
                    return Some(e);
                }
            }
        }
        let mut closest: Option<EntityView> = None;
        let mut was_los = false;
        let mut slow_adds = Vec::new();
        for e in w.nearby(pos, cx.data.range) {
            if (ignore_glow && e.glowing) || !e.alive {
                continue;
            }
            let dist = dist_sq(e.eye, pos);
            let mut current = None;
            if dist < crsq && dist < range_sq {
                current = self.targets.get(&e.uuid).copied().filter(|_| self.can_path_to(cx, w, e.pos));
            }
            if current.is_none() && !(dist < range_sq && self.should_target(cx, &e)) {
                continue;
            }
            let has_los = self.can_see(cx, &e, w);
            if !has_los && !current.is_some_and(|c| c.has_los) {
                continue;
            }
            if has_los {
                if let Some(c) = self.targets.get_mut(&e.uuid) {
                    c.has_los = true;
                }
            }
            if current.is_none() && cx.data.reaction_slowdown != 0 {
                slow_adds.push(e.uuid);
                continue;
            }
            if !was_los || has_los {
                range_sq = dist;
                was_los = has_los;
                closest = Some(e);
            }
        }
        for u in slow_adds {
            self.add_target(cx, u);
        }
        if let Some(c) = &closest {
            self.add_target(cx, c.uuid);
        }
        closest
    }

    /// `processMultiTargets`: a `multi:` list whose every entry is matched by someone in sight
    /// makes all of them targets (or avoids).
    fn process_multi(&mut self, cx: &Ctx, w: &mut dyn World, avoids: bool) {
        let base = if avoids { &cx.data.all_avoids } else { &cx.data.all_targets };
        if base.by_multiple.is_empty() {
            return;
        }
        let mut subs: Vec<(crate::list::TargetList, Vec<u128>)> =
            base.by_multiple.iter().map(|l| (l.clone(), Vec::new())).collect();
        let pos = self.guard_zone(cx, w);
        let mut done = Vec::new();
        for e in w.nearby(pos, cx.data.range) {
            if !e.alive || self.is_ignored(cx, &e) || !self.can_see(cx, &e, w) {
                continue;
            }
            for (list, hit) in subs.iter_mut() {
                if list.take_match(&e, cx.asker) {
                    hit.push(e.uuid);
                    if list.total() == 0 {
                        done.extend(hit.iter().copied());
                    }
                }
            }
        }
        for u in done {
            if avoids {
                self.add_avoid(cx, u);
            } else {
                self.add_target(cx, u);
            }
        }
    }

    // ----- attacking (SentinelAttackHelper) -----

    fn chases_with(&self, cx: &Ctx, ranged: bool) -> bool {
        if ranged { cx.data.ranged_chase } else { cx.data.close_chase }
    }

    fn weapon(&self, cx: &Ctx) -> WeaponKind {
        combat::weapon_kind(cx.data, cx.me.main_hand.as_ref(), cx.cfg.random.skull_allowed)
    }

    /// `chase`: follow the target.
    fn chase(&mut self, cx: &Ctx, w: &mut dyn World, target: &EntityView) {
        self.clever_ticks = 0;
        self.chasing = Some(target.uuid);
        self.chased = true;
        self.needs_safe_return = true;
        let speed = cx.data.speed;
        if cx.cfg.random.workaround_entity_chase_pathfinder {
            // Walk to where the target will be, with a little spread so a crowd does not stack.
            let jitter = [w.random() * 2.0 - 1.0, w.random() * 2.0 - 1.0];
            let goal = [target.pos[0] + target.velocity[0] + jitter[0], target.pos[1], target.pos[2] + target.velocity[2] + jitter[1]];
            w.navigate_to(goal, speed, 1.0);
        } else {
            w.navigate_to_entity(target, speed);
        }
    }

    fn rechase(&mut self, cx: &Ctx, w: &mut dyn World) {
        let Some(c) = self.chasing else { return };
        if self.chases_with(cx, self.weapon(cx).is_ranged()) {
            if let Some(e) = w.entity(c) {
                self.chase(cx, w, &e);
            }
        }
    }

    /// `tryDefendFrom`: raise the shield while the target threatens.
    fn try_defend_from(&mut self, cx: &Ctx, w: &mut dyn World, e: &EntityView) {
        let has_shield = cx.me.off_hand.as_ref().is_some_and(|i| combat::is_shield(&i.id))
            || cx.me.main_hand.as_ref().is_some_and(|i| combat::is_shield(&i.id));
        if !has_shield {
            return;
        }
        let threat = self.can_see(cx, e, w)
            && looking_towards(cx.me.eye, cx.me.yaw, cx.me.pitch, e.pos, 60.0, 60.0)
            && (dist_sq(cx.me.eye, e.eye) < 25.0
                || e.main_hand.as_ref().is_some_and(|i| combat::is_bow(&i.id) || i.id == "minecraft:trident"));
        if threat != self.blocking {
            self.blocking = threat;
            w.set_blocking(threat);
        }
    }

    /// `tryAttack`: attack the target, or a quick melee target in reach.
    fn try_attack(&mut self, cx: &mut Ctx, w: &mut dyn World, target: &EntityView) -> bool {
        if self.try_attack_internal(cx, w, target) {
            return true;
        }
        let reach = cx.data.reach * 0.75;
        let quick = w
            .nearby(cx.me.eye, reach)
            .into_iter()
            .find(|e| self.should_target(cx, e) && self.can_see(cx, e, w));
        if let Some(q) = quick {
            if self.weapon(cx).is_ranged() {
                if !cx.data.autoswitch || !w.swap_held(&|id| !is_ranged_item(id)) {
                    return false;
                }
            }
            let hit = self.try_attack_internal(cx, w, &q);
            if self.chases_with(cx, self.weapon(cx).is_ranged()) {
                self.chase(cx, w, target);
            }
            return hit;
        }
        false
    }

    /// `rangedPreCalculation`: true = don't fire now.
    fn ranged_wait(&mut self, cx: &Ctx, w: &mut dyn World, e: &EntityView) -> bool {
        let dist = dist_sq(cx.me.eye, e.eye);
        if dist < cx.data.projectile_range * cx.data.projectile_range && self.can_see(cx, e, w) {
            if self.time_since_attack < cx.data.attack_rate_ranged {
                if cx.data.ranged_chase {
                    self.rechase(cx, w);
                }
                return true;
            }
            self.time_since_attack = 0;
            return false;
        }
        if cx.data.ranged_chase {
            self.chase(cx, w, e);
        }
        true
    }

    fn try_attack_internal(&mut self, cx: &mut Ctx, w: &mut dyn World, e: &EntityView) -> bool {
        if e.dimension != cx.me.dimension {
            return false;
        }
        if !w.line_of_sight(cx.me.eye, e.eye) {
            if cx.data.ignore_los {
                if self.chases_with(cx, self.weapon(cx).is_ranged()) {
                    self.chase(cx, w, e);
                }
            } else if !w.is_navigating() {
                if let Some(t) = self.targets.get_mut(&e.uuid) {
                    t.has_los = false;
                }
            }
            return false;
        }
        cx.data.stats.attack_attempts += 1;
        let dist = dist_sq(cx.me.eye, e.eye);
        let reach_sq = cx.data.reach * cx.data.reach;
        if cx.data.autoswitch {
            if dist > reach_sq {
                w.swap_held(&is_ranged_item);
            } else {
                w.swap_held(&|id| !is_ranged_item(id));
            }
        }
        self.chasing = Some(e.uuid);
        self.add_target(cx, e.uuid);
        let kind = self.weapon(cx);
        if kind != WeaponKind::Melee {
            if self.ranged_wait(cx, w, e) {
                return false;
            }
            self.fire(cx, w, kind, e);
            if cx.data.needs_ammo {
                w.take_held_item();
            }
            return true;
        }
        if dist < reach_sq {
            if self.time_since_attack < cx.data.attack_rate {
                if cx.data.close_chase {
                    self.rechase(cx, w);
                }
                return false;
            }
            self.time_since_attack = 0;
            self.punch(cx, w, e);
            return true;
        }
        if cx.data.close_chase {
            self.chase(cx, w, e);
        }
        false
    }

    /// `punch`.
    fn punch(&mut self, cx: &mut Ctx, w: &mut dyn World, e: &EntityView) {
        w.face(e.eye);
        w.swing();
        cx.data.stats.punches += 1;
        let r = &cx.cfg.random;
        if r.workaround_damage {
            let dmg = combat::damage(cx.data, false, cx.me.main_hand.as_ref());
            if !cx.data.enemy_drops && !e.is_player() {
                self.needs_drops_clear.insert(e.uuid);
            }
            let blocked = combat::armor(cx.data, &e.armor);
            w.hurt(e, dmg * (1.0 - blocked));
            w.push(e, combat::knockback(cx.me.pos, e.pos, e.velocity, 0.0, 1.0));
            return;
        }
        let native_kind = cx.me.kind == "minecraft:player"
            || cx.me.kind == "minecraft:mannequin"
            || crate::target::NATIVE_COMBAT_KINDS.contains(&cx.me.kind.as_str());
        if cx.data.damage < 0.0 && r.do_native_attacks && native_kind && w.native_attack(e) {
            return;
        }
        let dmg = combat::damage(cx.data, false, cx.me.main_hand.as_ref());
        w.hurt(e, dmg);
    }

    /// `fireArrow`, `fireSnowball` and the rest: a projectile aimed at the target's eyes.
    fn fire(&mut self, cx: &mut Ctx, w: &mut dyn World, kind: WeaponKind, e: &EntityView) {
        w.face(e.eye);
        w.swing();
        let start_dist = match cx.me.kind.as_str() {
            "minecraft:wither" | "minecraft:ghast" => 8.0,
            "minecraft:blaze" => 3.0,
            "minecraft:player" | "minecraft:mannequin" => 0.1,
            _ => 2.0,
        };
        let dir = unit([e.eye[0] - cx.me.eye[0], e.eye[1] - cx.me.eye[1], e.eye[2] - cx.me.eye[2]]);
        let start = [cx.me.eye[0] + dir[0] * start_dist, cx.me.eye[1] + dir[1] * start_dist, cx.me.eye[2] + dir[2] * start_dist];
        let acc = cx.data.accuracy;
        let spread = if acc > 0.0 {
            [w.random() * acc * 2.0 - acc, w.random() * acc * 2.0 - acc, w.random() * acc * 2.0 - acc]
        } else {
            [0.0; 3]
        };
        let (projectile, speed, gravity) = match kind {
            WeaponKind::Bow => (Projectile::Arrow, 3.0, 0.05),
            WeaponKind::Snowball => (Projectile::Snowball, 1.5, 0.03),
            WeaponKind::Trident => (Projectile::Trident, 2.5, 0.05),
            WeaponKind::Potion => (Projectile::Potion, 0.75, 0.05),
            WeaponKind::Egg => (Projectile::Egg, 1.5, 0.03),
            WeaponKind::LlamaSpit => (Projectile::LlamaSpit, 1.5, 0.06),
            WeaponKind::ShulkerBullet => (Projectile::ShulkerBullet, 0.5, 0.0),
            WeaponKind::Pearl => (Projectile::Pearl, 1.5, 0.03),
            WeaponKind::WitherSkull => (Projectile::WitherSkull, 1.0, 0.0),
            WeaponKind::Fireball => (Projectile::SmallFireball, 1.0, 0.0),
            WeaponKind::FangsBook => {
                w.launch(Projectile::EvokerFangs, e.pos, [0.0; 3], None);
                cx.data.stats.evoker_fangs_spawned += 1;
                return;
            }
            WeaponKind::Lightning => {
                w.lightning_effect(e.pos);
                let dmg = combat::damage(cx.data, false, cx.me.main_hand.as_ref());
                w.hurt(e, dmg);
                return;
            }
            WeaponKind::Spectral => {
                if !e.glowing {
                    if !cx.cfg.random.spectral_sound.is_empty() {
                        w.play_sound(&cx.cfg.random.spectral_sound, e.pos);
                    }
                    w.make_glow(e, 20 * 60);
                }
                return;
            }
            WeaponKind::Melee => return,
        };
        let lead = [e.velocity[0], 0.0, e.velocity[2]];
        let mut v = launch_velocity(start, [e.eye[0] + lead[0], e.eye[1], e.eye[2] + lead[2]], speed, gravity);
        v = [v[0] + spread[0], v[1] + spread[1], v[2] + spread[2]];
        let item = cx.me.main_hand.as_ref().map(|i| i.id.clone());
        w.launch(projectile, start, v, item.as_deref());
        let s = &mut cx.data.stats;
        match projectile {
            Projectile::Arrow | Projectile::Trident => s.arrows_fired += 1,
            Projectile::Snowball => s.snowballs_thrown += 1,
            Projectile::Potion => s.potions_thrown += 1,
            Projectile::Egg => s.eggs_thrown += 1,
            Projectile::LlamaSpit => s.llama_spit_shot += 1,
            Projectile::ShulkerBullet => s.shulker_bullets_shot += 1,
            Projectile::Pearl => s.pearls_used += 1,
            Projectile::WitherSkull => s.skulls_thrown += 1,
            Projectile::SmallFireball => s.fireballs_fired += 1,
            Projectile::EvokerFangs => s.evoker_fangs_spawned += 1,
        }
    }

    // ----- running away -----

    /// `processAvoidance`: threats in reach make the NPC run to its avoid return point, its
    /// guard zone, or the direction farthest from the threats.
    fn process_avoidance(&mut self, cx: &Ctx, w: &mut dyn World) {
        if self.avoids.is_empty() && cx.data.all_avoids.total() == 0 {
            return;
        }
        let range = cx.data.avoid_range + 10.0;
        let mut threats = Vec::new();
        for e in w.nearby(cx.me.pos, range) {
            if self.should_avoid(cx, &e) {
                if !self.avoids.contains_key(&e.uuid) && !self.can_see(cx, &e, w) {
                    continue;
                }
                self.add_avoid(cx, e.uuid);
                threats.push(e);
            }
        }
        if threats.is_empty() {
            return;
        }
        let run_to = if let Some(p) = &cx.data.avoid_return_point {
            Some(p.pos())
        } else {
            let center = self.guard_zone(cx, w);
            let closest = threats.iter().map(|t| dist_sq(t.pos, center)).fold(MAX_DIST, f64::min);
            if closest >= cx.data.avoid_range * cx.data.avoid_range {
                Some(center)
            } else {
                Some(run_direction(center, cx.data.avoid_range, &threats))
            }
        };
        if let Some(to) = run_to {
            if self.can_path_to(cx, w, to) {
                if !self.other_paused {
                    self.other_paused = true;
                    w.pause_waypoints(true);
                }
                self.pathing_to = Some(to);
                w.navigate_to(to, cx.data.speed, 1.5);
                self.chasing = None;
                self.needs_safe_return = true;
            }
        }
    }

    // ----- the update -----

    /// `runUpdate`, every `update rate` ticks while the NPC is spawned.
    pub fn update(&mut self, cx: &mut Ctx, w: &mut dyn World) {
        let rate = cx.cfg.update_rate;
        self.ticks_since_burn += rate;
        self.time_since_attack += rate;
        self.time_since_heal += rate;
        // Below the world: hurt itself until it dies.
        if cx.me.pos[1] <= w.min_y() {
            w.hurt_self(1.0);
        }
        if (cx.me.max_health - cx.data.health).abs() > f64::EPSILON {
            w.set_max_health(cx.data.health);
        }
        if cx.data.heal_rate > 0 && self.time_since_heal > cx.data.heal_rate && cx.me.health < cx.data.health {
            w.set_health((cx.me.health + 1.0).min(cx.data.health));
            self.time_since_heal = 0;
        }
        if !w.is_navigating() {
            self.pathing_to = None;
        }
        let guarding = cx.guarded.is_some();
        if guarding || self.chasing.is_some() || self.pathing_to.is_some() {
            if !self.other_paused {
                self.other_paused = true;
                w.pause_waypoints(true);
            }
        } else if self.other_paused {
            self.other_paused = false;
            w.pause_waypoints(false);
        }
        self.update_targets(cx, w, rate);
        self.update_avoids(w, rate);
        let mut go_home = self.chased;
        if let Some(c) = self.chasing {
            let valid = w.entity(c).is_some_and(|e| e.alive && self.should_target(cx, &e));
            if !valid {
                self.chasing = None;
            }
        }
        self.process_multi(cx, w, false);
        self.process_multi(cx, w, true);
        let target = self.find_best_target(cx, w);
        if let Some(t) = &target {
            if self.can_path_to(cx, w, t.pos) {
                self.chasing = Some(t.uuid);
                self.needs_safe_return = true;
                self.clever_ticks = 0;
                self.try_defend_from(cx, w, t);
                self.try_attack(cx, w, t);
                go_home = false;
            } else {
                self.chasing = None;
                self.clever_ticks = 0;
            }
        } else if let Some(c) = self.chasing.and_then(|c| w.entity(c)) {
            self.needs_safe_return = true;
            if self.can_see(cx, &c, w) && cx.cfg.random.workaround_entity_chase_pathfinder {
                self.rechase(cx, w);
            }
            self.clever_ticks += 1;
            if self.clever_ticks >= cx.cfg.random.clever_ticks {
                self.chasing = None;
            } else if self.can_path_to(cx, w, c.pos) {
                self.try_defend_from(cx, w, &c);
                self.try_attack(cx, w, &c);
                go_home = false;
            }
        }
        if let Some(g) = cx.guarded.clone() {
            let dist = if g.dimension == cx.me.dimension { dist_sq(cx.me.pos, g.pos) } else { MAX_DIST };
            if dist > 60.0 * 60.0 && !cx.data.disable_teleporting {
                w.teleport_self(g.pos);
            }
            let min = cx.data.guard_distance_minimum;
            if dist > min * min {
                self.ticks_count_guard += rate;
                if self.ticks_count_guard >= 30 {
                    self.ticks_count_guard = 0;
                    let r = cx.data.guard_selection_range;
                    let picked = [g.pos[0] + (w.random() * 2.0 - 1.0) * r, g.pos[1], g.pos[2] + (w.random() * 2.0 - 1.0) * r];
                    w.navigate_to(picked, cx.data.speed, min * 0.75);
                    self.chased = true;
                }
            }
            self.needs_safe_return = true;
            go_home = false;
        }
        self.process_avoidance(cx, w);
        if self.pathing_to.is_some() {
            go_home = false;
            self.needs_safe_return = true;
        }
        if self.blocking && target.is_none() && self.chasing.is_none() {
            self.blocking = false;
            w.set_blocking(false);
        }
        if go_home && cx.data.chase_range > 0.0 && target.is_none() && self.needs_safe_return {
            let near = self.nearest_path_point(cx, w);
            let chasing_reachable = self.chasing.and_then(|c| w.entity(c)).is_some_and(|c| self.can_path_to(cx, w, c.pos));
            match near {
                Some(near) if !chasing_reachable => {
                    w.navigate_to(near, cx.data.speed, 1.0);
                    self.needs_safe_return = false;
                    self.chased = false;
                    self.chasing = None;
                }
                _ => {
                    if self.pathing_to.is_none() && w.is_navigating() && !guarding {
                        w.cancel_navigation();
                        self.needs_safe_return = false;
                    }
                }
            }
        } else if self.chasing.is_none() && !guarding && self.pathing_to.is_none() && self.needs_safe_return {
            if w.is_navigating() {
                w.cancel_navigation();
            }
            self.needs_safe_return = false;
        }
        self.greet(cx, w);
    }

    /// `onPlayerMovesInRange`, checked each update instead of on every move: a player who comes
    /// within `greet range` hears the greeting (or the warning, if they are a target).
    fn greet(&mut self, cx: &Ctx, w: &mut dyn World) {
        if cx.data.greeting_text.is_empty() && cx.data.warning_text.is_empty() {
            return;
        }
        let r = cx.data.greet_range;
        let players: Vec<EntityView> = w.nearby(cx.me.pos, r + 2.0).into_iter().filter(|e| e.is_player()).collect();
        let now = cx.data.stats.ticks_spawned;
        let mut inside = HashSet::new();
        for p in &players {
            let d = p.distance_sq(cx.me.pos);
            if d < r * r && !self.greeted.contains(&p.uuid) && self.can_see(cx, p, w) {
                inside.insert(p.uuid);
                self.greeted.insert(p.uuid);
                if self.last_greet.get(&p.uuid).is_some_and(|t| t + cx.data.greet_rate > now) {
                    continue;
                }
                self.last_greet.insert(p.uuid, now);
                let enemy = self.should_target(cx, p);
                let text = if enemy { &cx.data.warning_text } else { &cx.data.greeting_text };
                if !text.is_empty() {
                    let name = cx.me.npc.as_ref().map_or("NPC", |n| n.name.as_str());
                    w.tell(p.uuid, &format!("\u{a7}b[{name}]: \u{a7}e{text}"));
                }
            } else if d < r * r {
                inside.insert(p.uuid);
            }
        }
        // Who walked out of range (+1, as Sentinel) may be greeted again.
        self.greeted.retain(|u| inside.contains(u) || players.iter().any(|p| p.uuid == *u && p.distance_sq(cx.me.pos) < r * r + 1.0));
    }

    /// `onDeath` / `onDespawn`: forget the fight.
    pub fn reset_fight(&mut self) {
        self.greeted.clear();
        self.targets.clear();
        self.avoids.clear();
        self.pending.clear();
        self.chasing = None;
        self.blocking = false;
    }
}

fn ensure(set: &mut HashMap<u128, Current>, time: i64, uuid: u128) {
    let entry = set.entry(uuid).or_insert(Current { ticks_left: time, has_los: true });
    entry.ticks_left = time;
}

fn is_ranged_item(id: &str) -> bool {
    let path = id.split_once(':').map_or(id, |(_, p)| p);
    matches!(
        path,
        "bow" | "crossbow" | "snowball" | "trident" | "splash_potion" | "lingering_potion" | "egg" | "ender_pearl"
            | "blaze_rod" | "nether_star" | "spectral_arrow" | "wither_skeleton_skull"
    )
}

fn unit(v: Vec3) -> Vec3 {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l == 0.0 || !l.is_finite() { [0.0, 0.0, 1.0] } else { [v[0] / l, v[1] / l, v[2] / l] }
}

/// Initial velocity that carries a projectile of `speed` blocks/tick from `start` to `target`
/// under `gravity` blocks/tick² (`SentinelUtilities.getLaunchDetail`, as a closed-form
/// low-arc solution; drag is left out, so far shots fall a little short).
pub fn launch_velocity(start: Vec3, target: Vec3, speed: f64, gravity: f64) -> Vec3 {
    let d = [target[0] - start[0], target[1] - start[1], target[2] - start[2]];
    let horiz = (d[0] * d[0] + d[2] * d[2]).sqrt();
    if gravity <= 0.0 || horiz < 1e-6 {
        let u = unit(d);
        return [u[0] * speed, u[1] * speed, u[2] * speed];
    }
    let v2 = speed * speed;
    let disc = v2 * v2 - gravity * (gravity * horiz * horiz + 2.0 * d[1] * v2);
    let angle = if disc < 0.0 {
        std::f64::consts::FRAC_PI_4
    } else {
        ((v2 - disc.sqrt()) / (gravity * horiz)).atan()
    };
    let (hx, hz) = (d[0] / horiz, d[2] / horiz);
    [hx * speed * angle.cos(), speed * angle.sin(), hz * speed * angle.cos()]
}

/// `runDirection`: of 36 directions, the one whose point at `range` is farthest from every
/// threat (Sentinel also checks the spot is walkable with an A* run; the host's navigation
/// stops at the nearest reachable block instead).
pub fn run_direction(center: Vec3, range: f64, threats: &[EntityView]) -> Vec3 {
    let mut best = center;
    let mut longest = -1.0;
    for i in 0..36 {
        let yaw = (i * 10) as f64;
        let dir = [(-yaw.to_radians()).sin(), 0.0, yaw.to_radians().cos()];
        let point = [dir[0] * range, 0.0, dir[2] * range];
        let mut nearest = 1_000_000.0;
        for t in threats {
            let rel = [t.pos[0] - center[0], t.pos[1] - center[1], t.pos[2] - center[2]];
            nearest = f64::min(nearest, dist_sq(rel, point));
        }
        if nearest > longest {
            longest = nearest;
            best = [center[0] + point[0], center[1], center[2] + point[2]];
        }
    }
    best
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::list::TargetLabel;
    use crate::view::{GameMode, NpcInfo};

    /// A flat empty world with entities in it; records what the NPC did.
    #[derive(Default)]
    pub(crate) struct Fake {
        pub entities: Vec<EntityView>,
        pub navigating: bool,
        pub nav_to: Option<Vec3>,
        pub attacks: Vec<u128>,
        pub hurts: Vec<(u128, f64)>,
        pub told: Vec<(u128, String)>,
        pub launched: Vec<Projectile>,
        pub native: bool,
    }

    impl World for Fake {
        fn nearby(&self, c: Vec3, r: f64) -> Vec<EntityView> {
            self.entities
                .iter()
                .filter(|e| (e.pos[0] - c[0]).abs() <= r && (e.pos[1] - c[1]).abs() <= r && (e.pos[2] - c[2]).abs() <= r)
                .cloned()
                .collect()
        }
        fn entity(&self, u: u128) -> Option<EntityView> {
            self.entities.iter().find(|e| e.uuid == u).cloned()
        }
        fn line_of_sight(&self, _: Vec3, _: Vec3) -> bool {
            true
        }
        fn min_y(&self) -> f64 {
            -64.0
        }
        fn random(&mut self) -> f64 {
            0.5
        }
        fn set_max_health(&mut self, _: f64) {}
        fn set_health(&mut self, _: f64) {}
        fn hurt_self(&mut self, _: f64) {}
        fn teleport_self(&mut self, _: Vec3) {}
        fn stop_motion(&mut self) {}
        fn navigate_to(&mut self, to: Vec3, _: f64, _: f64) {
            self.navigating = true;
            self.nav_to = Some(to);
        }
        fn navigate_to_entity(&mut self, t: &EntityView, _: f64) {
            self.navigating = true;
            self.nav_to = Some(t.pos);
        }
        fn cancel_navigation(&mut self) {
            self.navigating = false;
        }
        fn is_navigating(&self) -> bool {
            self.navigating
        }
        fn face(&mut self, _: Vec3) {}
        fn swing(&mut self) {}
        fn set_blocking(&mut self, _: bool) {}
        fn native_attack(&mut self, t: &EntityView) -> bool {
            if self.native {
                self.attacks.push(t.uuid);
            }
            self.native
        }
        fn hurt(&mut self, t: &EntityView, a: f64) {
            self.hurts.push((t.uuid, a));
        }
        fn push(&mut self, _: &EntityView, _: Vec3) {}
        fn launch(&mut self, k: Projectile, _: Vec3, _: Vec3, _: Option<&str>) {
            self.launched.push(k);
        }
        fn lightning_effect(&mut self, _: Vec3) {}
        fn make_glow(&mut self, _: &EntityView, _: i32) {}
        fn play_sound(&mut self, _: &str, _: Vec3) {}
        fn tell(&mut self, p: u128, t: &str) {
            self.told.push((p, t.to_owned()));
        }
        fn take_held_item(&mut self) -> bool {
            true
        }
        fn swap_held(&mut self, _: &dyn Fn(&str) -> bool) -> bool {
            false
        }
        fn waypoints(&self) -> Vec<Vec3> {
            Vec::new()
        }
        fn pause_waypoints(&mut self, _: bool) {}
    }

    pub(crate) fn body(uuid: u128, kind: &str, pos: Vec3) -> EntityView {
        EntityView {
            id: uuid as i32,
            uuid,
            kind: kind.into(),
            dimension: "minecraft:overworld".into(),
            pos,
            eye: [pos[0], pos[1] + 1.62, pos[2]],
            alive: true,
            health: 20.0,
            max_health: 20.0,
            ..Default::default()
        }
    }

    pub(crate) fn fighter(uuid: u128, name: &str, pos: Vec3) -> EntityView {
        let mut e = body(uuid, "minecraft:mannequin", pos);
        e.npc = Some(NpcInfo { id: uuid as i32, name: name.into(), sentinel: true });
        e
    }

    fn run(data: &mut SentinelData, rt: &mut Runtime, me: &EntityView, w: &mut Fake) {
        let cfg = Config::default();
        let asker = Asker { owner: Some(1000), ..Default::default() };
        let mut cx = Ctx { data, cfg: &cfg, me, asker: &asker, guarded: None };
        rt.update(&mut cx, w);
    }

    /// The arena setup: fighters target NPCs and ignore the portal NPCs by name.
    #[test]
    fn fighter_attacks_npcs_but_not_ignored_portal_npcs() {
        let mut data = SentinelData::default();
        TargetLabel::parse("npc").add_to(&mut data.all_targets);
        TargetLabel::parse("owner").add_to(&mut data.all_ignores);
        TargetLabel::parse("npc:^Portal$").add_to(&mut data.all_ignores);
        let me = fighter(1, "Fighter", [0.0, 64.0, 0.0]);
        let portal = fighter(2, "Portal", [1.0, 64.0, 0.0]);
        let mut w = Fake { entities: vec![me.clone(), portal.clone()], native: true, ..Default::default() };
        let mut rt = Runtime::new();
        rt.time_since_attack = 100;
        run(&mut data, &mut rt, &me, &mut w);
        assert!(w.attacks.is_empty() && w.hurts.is_empty(), "the portal NPC is ignored");
        assert_eq!(rt.chasing, None);

        let enemy = fighter(3, "Other", [2.0, 64.0, 0.0]);
        w.entities.push(enemy);
        run(&mut data, &mut rt, &me, &mut w);
        assert_eq!(w.attacks, vec![3], "the other fighter is hit with a native attack");
        assert_eq!(data.stats.punches, 1);
        // The next update is within the attack rate: no second hit.
        run(&mut data, &mut rt, &me, &mut w);
        assert_eq!(w.attacks, vec![3]);
    }

    #[test]
    fn creative_players_and_owner_are_left_alone() {
        let mut data = SentinelData::default();
        TargetLabel::parse("players").add_to(&mut data.all_targets);
        TargetLabel::parse("owner").add_to(&mut data.all_ignores);
        let me = fighter(1, "Guard", [0.0, 64.0, 0.0]);
        let mut owner = body(1000, "minecraft:player", [1.0, 64.0, 0.0]);
        owner.player_name = Some("Owner".into());
        let mut builder = body(7, "minecraft:player", [1.0, 64.0, 1.0]);
        builder.player_name = Some("Builder".into());
        builder.game_mode = GameMode::Creative;
        let mut w = Fake { entities: vec![me.clone(), owner, builder], ..Default::default() };
        let mut rt = Runtime::new();
        rt.time_since_attack = 100;
        run(&mut data, &mut rt, &me, &mut w);
        assert!(w.hurts.is_empty() && w.attacks.is_empty());
    }

    #[test]
    fn out_of_reach_target_is_chased() {
        let mut data = SentinelData::default();
        TargetLabel::parse("zombies").add_to(&mut data.all_targets);
        let me = fighter(1, "Guard", [0.0, 64.0, 0.0]);
        let zombie = body(5, "minecraft:zombie", [10.0, 64.0, 0.0]);
        let mut w = Fake { entities: vec![me.clone(), zombie], ..Default::default() };
        let mut rt = Runtime::new();
        run(&mut data, &mut rt, &me, &mut w);
        assert_eq!(rt.chasing, Some(5));
        assert!(w.navigating);
        assert!(w.hurts.is_empty());
    }

    #[test]
    fn own_damage_when_not_native() {
        let mut data = SentinelData::default();
        data.damage = 4.0;
        TargetLabel::parse("zombie").add_to(&mut data.all_targets);
        let me = fighter(1, "Guard", [0.0, 64.0, 0.0]);
        let zombie = body(5, "minecraft:zombie", [1.0, 64.0, 0.0]);
        let mut w = Fake { entities: vec![me.clone(), zombie], native: true, ..Default::default() };
        let mut rt = Runtime::new();
        rt.time_since_attack = 100;
        run(&mut data, &mut rt, &me, &mut w);
        // A set damage value disables the native attack, as in Sentinel.
        assert_eq!(w.hurts, vec![(5, 4.0)]);
    }

    #[test]
    fn bow_fires_arrows() {
        let mut data = SentinelData::default();
        TargetLabel::parse("zombie").add_to(&mut data.all_targets);
        let mut me = fighter(1, "Archer", [0.0, 64.0, 0.0]);
        me.main_hand = Some(crate::view::ItemView::of("minecraft:bow"));
        let zombie = body(5, "minecraft:zombie", [12.0, 64.0, 0.0]);
        let mut w = Fake { entities: vec![me.clone(), zombie], ..Default::default() };
        let mut rt = Runtime::new();
        rt.time_since_attack = 100;
        run(&mut data, &mut rt, &me, &mut w);
        assert_eq!(w.launched, vec![Projectile::Arrow]);
        assert_eq!(data.stats.arrows_fired, 1);
    }

    #[test]
    fn greeting_once_until_they_leave() {
        let mut data = SentinelData::default();
        data.greeting_text = "Welcome".into();
        let me = fighter(1, "Gate", [0.0, 64.0, 0.0]);
        let mut p = body(9, "minecraft:player", [3.0, 64.0, 0.0]);
        p.player_name = Some("Visitor".into());
        let mut w = Fake { entities: vec![me.clone(), p], ..Default::default() };
        let mut rt = Runtime::new();
        run(&mut data, &mut rt, &me, &mut w);
        run(&mut data, &mut rt, &me, &mut w);
        assert_eq!(w.told.len(), 1);
        assert!(w.told[0].1.contains("Welcome"));
    }

    #[test]
    fn ballistic_hits_level_target() {
        let v = launch_velocity([0.0; 3], [20.0, 0.0, 0.0], 3.0, 0.05);
        assert!(v[0] > 0.0 && v[1] > 0.0 && v[2].abs() < 1e-9);
        let speed = (v[0] * v[0] + v[1] * v[1]).sqrt();
        assert!((speed - 3.0).abs() < 1e-9);
    }
}
