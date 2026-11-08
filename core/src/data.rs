//! The saved settings of one Sentinel NPC (`SentinelTrait`'s `@Persist` fields), read from and
//! written to the Citizens trait subtree (`npc.<id>.traits.sentinel`) with the same keys.
//!
//! Times are in ticks, as saved (the commands take seconds). A key missing from the file reads as
//! the Java field's initial value, as Citizens' `PersistenceLoader` does; a freshly added trait
//! takes the config's `sentinel defaults` instead ([`SentinelData::from_defaults`]).

use crate::config::Defaults;
use crate::list::TargetList;
use crate::node::Node;
use std::collections::BTreeMap;

/// A Citizens/Bukkit location (`world`, `worldid`, `x`, `y`, `z`, `yaw`, `pitch`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Location {
    pub world: String,
    /// The world's UUID as saved by Paper; kept to write it back unchanged.
    pub world_id: Option<String>,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
}

impl Location {
    pub fn load(node: &Node) -> Option<Self> {
        Some(Location {
            world: node.str("world")?.to_owned(),
            world_id: node.str("worldid").map(str::to_owned),
            x: node.f64("x")?,
            y: node.f64("y")?,
            z: node.f64("z")?,
            yaw: node.f64("yaw").unwrap_or(0.0) as f32,
            pitch: node.f64("pitch").unwrap_or(0.0) as f32,
        })
    }
    pub fn save(&self) -> Node {
        let mut n = Node::map();
        n.set("world", Node::string(&self.world));
        if let Some(id) = &self.world_id {
            n.set("worldid", Node::string(id));
        }
        n.set("x", Node::double(self.x));
        n.set("y", Node::double(self.y));
        n.set("z", Node::double(self.z));
        n.set("yaw", Node::float(self.yaw));
        n.set("pitch", Node::float(self.pitch));
        n
    }
    pub fn pos(&self) -> [f64; 3] {
        [self.x, self.y, self.z]
    }
}

/// Lifetime statistics (`stats_*`), shown by `/sentinel stats`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    pub ticks_spawned: i64,
    pub times_spawned: i64,
    pub arrows_fired: i64,
    pub potions_thrown: i64,
    pub fireballs_fired: i64,
    pub snowballs_thrown: i64,
    pub eggs_thrown: i64,
    pub skulls_thrown: i64,
    pub llama_spit_shot: i64,
    pub shulker_bullets_shot: i64,
    pub evoker_fangs_spawned: i64,
    pub pearls_used: i64,
    pub punches: i64,
    pub attack_attempts: i64,
    pub damage_taken: f64,
    pub damage_given: f64,
}

/// Every persisted Sentinel setting, with Sentinel's names in comments where they differ.
#[derive(Clone, Debug, PartialEq)]
pub struct SentinelData {
    pub stats: Stats,
    pub all_targets: TargetList,
    pub all_ignores: TargetList,
    pub all_avoids: TargetList,
    pub avoid_range: f64,
    pub range: f64,
    /// -1 = from the weapon.
    pub damage: f64,
    /// -1 = from the worn armor; else 0..1 of damage blocked.
    pub armor: f64,
    pub health: f64,
    pub ranged_chase: bool,
    pub close_chase: bool,
    pub invincible: bool,
    /// `protected`: damage from ignored entities is cancelled.
    pub protect_from_ignores: bool,
    pub protect_from_range: f64,
    pub fightback: bool,
    pub runaway: bool,
    pub attack_rate: i64,
    pub attack_rate_ranged: i64,
    pub heal_rate: i64,
    /// The guarded player's UUID as two halves (`guardingUpper`, `guardingLower`); 0/0 = none.
    pub guarding_upper: i64,
    pub guarding_lower: i64,
    pub guarded_npc: i32,
    pub needs_ammo: bool,
    pub safe_shot: bool,
    /// Ticks; 0 = stay dead (despawned), -1 = remove the NPC on death.
    pub respawn_time: i64,
    pub chase_range: f64,
    pub spawn_point: Option<Location>,
    pub avoid_return_point: Option<Location>,
    /// Bukkit-serialized item stacks, kept as they are (see [`DropItem`]).
    pub drops: Vec<Node>,
    pub drop_chances: Vec<f64>,
    pub enemy_drops: bool,
    pub enemy_target_time: i64,
    pub speed: f64,
    pub warning_text: String,
    pub greeting_text: String,
    pub greet_range: f64,
    pub greet_rate: i64,
    pub autoswitch: bool,
    pub squad: Option<String>,
    pub accuracy: f64,
    pub realistic: bool,
    pub ignore_los: bool,
    pub reach: f64,
    pub projectile_range: f64,
    pub guard_distance_minimum: f64,
    pub guard_selection_range: f64,
    pub retain_target: bool,
    /// Material (lower case, `diamond_sword`) -> damage.
    pub weapon_damage: BTreeMap<String, f64>,
    /// Material -> material it is treated as.
    pub weapon_redirects: BTreeMap<String, String>,
    pub disable_teleporting: bool,
    pub reaction_slowdown: i64,
    pub allow_knockback: bool,
    pub worldguard_region: Option<String>,
    pub death_xp: i64,
    /// Keys this port does not know, written back unchanged.
    pub unknown: Vec<(String, Node)>,
}

