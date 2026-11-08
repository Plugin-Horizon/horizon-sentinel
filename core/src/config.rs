//! The mod's config: Sentinel's `config.yml` (`sentinel defaults`, `random`, `update rate`) as
//! TOML, `config/sentinel-common.toml`. Keys keep Sentinel's names with dashes for spaces.
//! [`Config::from_sentinel_yaml`] reads an existing Sentinel `config.yml` for a move.

use crate::node::{parse_yaml, Node};

/// `sentinel defaults`: what a newly added trait starts with.
#[derive(Clone, Debug, PartialEq)]
pub struct Defaults {
    pub close_chase_target: bool,
    pub ranged_chase_target: bool,
    pub damage: f64,
    pub armor: f64,
    pub health: f64,
    pub invincible: bool,
    /// Ticks.
    pub attack_rate: i64,
    pub heal_rate: i64,
    pub protected: bool,
    pub protect_from_range: f64,
    pub fightback: bool,
    pub needs_ammo: bool,
    pub safe_shot: bool,
    pub respawn_time: i64,
    pub enemy_drops: bool,
    pub enemy_target_time: i64,
    pub speed: f64,
    pub autoswitch: bool,
    pub accuracy: f64,
    pub realistic: bool,
    pub reach: f64,
    pub projectile_range: f64,
    pub avoid_range: f64,
    pub runaway: bool,
    pub greet_range: f64,
    pub greet_rate: i64,
    pub reaction_slowdown: i64,
    pub allow_knockback: bool,
    pub death_xp: i64,
}

impl Default for Defaults {
    /// The values of Sentinel's shipped `config.yml`.
    fn default() -> Self {
        Defaults {
            close_chase_target: true,
            ranged_chase_target: false,
            damage: -1.0,
            armor: -1.0,
            health: 20.0,
            invincible: false,
            attack_rate: 30,
            heal_rate: 100,
            protected: false,
            protect_from_range: 0.0,
            fightback: true,
            needs_ammo: false,
            safe_shot: true,
            respawn_time: 100,
            enemy_drops: false,
            enemy_target_time: 0,
            speed: 1.5,
            autoswitch: false,
            accuracy: 0.0,
            realistic: false,
            reach: 3.0,
            projectile_range: 100.0,
            avoid_range: 10.0,
            runaway: false,
            greet_range: 10.0,
            greet_rate: 100,
            reaction_slowdown: 0,
            allow_knockback: true,
            death_xp: 0,
        }
    }
}

/// `random`: server-wide behaviour switches.
#[derive(Clone, Debug, PartialEq)]
pub struct Random {
    pub run_away_time: i64,
    /// A sound id (`minecraft:entity.villager.yes`) or empty for none.
    pub spectral_sound: String,
    pub workaround_damage: bool,
    pub do_native_attacks: bool,
    pub workaround_drops: bool,
    pub workaround_bukkit_events: bool,
    pub workaround_entity_chase_pathfinder: bool,
    pub death_messages: bool,
    pub shoot_speed_minimum: i64,
    pub enforce_damage: bool,
    pub skull_allowed: bool,
    pub clever_ticks: i64,
    pub ignore_invisible_targets: bool,
    pub no_guard_damage: bool,
    pub enhance_los_traces: bool,
    pub guard_selection_range: f64,
    pub guard_minimum: f64,
    pub arrow_cleanup_time: i64,
    pub block_sunburn: bool,
    pub retain_target: bool,
    pub prevent_explosion_block_damage: bool,
    pub auto_correct_pathfinder_mode: bool,
    pub allow_armor_stand_targets: bool,
    /// Server-wide `protected` (the plugin field `protectFromIgnores`).
    pub protect_from_ignores: bool,
}