/// The maxima the commands accept (`attackRateMax`, `healRateMax`), in ticks.
pub const ATTACK_RATE_MAX: i64 = 2000;
pub const HEAL_RATE_MAX: i64 = 2000;

impl Default for SentinelData {
    /// The Java field initial values (what a missing key reads as).
    fn default() -> Self {
        SentinelData {
            stats: Stats::default(),
            all_targets: TargetList::default(),
            all_ignores: TargetList::default(),
            all_avoids: TargetList::default(),
            avoid_range: 10.0,
            range: 20.0,
            damage: -1.0,
            armor: -1.0,
            health: 20.0,
            ranged_chase: false,
            close_chase: true,
            invincible: false,
            protect_from_ignores: false,
            protect_from_range: 0.0,
            fightback: true,
            runaway: false,
            attack_rate: 30,
            attack_rate_ranged: 30,
            heal_rate: 100,
            guarding_upper: 0,
            guarding_lower: 0,
            guarded_npc: -1,
            needs_ammo: false,
            safe_shot: true,
            respawn_time: 100,
            chase_range: 100.0,
            spawn_point: None,
            avoid_return_point: None,
            drops: Vec::new(),
            drop_chances: Vec::new(),
            enemy_drops: false,
            enemy_target_time: 0,
            speed: 1.0,
            warning_text: String::new(),
            greeting_text: String::new(),
            greet_range: 10.0,
            greet_rate: 100,
            autoswitch: false,
            squad: None,
            accuracy: 0.0,
            realistic: false,
            ignore_los: false,
            reach: 3.0,
            projectile_range: 100.0,
            guard_distance_minimum: 7.0,
            guard_selection_range: 4.0,
            retain_target: false,
            weapon_damage: BTreeMap::new(),
            weapon_redirects: BTreeMap::new(),
            disable_teleporting: false,
            reaction_slowdown: 0,
            allow_knockback: true,
            worldguard_region: None,
            death_xp: 0,
            unknown: Vec::new(),
        }
    }
}

/// Every key this port reads, so the rest is kept as unknown.
const KNOWN: &[&str] = &[
    "stats_ticksSpawned", "stats_timesSpawned", "stats_arrowsFired", "stats_potionsThrow", "stats_fireballsFired",
    "stats_snowballsThrown", "stats_eggsThrown", "stats_skullsThrown", "stats_llamaSpitShot",
    "stats_shulkerBulletsShot", "stats_evokerFangsSpawned", "stats_pearlsUsed", "stats_punches",
    "stats_attackAttempts", "stats_damageTaken", "stats_damageGiven", "allTargets", "allIgnores", "allAvoids",
    "avoidRange", "range", "damage", "armor", "health", "ranged_chase", "close_chase", "invincible", "protected",
    "protectFromRange", "fightback", "runaway", "attackRate", "attackRateRanged", "healRate", "guardingUpper",
    "guardingLower", "guardedNPC", "needsAmmo", "safeShot", "respawnTime", "chaseRange", "spawnPoint",
    "avoidReturnPoint", "drops", "dropChances", "enemyDrops", "enemyTargetTime", "speed", "warning_text",
    "greeting_text", "greet_range", "greet_rate", "autoswitch", "squad", "accuracy", "realistic", "ignore_los",
    "reach", "projectileRange", "guard_distance_minimum", "guard_selection_range", "retain_target",
    "weapon_damage_map", "weapon_redirect_map", "disable_teleporting", "reaction_slowdown", "allow_knockback",
    "worldguard_region", "death_xp",
    // Pre-2.0 layouts, read once by `updateOld` and dropped on save.
    "targets", "ignores",
];

impl SentinelData {
    /// A new trait (`onAttach`): the config's defaults, and the owner ignored.
    pub fn from_defaults(d: &Defaults, retain_target: bool, guard_min: f64, guard_select: f64) -> Self {
        let mut s = SentinelData {
            attack_rate: d.attack_rate,
            heal_rate: d.heal_rate,
            respawn_time: d.respawn_time,
            ranged_chase: d.ranged_chase_target,
            close_chase: d.close_chase_target,
            armor: d.armor,
            damage: d.damage,
            health: d.health,
            invincible: d.invincible,
            protect_from_ignores: d.protected,
            fightback: d.fightback,
            needs_ammo: d.needs_ammo,
            safe_shot: d.safe_shot,
            enemy_drops: d.enemy_drops,
            enemy_target_time: d.enemy_target_time,
            speed: if d.speed <= 0.0 { 1.0 } else { d.speed },
            autoswitch: d.autoswitch,
            reach: d.reach,
            projectile_range: d.projectile_range,
            avoid_range: d.avoid_range,
            runaway: d.runaway,
            greet_range: d.greet_range,
            greet_rate: d.greet_rate,
            retain_target,
            reaction_slowdown: d.reaction_slowdown,
            death_xp: d.death_xp,
            allow_knockback: d.allow_knockback,
            guard_distance_minimum: guard_min,
            guard_selection_range: guard_select,
            ..SentinelData::default()
        };
        s.all_ignores.targets.push(crate::target::OWNER.to_owned());
        s
    }

    /// The guarded player's UUID, if any (`getGuarding`).
    pub fn guarding(&self) -> Option<u128> {
        if self.guarding_upper == 0 && self.guarding_lower == 0 {
            None
        } else {
            Some(((self.guarding_upper as u64 as u128) << 64) | self.guarding_lower as u64 as u128)
        }
    }

    pub fn set_guarding(&mut self, uuid: Option<u128>) {
        match uuid {
            Some(u) => {
                self.guarding_upper = (u >> 64) as u64 as i64;
                self.guarding_lower = u as u64 as i64;
            }
            None => {
                self.guarding_upper = 0;
                self.guarding_lower = 0;
            }
        }
    }