impl Default for Random {
    fn default() -> Self {
        Random {
            run_away_time: 1200,
            spectral_sound: "minecraft:entity.villager.yes".into(),
            workaround_damage: false,
            do_native_attacks: true,
            workaround_drops: false,
            workaround_bukkit_events: false,
            workaround_entity_chase_pathfinder: true,
            death_messages: true,
            shoot_speed_minimum: 20,
            enforce_damage: false,
            skull_allowed: true,
            clever_ticks: 10,
            ignore_invisible_targets: true,
            no_guard_damage: true,
            enhance_los_traces: true,
            guard_selection_range: 4.0,
            guard_minimum: 7.0,
            arrow_cleanup_time: 200,
            block_sunburn: true,
            retain_target: false,
            prevent_explosion_block_damage: true,
            auto_correct_pathfinder_mode: true,
            allow_armor_stand_targets: false,
            protect_from_ignores: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub defaults: Defaults,
    pub random: Random,
    /// Ticks between two updates of every Sentinel (`update rate`).
    pub update_rate: i64,
}

impl Default for Config {
    fn default() -> Self {
        Config { defaults: Defaults::default(), random: Random::default(), update_rate: 10 }
    }
}

pub const DEFAULT_TOML: &str = include_str!("../default.toml");

/// A value source: TOML (`a-b`) or Sentinel's YAML (`a b`).
trait Source {
    fn f(&self, section: &str, key: &str) -> Option<f64>;
    fn b(&self, section: &str, key: &str) -> Option<bool>;
    fn s(&self, section: &str, key: &str) -> Option<String>;
}

struct Toml(toml::Table);

impl Toml {
    fn get(&self, section: &str, key: &str) -> Option<&toml::Value> {
        let key = key.replace(' ', "-");
        if section.is_empty() {
            return self.0.get(&key);
        }
        let mut at = self.0.get(section)?;
        for part in key.split('.') {
            at = at.get(part)?;
        }
        Some(at)
    }
}

impl Source for Toml {
    fn f(&self, section: &str, key: &str) -> Option<f64> {
        match self.get(section, key)? {
            toml::Value::Float(f) => Some(*f),
            toml::Value::Integer(i) => Some(*i as f64),
            _ => None,
        }
    }
    fn b(&self, section: &str, key: &str) -> Option<bool> {
        self.get(section, key)?.as_bool()
    }
    fn s(&self, section: &str, key: &str) -> Option<String> {
        self.get(section, key)?.as_str().map(str::to_owned)
    }
}

struct Yaml(Node);

impl Yaml {
    fn path(section: &str, key: &str) -> String {
        let section = match section {
            "defaults" => "sentinel defaults",
            other => other,
        };
        if section.is_empty() { key.to_owned() } else { format!("{section}.{key}") }
    }
}

impl Source for Yaml {
    fn f(&self, section: &str, key: &str) -> Option<f64> {
        self.0.f64(&Yaml::path(section, key))
    }
    fn b(&self, section: &str, key: &str) -> Option<bool> {
        self.0.bool(&Yaml::path(section, key))
    }
    fn s(&self, section: &str, key: &str) -> Option<String> {
        self.0.str(&Yaml::path(section, key)).map(str::to_owned)
    }
}

/// Sentinel's `spectral sound` is a Bukkit `Sound` name (`ENTITY_VILLAGER_YES`).
fn sound_id(name: &str) -> String {
    if name.contains(':') || name.is_empty() {
        return name.to_owned();
    }
    let lower = name.to_lowercase();
    // ENTITY_VILLAGER_YES -> entity.villager.yes (the first two dots; good for most names).
    let mut parts = lower.splitn(3, '_');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(a), Some(b), Some(c)) => format!("minecraft:{a}.{b}.{}", c.replace('_', "_")),
        _ => format!("minecraft:{lower}"),
    }
}