    /// Reads the trait subtree (`load` + `updateOld`).
    pub fn load(node: &Node) -> Self {
        let d = SentinelData::default();
        let f = |k: &str, def: f64| node.f64(k).unwrap_or(def);
        let i = |k: &str, def: i64| node.i64(k).unwrap_or(def);
        let b = |k: &str, def: bool| node.bool(k).unwrap_or(def);
        let s = |k: &str| node.str(k).map(str::to_owned);
        let mut data = SentinelData {
            stats: Stats {
                ticks_spawned: i("stats_ticksSpawned", 0),
                times_spawned: i("stats_timesSpawned", 0),
                arrows_fired: i("stats_arrowsFired", 0),
                potions_thrown: i("stats_potionsThrow", 0),
                fireballs_fired: i("stats_fireballsFired", 0),
                snowballs_thrown: i("stats_snowballsThrown", 0),
                eggs_thrown: i("stats_eggsThrown", 0),
                skulls_thrown: i("stats_skullsThrown", 0),
                llama_spit_shot: i("stats_llamaSpitShot", 0),
                shulker_bullets_shot: i("stats_shulkerBulletsShot", 0),
                evoker_fangs_spawned: i("stats_evokerFangsSpawned", 0),
                pearls_used: i("stats_pearlsUsed", 0),
                punches: i("stats_punches", 0),
                attack_attempts: i("stats_attackAttempts", 0),
                damage_taken: f("stats_damageTaken", 0.0),
                damage_given: f("stats_damageGiven", 0.0),
            },
            all_targets: node.get("allTargets").map(TargetList::load).unwrap_or_default(),
            all_ignores: node.get("allIgnores").map(TargetList::load).unwrap_or_default(),
            all_avoids: node.get("allAvoids").map(TargetList::load).unwrap_or_default(),
            avoid_range: f("avoidRange", d.avoid_range),
            range: f("range", d.range),
            damage: f("damage", d.damage),
            armor: f("armor", d.armor),
            health: f("health", d.health),
            ranged_chase: b("ranged_chase", d.ranged_chase),
            close_chase: b("close_chase", d.close_chase),
            invincible: b("invincible", d.invincible),
            protect_from_ignores: b("protected", d.protect_from_ignores),
            protect_from_range: f("protectFromRange", d.protect_from_range),
            fightback: b("fightback", d.fightback),
            runaway: b("runaway", d.runaway),
            attack_rate: i("attackRate", d.attack_rate),
            attack_rate_ranged: i("attackRateRanged", d.attack_rate_ranged),
            heal_rate: i("healRate", d.heal_rate),
            guarding_upper: i("guardingUpper", 0),
            guarding_lower: i("guardingLower", 0),
            guarded_npc: i("guardedNPC", -1) as i32,
            needs_ammo: b("needsAmmo", d.needs_ammo),
            safe_shot: b("safeShot", d.safe_shot),
            respawn_time: i("respawnTime", d.respawn_time),
            chase_range: f("chaseRange", d.chase_range),
            spawn_point: node.get("spawnPoint").and_then(Location::load),
            avoid_return_point: node.get("avoidReturnPoint").and_then(Location::load),
            drops: match node.get("drops") {
                Some(Node::List(items)) => items.clone(),
                Some(Node::Map(entries)) => entries.iter().map(|(_, n)| n.clone()).collect(),
                _ => Vec::new(),
            },
            drop_chances: node.strings("dropChances").iter().filter_map(|c| c.parse().ok()).collect(),
            enemy_drops: b("enemyDrops", d.enemy_drops),
            enemy_target_time: i("enemyTargetTime", d.enemy_target_time),
            speed: f("speed", d.speed),
            warning_text: s("warning_text").unwrap_or_default(),
            greeting_text: s("greeting_text").unwrap_or_default(),
            greet_range: f("greet_range", d.greet_range),
            greet_rate: i("greet_rate", d.greet_rate),
            autoswitch: b("autoswitch", d.autoswitch),
            squad: s("squad").filter(|q| !q.is_empty()),
            accuracy: f("accuracy", d.accuracy),
            realistic: b("realistic", d.realistic),
            ignore_los: b("ignore_los", d.ignore_los),
            reach: f("reach", d.reach),
            projectile_range: f("projectileRange", d.projectile_range),
            guard_distance_minimum: f("guard_distance_minimum", d.guard_distance_minimum),
            guard_selection_range: f("guard_selection_range", d.guard_selection_range),
            retain_target: b("retain_target", d.retain_target),
            weapon_damage: map_entries(node.get("weapon_damage_map"))
                .filter_map(|(k, v)| Some((k, v.parse().ok()?)))
                .collect(),
            weapon_redirects: map_entries(node.get("weapon_redirect_map")).collect(),
            disable_teleporting: b("disable_teleporting", d.disable_teleporting),
            reaction_slowdown: i("reaction_slowdown", d.reaction_slowdown),
            allow_knockback: b("allow_knockback", d.allow_knockback),
            worldguard_region: s("worldguard_region").filter(|r| !r.is_empty()),
            death_xp: i("death_xp", d.death_xp),
            unknown: match node {
                Node::Map(entries) => entries
                    .iter()
                    .filter(|(k, _)| !KNOWN.contains(&k.as_str()) && !k.ends_with("Targets") && !k.ends_with("Ignores"))
                    .cloned()
                    .collect(),
                _ => Vec::new(),
            },
        };
        data.update_old(node);
        data
    }

    /// `updateOld`: the layout of Sentinel 1.x (`targets`, `ignores`, `playerNameTargets` ...).
    fn update_old(&mut self, node: &Node) {
        let Node::Map(entries) = node else { return };
        for (key, _) in entries {
            if key == "targets" {
                self.all_targets.targets.extend(node.strings(key));
                self.all_targets.recalculate();
            } else if key == "ignores" {
                self.all_ignores.targets.extend(node.strings(key));
                self.all_ignores.recalculate();
            }
            let (list, kind) = if let Some(kind) = key.strip_suffix("Targets") {
                (&mut self.all_targets, kind)
            } else if let Some(kind) = key.strip_suffix("Ignores") {
                (&mut self.all_ignores, kind)
            } else {
                continue;
            };
            let values = node.strings(key);
            let field = match kind {
                "playerName" => &mut list.by_player_name,
                "npcName" => &mut list.by_npc_name,
                "entityName" => &mut list.by_entity_name,
                "heldItem" => &mut list.by_held_item,
                "group" => &mut list.by_group,
                "event" => &mut list.by_event,
                "other" => &mut list.by_other,
                _ => continue,
            };
            field.extend(values);
        }
    }

    /// Writes the trait subtree in the field order Citizens uses. Every field is written
    /// (Citizens writes them all); the old 1.x keys are not.
    pub fn save(&self) -> Node {
        let mut n = Node::map();
        let st = &self.stats;
        for (k, v) in [
            ("stats_ticksSpawned", st.ticks_spawned),
            ("stats_timesSpawned", st.times_spawned),
            ("stats_arrowsFired", st.arrows_fired),
            ("stats_potionsThrow", st.potions_thrown),
            ("stats_fireballsFired", st.fireballs_fired),
            ("stats_snowballsThrown", st.snowballs_thrown),
            ("stats_eggsThrown", st.eggs_thrown),
            ("stats_skullsThrown", st.skulls_thrown),
            ("stats_llamaSpitShot", st.llama_spit_shot),
            ("stats_shulkerBulletsShot", st.shulker_bullets_shot),
            ("stats_evokerFangsSpawned", st.evoker_fangs_spawned),
            ("stats_pearlsUsed", st.pearls_used),
            ("stats_punches", st.punches),
            ("stats_attackAttempts", st.attack_attempts),
        ] {
            n.set(k, Node::scalar(v));
        }
        n.set("stats_damageTaken", Node::double(st.damage_taken));
        n.set("stats_damageGiven", Node::double(st.damage_given));
        n.set("allTargets", self.all_targets.save());
        n.set("allIgnores", self.all_ignores.save());
        n.set("allAvoids", self.all_avoids.save());
        let num = |n: &mut Node, k: &str, v: f64| n.set(k, Node::double(v));
        let int = |n: &mut Node, k: &str, v: i64| n.set(k, Node::scalar(v));
        let flag = |n: &mut Node, k: &str, v: bool| n.set(k, Node::scalar(v));
        num(&mut n, "avoidRange", self.avoid_range);
        num(&mut n, "range", self.range);
        num(&mut n, "damage", self.damage);
        num(&mut n, "armor", self.armor);
        num(&mut n, "health", self.health);
        flag(&mut n, "ranged_chase", self.ranged_chase);
        flag(&mut n, "close_chase", self.close_chase);
        flag(&mut n, "invincible", self.invincible);
        flag(&mut n, "protected", self.protect_from_ignores);
        num(&mut n, "protectFromRange", self.protect_from_range);
        flag(&mut n, "fightback", self.fightback);
        flag(&mut n, "runaway", self.runaway);
        int(&mut n, "attackRate", self.attack_rate);
        int(&mut n, "attackRateRanged", self.attack_rate_ranged);
        int(&mut n, "healRate", self.heal_rate);
        int(&mut n, "guardingUpper", self.guarding_upper);
        int(&mut n, "guardingLower", self.guarding_lower);
        int(&mut n, "guardedNPC", i64::from(self.guarded_npc));
        flag(&mut n, "needsAmmo", self.needs_ammo);
        flag(&mut n, "safeShot", self.safe_shot);
        int(&mut n, "respawnTime", self.respawn_time);
        num(&mut n, "chaseRange", self.chase_range);
        if let Some(l) = &self.spawn_point {
            n.set("spawnPoint", l.save());
        }
        if let Some(l) = &self.avoid_return_point {
            n.set("avoidReturnPoint", l.save());
        }
        if !self.drops.is_empty() {
            n.set("drops", Node::List(self.drops.clone()));
        }
        if !self.drop_chances.is_empty() {
            n.set("dropChances", Node::List(self.drop_chances.iter().map(|c| Node::double(*c)).collect()));
        }
        flag(&mut n, "enemyDrops", self.enemy_drops);
        int(&mut n, "enemyTargetTime", self.enemy_target_time);
        num(&mut n, "speed", self.speed);
        n.set("warning_text", Node::string(&self.warning_text));
        n.set("greeting_text", Node::string(&self.greeting_text));
        num(&mut n, "greet_range", self.greet_range);
        int(&mut n, "greet_rate", self.greet_rate);
        flag(&mut n, "autoswitch", self.autoswitch);
        if let Some(q) = &self.squad {
            n.set("squad", Node::string(q));
        }
        num(&mut n, "accuracy", self.accuracy);
        flag(&mut n, "realistic", self.realistic);
        flag(&mut n, "ignore_los", self.ignore_los);
        num(&mut n, "reach", self.reach);
        num(&mut n, "projectileRange", self.projectile_range);
        num(&mut n, "guard_distance_minimum", self.guard_distance_minimum);
        num(&mut n, "guard_selection_range", self.guard_selection_range);
        flag(&mut n, "retain_target", self.retain_target);
        if !self.weapon_damage.is_empty() {
            n.set(
                "weapon_damage_map",
                Node::Map(self.weapon_damage.iter().map(|(k, v)| (k.clone(), Node::double(*v))).collect()),
            );
        }
        if !self.weapon_redirects.is_empty() {
            n.set(
                "weapon_redirect_map",
                Node::Map(self.weapon_redirects.iter().map(|(k, v)| (k.clone(), Node::string(v))).collect()),
            );
        }
        flag(&mut n, "disable_teleporting", self.disable_teleporting);
        int(&mut n, "reaction_slowdown", self.reaction_slowdown);
        flag(&mut n, "allow_knockback", self.allow_knockback);
        if let Some(r) = &self.worldguard_region {
            n.set("worldguard_region", Node::string(r));
        }
        int(&mut n, "death_xp", self.death_xp);
        for (k, v) in &self.unknown {
            n.set(k, v.clone());
        }
        n
    }
}