impl Config {
    fn read(src: &dyn Source) -> Self {
        let d = Defaults::default();
        let r = Random::default();
        let df = |k: &str, v: f64| src.f("defaults", k).unwrap_or(v);
        let di = |k: &str, v: i64| src.f("defaults", k).map_or(v, |f| f as i64);
        let db = |k: &str, v: bool| src.b("defaults", k).unwrap_or(v);
        let rf = |k: &str, v: f64| src.f("random", k).unwrap_or(v);
        let ri = |k: &str, v: i64| src.f("random", k).map_or(v, |f| f as i64);
        let rb = |k: &str, v: bool| src.b("random", k).unwrap_or(v);
        Config {
            defaults: Defaults {
                close_chase_target: db("close chase target", d.close_chase_target),
                ranged_chase_target: db("ranged chase target", d.ranged_chase_target),
                damage: df("damage", d.damage),
                armor: df("armor", d.armor),
                health: df("health", d.health),
                invincible: db("invincible", d.invincible),
                attack_rate: di("attack rate", d.attack_rate),
                heal_rate: di("heal rate", d.heal_rate),
                protected: db("protected", d.protected),
                protect_from_range: df("protect from range", d.protect_from_range),
                fightback: db("fightback", d.fightback),
                needs_ammo: db("needs ammo", d.needs_ammo),
                safe_shot: db("safe shot", d.safe_shot),
                respawn_time: di("respawn time", d.respawn_time),
                enemy_drops: db("enemy drops", d.enemy_drops),
                enemy_target_time: di("enemy target time", d.enemy_target_time),
                speed: df("speed", d.speed),
                autoswitch: db("autoswitch", d.autoswitch),
                accuracy: df("accuracy", d.accuracy),
                realistic: db("realistic", d.realistic),
                reach: df("reach", d.reach),
                projectile_range: df("projectile range", d.projectile_range),
                avoid_range: df("avoid range", d.avoid_range),
                runaway: db("runaway", d.runaway),
                greet_range: df("greet range", d.greet_range),
                greet_rate: di("greet rate", d.greet_rate),
                reaction_slowdown: di("reaction slowdown", d.reaction_slowdown),
                allow_knockback: db("allow knockback", d.allow_knockback),
                death_xp: di("death xp", d.death_xp),
            },
            random: Random {
                run_away_time: ri("run away time", r.run_away_time),
                spectral_sound: src.s("random", "spectral sound").map_or(r.spectral_sound, |s| sound_id(&s)),
                workaround_damage: rb("workaround damage", r.workaround_damage),
                do_native_attacks: rb("do native attacks", r.do_native_attacks),
                workaround_drops: rb("workaround drops", r.workaround_drops),
                workaround_bukkit_events: rb("workaround bukkit events", r.workaround_bukkit_events),
                workaround_entity_chase_pathfinder: rb(
                    "workaround entity chase pathfinder",
                    r.workaround_entity_chase_pathfinder,
                ),
                death_messages: rb("death messages", r.death_messages),
                shoot_speed_minimum: ri("shoot speed minimum", r.shoot_speed_minimum),
                enforce_damage: rb("enforce damage", r.enforce_damage),
                skull_allowed: rb("skull allowed", r.skull_allowed),
                clever_ticks: ri("clever ticks", r.clever_ticks),
                ignore_invisible_targets: rb("ignore invisible targets", r.ignore_invisible_targets),
                no_guard_damage: rb("no guard damage", r.no_guard_damage),
                enhance_los_traces: rb("enhance los traces", r.enhance_los_traces),
                guard_selection_range: rf("guard follow distance.selection range", r.guard_selection_range),
                guard_minimum: rf("guard follow distance.minimum", r.guard_minimum),
                arrow_cleanup_time: ri("arrow cleanup time", r.arrow_cleanup_time),
                block_sunburn: rb("block sunburn", r.block_sunburn),
                retain_target: rb("retain target", r.retain_target),
                prevent_explosion_block_damage: rb(
                    "prevent explosion block damage",
                    r.prevent_explosion_block_damage,
                ),
                auto_correct_pathfinder_mode: rb("auto correct pathfinder mode", r.auto_correct_pathfinder_mode),
                allow_armor_stand_targets: rb("allow armor stand targets", r.allow_armor_stand_targets),
                protect_from_ignores: rb("protected", r.protect_from_ignores),
            },
            update_rate: src.f("", "update rate").map_or(10, |f| (f as i64).max(1)),
        }
    }

    /// Parses `sentinel-common.toml`; a missing key keeps its default.
    pub fn from_toml(text: &str) -> Result<Self, String> {
        let table: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
        Ok(Config::read(&Toml(table)))
    }

    /// Reads Sentinel's own `config.yml`.
    pub fn from_sentinel_yaml(text: &str) -> Result<Self, String> {
        let node = parse_yaml(text).map_err(|e| e.to_string())?;
        Ok(Config::read(&Yaml(node)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_file_is_the_default() {
        assert_eq!(Config::from_toml(DEFAULT_TOML).unwrap(), Config::default());
    }

    #[test]
    fn reads_sentinel_yaml() {
        let yaml = "\
sentinel defaults:
    speed: 1.5
    reach: 4.5
    protected: true
    respawn time: 100
random:
    spectral sound: ENTITY_VILLAGER_YES
    death messages: false
    guard follow distance:
        selection range: 4
        minimum: 7
update rate: 10
";
        let c = Config::from_sentinel_yaml(yaml).unwrap();
        assert_eq!(c.defaults.reach, 4.5);
        assert!(c.defaults.protected);
        assert!(!c.random.death_messages);
        assert_eq!(c.random.guard_minimum, 7.0);
        assert_eq!(c.random.spectral_sound, "minecraft:entity.villager.yes");
        assert_eq!(c.update_rate, 10);
    }
}