fn map_entries(node: Option<&Node>) -> impl Iterator<Item = (String, String)> + '_ {
    let entries: &[(String, Node)] = match node {
        Some(Node::Map(e)) => e,
        _ => &[],
    };
    entries.iter().filter_map(|(k, v)| match v {
        Node::Scalar(s) | Node::Str(s) => Some((k.clone(), s.clone())),
        _ => None,
    })
}

/// A drop as this port reads Bukkit's serialized stack: its material and amount, and from the
/// stack's `meta` its custom name, lore and enchantments.
#[derive(Clone, Debug, PartialEq)]
pub struct DropItem {
    /// `minecraft:bread`.
    pub id: String,
    pub count: u32,
    /// The custom name as plain text.
    pub name: Option<String>,
    pub lore: Vec<String>,
    /// `minecraft:sharpness`, level.
    pub enchants: Vec<(String, i32)>,
}

/// Bukkit's old enchantment names that differ from the key.
fn enchant_key(bukkit: &str) -> String {
    let lower = bukkit.to_lowercase();
    let key = match lower.as_str() {
        "damage_all" => "sharpness",
        "damage_undead" => "smite",
        "damage_arthropods" => "bane_of_arthropods",
        "arrow_damage" => "power",
        "arrow_knockback" => "punch",
        "arrow_fire" => "flame",
        "arrow_infinite" => "infinity",
        "protection_environmental" => "protection",
        "protection_fire" => "fire_protection",
        "protection_fall" => "feather_falling",
        "protection_explosions" => "blast_protection",
        "protection_projectile" => "projectile_protection",
        "oxygen" => "respiration",
        "water_worker" => "aqua_affinity",
        "dig_speed" => "efficiency",
        "durability" => "unbreaking",
        "loot_bonus_blocks" => "fortune",
        "loot_bonus_mobs" => "looting",
        "luck" => "luck_of_the_sea",
        "binding_curse" => "binding_curse",
        other => other,
    };
    // A namespaced key (`minecraft:sharpness`) is kept.
    if key.contains(':') { key.to_owned() } else { format!("minecraft:{key}") }
}

/// A text stored as Bukkit saves it: plain or legacy `§` text, or a component as JSON.
fn plain_name(s: &str) -> String {
    let s = s.trim();
    if s.starts_with('{') || s.starts_with('[') {
        fn words(v: &serde_json::Value) -> String {
            match v {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Array(a) => a.iter().map(words).collect(),
                serde_json::Value::Object(o) => {
                    let mut t = o.get("text").map(words).unwrap_or_default();
                    if let Some(e) = o.get("extra") {
                        t.push_str(&words(e));
                    }
                    t
                }
                _ => String::new(),
            }
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
            return words(&v);
        }
    }
    // Legacy colour codes are dropped.
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\u{a7}' {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out
}

impl DropItem {
    pub fn from_node(node: &Node) -> Option<Self> {
        let ty = node.str("type")?;
        let name = node.str("meta.display-name").map(plain_name).filter(|n| !n.is_empty());
        let lore = node.strings("meta.lore").iter().map(|l| plain_name(l)).collect();
        let enchants = match node.get("meta.enchants") {
            Some(Node::Map(entries)) => entries
                .iter()
                .filter_map(|(k, _)| Some((enchant_key(k), node.i64(&format!("meta.enchants.{k}"))? as i32)))
                .collect(),
            _ => Vec::new(),
        };
        Some(DropItem {
            id: format!("minecraft:{}", ty.to_lowercase()),
            count: node.i64("amount").unwrap_or(1).clamp(1, 99) as u32,
            name,
            lore,
            enchants,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{parse_yaml, to_yaml};

    /// The shape of an arena fighter's trait (numbers made up).
    const FIGHTER: &str = "\
stats_punches: 10
stats_attackAttempts: 50
stats_ticksSpawned: 1000
stats_damageGiven: 12.5
stats_timesSpawned: 3
stats_damageTaken: 4.0
respawnTime: 0
allTargets:
  targets:
  - NPC
allIgnores:
  byNpcName:
  - '3'
  - '4'
  targets:
  - OWNER
autoswitch: true
avoidReturnPoint:
  world: world
  worldid: 00000000-0000-0000-0000-000000000001
  x: 12.5
  y: 70.0
  z: -30.5
  pitch: 0.0
  yaw: 0.0
";

    #[test]
    fn drops_keep_name_lore_and_enchants() {
        let n = parse_yaml(
            "type: DIAMOND_SWORD\namount: 2\nmeta:\n  display-name: '{\"text\":\"Blade\"}'\n  lore:\n  - \"\u{a7}7old\"\n  enchants:\n    DAMAGE_ALL: 3\n    minecraft:unbreaking: 1\n",
        )
        .unwrap();
        let d = DropItem::from_node(&n).unwrap();
        assert_eq!(d.id, "minecraft:diamond_sword");
        assert_eq!(d.count, 2);
        assert_eq!(d.name.as_deref(), Some("Blade"));
        assert_eq!(d.lore, vec!["old".to_owned()]);
        assert!(d.enchants.contains(&("minecraft:sharpness".to_owned(), 3)));
        assert!(d.enchants.contains(&("minecraft:unbreaking".to_owned(), 1)));
        // A plain stack still reads.
        let p = DropItem::from_node(&parse_yaml("type: BREAD\n").unwrap()).unwrap();
        assert_eq!((p.count, p.name, p.enchants.len()), (1, None, 0));
    }

    #[test]
    fn reads_a_fighter() {
        let d = SentinelData::load(&parse_yaml(FIGHTER).unwrap());
        assert_eq!(d.respawn_time, 0);
        assert_eq!(d.stats.punches, 10);
        assert_eq!(d.all_targets.targets, vec!["NPC"]);
        assert_eq!(d.all_ignores.by_npc_name, vec!["3", "4"]);
        assert_eq!(d.all_ignores.targets, vec!["OWNER"]);
        assert!(d.autoswitch);
        // Missing keys read as the Java defaults.
        assert_eq!(d.health, 20.0);
        assert_eq!(d.speed, 1.0);
        assert_eq!(d.reach, 3.0);
        assert_eq!(d.avoid_return_point.as_ref().unwrap().z, -30.5);
        assert!(d.unknown.is_empty());
    }

    #[test]
    fn save_then_load_is_the_same() {
        let d = SentinelData::load(&parse_yaml(FIGHTER).unwrap());
        let text = to_yaml(&d.save(), 0);
        let again = SentinelData::load(&parse_yaml(&text).unwrap());
        assert_eq!(d, again);
        assert!(text.contains("byNpcName:\n  - '3'\n"));
    }

    #[test]
    fn old_layout() {
        let n = parse_yaml("targets:\n- players\nnpcNameIgnores:\n- guard\n").unwrap();
        let d = SentinelData::load(&n);
        assert_eq!(d.all_targets.targets, vec!["PLAYER"]);
        assert_eq!(d.all_ignores.by_npc_name, vec!["guard"]);
        let saved = d.save();
        assert!(saved.get("targets").is_none());
    }

    #[test]
    fn guarding_uuid_halves() {
        let mut d = SentinelData::default();
        let u = 0x0123_4567_89ab_cdef_fedc_ba98_7654_3210u128;
        d.set_guarding(Some(u));
        assert_eq!(d.guarding(), Some(u));
        d.set_guarding(None);
        assert_eq!(d.guarding(), None);
    }

    #[test]
    fn doubles_keep_their_decimal_point() {
        // Review S1: `20` / `1` load as Integer on Paper; Sentinel casts these to Double.
        let mut d = SentinelData::load(&parse_yaml(FIGHTER).unwrap());
        d.drop_chances = vec![1.0, 0.5];
        d.weapon_damage.insert("diamond_sword".into(), 7.0);
        d.avoid_return_point.as_mut().unwrap().x = 12.0;
        d.avoid_return_point.as_mut().unwrap().yaw = 90.0;
        let text = to_yaml(&d.save(), 0);
        for want in ["health: 20.0", "- 1.0", "- 0.5", "diamond_sword: 7.0", "x: 12.0", "yaw: 90.0", "speed: 1.0"] {
            assert!(text.contains(want), "{want} missing in:\n{text}");
        }
        assert!(text.contains("respawnTime: 0\n"), "integers stay integers");
        let again = SentinelData::load(&parse_yaml(&text).unwrap());
        assert_eq!(again.drop_chances, vec![1.0, 0.5]);
    }
}

