//! Target types (`SentinelTarget`): the names `addtarget`/`addignore` take without a prefix
//! (`PLAYERS`, `MONSTERS`, `zombie`) and the entity kinds each one stands for.
//!
//! Translated from Sentinel's `SentinelTarget` and `SentinelVersionCompat` (MIT, see LICENSE),
//! for the 1.21+ entity set. Kinds are namespaced ids (`minecraft:zombie`) instead of Bukkit's
//! `EntityType` constants.

use std::collections::HashMap;
use std::sync::OnceLock;

/// One target type: its canonical name (what is saved, `targets: [NPC]`) and its kinds.
#[derive(Debug)]
pub struct TargetType {
    pub name: &'static str,
    pub kinds: Vec<&'static str>,
}

/// The two types that are not entity kinds.
pub const NPCS: &str = "NPC";
pub const OWNER: &str = "OWNER";
pub const PLAYERS: &str = "PLAYER";

/// (names, kinds without the `minecraft:` namespace). The first name is the canonical one.
/// Each name is also accepted with an `S` appended (`ZOMBIES`), as Sentinel registers them.
const SINGLE: &[(&[&str], &[&str])] = &[
    (&["NPC"], &[]),
    (&["OWNER"], &[]),
    (&["PLAYER"], &["player"]),
    (&["PIG"], &["pig"]),
    (&["OCELOT"], &["ocelot"]),
    (&["CAT"], &["cat"]),
    (&["COW"], &["cow"]),
    (&["RABBIT", "BUNNY", "BUNNIE"], &["rabbit"]),
    (&["SHEEP"], &["sheep"]),
    (&["CHICKEN", "DUCK"], &["chicken"]),
    (&["HORSE"], &["horse"]),
    (&["MUSHROOM_COW", "MUSHROOMCOW", "MOOSHROOM"], &["mooshroom"]),
    (&["IRON_GOLEM", "IRONGOLEM"], &["iron_golem"]),
    (&["SQUID"], &["squid"]),
    (&["VILLAGER"], &["villager"]),
    (&["WOLF", "WOLVE"], &["wolf"]),
    (&["SNOWMAN", "SNOWMEN"], &["snow_golem"]),
    (&["WITCH"], &["witch"]),
    (&["GUARDIAN"], &["guardian"]),
    (&["CREEPER"], &["creeper"]),
    (&["SKELETON"], &["skeleton"]),
    (&["ZOMBIE"], &["zombie"]),
    (&["MAGMA_CUBE", "MAGMACUBE"], &["magma_cube"]),
    (&["SILVERFISH", "SILVER_FISH", "SILVERFISHE", "SILVER_FISHE"], &["silverfish"]),
    (&["BAT"], &["bat"]),
    (&["BLAZE"], &["blaze"]),
    (&["GHAST"], &["ghast"]),
    (&["GIANT"], &["giant"]),
    (&["SLIME"], &["slime"]),
    (&["SPIDER"], &["spider"]),
    (&["CAVE_SPIDER", "CAVESPIDER"], &["cave_spider"]),
    (&["ENDERMAN", "ENDER_MAN", "ENDERMEN", "ENDER_MEN"], &["enderman"]),
    (&["ENDERMITE", "ENDER_MITE"], &["endermite"]),
    (&["WITHER"], &["wither"]),
    (&["ENDERDRAGON", "ENDER_DRAGON"], &["ender_dragon"]),
    (&["ARMORSTAND", "ARMOR_STAND"], &["armor_stand"]),
    (&["SHULKER"], &["shulker"]),
    (&["POLARBEAR", "POLAR_BEAR"], &["polar_bear"]),
    (&["VEX"], &["vex"]),
    (&["DONKEY"], &["donkey"]),
    (&["LLAMA"], &["llama"]),
    (&["MULE"], &["mule"]),
    (&["HUSK"], &["husk"]),
    (&["ELDER_GUARDIAN", "ELDERGUARDIAN"], &["elder_guardian"]),
    (&["EVOKER"], &["evoker"]),
    (&["SKELETON_HORSE", "SKELETONHORSE"], &["skeleton_horse"]),
    (&["STRAY"], &["stray"]),
    (&["ZOMBIE_VILLAGER", "ZOMBIEVILLAGER"], &["zombie_villager"]),
    (&["ZOMBIE_HORSE", "ZOMBIEHORSE"], &["zombie_horse"]),
    (&["WITHER_SKELETON", "WITHERSKELETON"], &["wither_skeleton"]),
    (&["VINDICATOR"], &["vindicator"]),
    (&["PARROT"], &["parrot"]),
    (&["ILLUSIONER"], &["illusioner"]),
    (&["DOLPHIN"], &["dolphin"]),
    (&["DROWNED"], &["drowned"]),
    (&["COD"], &["cod"]),
    (&["SALMON"], &["salmon"]),
    (&["PUFFERFISH", "PUFFERFISHE"], &["pufferfish"]),
    (&["TROPICAL_FISH", "TROPICALFISH", "TROPICAL_FISHE", "TROPICALFISHE"], &["tropical_fish"]),
    (&["FISH", "FISHE"], &["tropical_fish", "pufferfish", "salmon", "cod"]),
    (&["PHANTOM"], &["phantom"]),
    (&["TURTLE"], &["turtle"]),
    (&["RAVAGER"], &["ravager"]),
    (&["PILLAGER"], &["pillager"]),
    (&["PANDA"], &["panda"]),
    (&["TRADER_LLAMA", "TRADERLLAMA"], &["trader_llama"]),
    (&["WANDERING_TRADER", "WANDERINGTRADER", "TRADER"], &["wandering_trader"]),
    (&["FOX", "FOXE"], &["fox"]),
    (&["BEE"], &["bee"]),
    (&["HOGLIN"], &["hoglin"]),
    (&["PIGLIN"], &["piglin"]),
    (&["STRIDER"], &["strider"]),
    (&["ZOGLIN"], &["zoglin"]),
    (&["PIGLIN_BRUTE"], &["piglin_brute"]),
    (
        &[
            "ZOMBIFIED_PIGLIN", "ZOMBIFIEDPIGLIN", "ZOMBIE_PIGLIN", "ZOMBIEPIGLIN", "PIG_ZOMBIE",
            "PIGZOMBIE", "ZOMBIEPIGMAN", "ZOMBIEPIGMEN", "ZOMBIE_PIGMAN", "ZOMBIE_PIGMEN",
        ],
        &["zombified_piglin"],
    ),
    (&["GOAT"], &["goat"]),
    (&["AXOLOTL"], &["axolotl"]),
    (&["TADPOLE"], &["tadpole"]),
    (&["FROG"], &["frog"]),
    (&["ALLAY"], &["allay"]),
    (&["WARDEN"], &["warden"]),
    (&["CAMEL"], &["camel"]),
    (&["SNIFFER"], &["sniffer"]),
    (&["BREEZE"], &["breeze"]),
    (&["BOGGED", "BOGGEDSKELETON", "BOGGED_SKELETON"], &["bogged"]),
    (&["ARMADILLO"], &["armadillo"]),
];

/// `v1_21_passive()`.
const PASSIVE: &[&str] = &[
    "pig", "ocelot", "cow", "rabbit", "sheep", "chicken", "mooshroom", "horse", "iron_golem", "squid",
    "villager", "wolf", "snow_golem", "polar_bear", "donkey", "llama", "mule", "zombie_horse",
    "skeleton_horse", "parrot", "dolphin", "cod", "salmon", "pufferfish", "tropical_fish", "turtle",
    "cat", "panda", "trader_llama", "wandering_trader", "fox", "strider", "goat", "axolotl", "tadpole",
    "frog", "allay", "camel", "sniffer", "armadillo",
];

/// `v1_21_monsters()`.
const MONSTER: &[&str] = &[
    "guardian", "creeper", "skeleton", "zombie", "magma_cube", "silverfish", "bat", "blaze", "ghast",
    "giant", "slime", "spider", "cave_spider", "enderman", "endermite", "wither", "ender_dragon",
    "witch", "shulker", "vex", "husk", "elder_guardian", "evoker", "stray", "zombie_villager",
    "wither_skeleton", "vindicator", "illusioner", "drowned", "phantom", "ravager", "pillager", "bee",
    "hoglin", "piglin", "zoglin", "zombified_piglin", "piglin_brute", "warden", "breeze", "bogged",
];

const PASSIVE_NAMES: &[&str] = &["PASSIVE_MOB", "PASSIVEMOB", "GOODMOB", "GOOD_MOB", "FRIENDLYMOB", "FRIENDLY_MOB"];

/// Mob kinds that attack natively (`NATIVE_COMBAT_CAPABLE_TYPES`).
pub const NATIVE_COMBAT_KINDS: &[&str] = &["minecraft:zombie", "minecraft:skeleton"];

struct Registry {
    types: Vec<TargetType>,
    /// Every accepted name (upper case, also with `S`) -> index in `types`.
    by_name: HashMap<String, usize>,
}

fn registry() -> &'static Registry {
    static R: OnceLock<Registry> = OnceLock::new();
    R.get_or_init(|| {
        let mut types = Vec::new();
        let mut by_name = HashMap::new();
        let mut add = |names: &[&'static str], kinds: Vec<&'static str>| {
            let i = types.len();
            types.push(TargetType { name: names[0], kinds });
            for n in names {
                // A later registration of the same name wins, as in Sentinel's map.
                by_name.insert(n.to_string(), i);
                by_name.insert(format!("{n}S"), i);
            }
        };
        for (names, kinds) in SINGLE {
            add(names, kinds.to_vec());
        }
        add(PASSIVE_NAMES, PASSIVE.to_vec());
        let mut mobs = PASSIVE.to_vec();
        mobs.extend_from_slice(MONSTER);
        add(&["MOB"], mobs);
        add(&["MONSTER"], MONSTER.to_vec());
        Registry { types, by_name }
    })
}

/// The canonical name of a target type for any of its names (`zombies` -> `ZOMBIE`).
pub fn canonical(name: &str) -> Option<&'static str> {
    let r = registry();
    r.by_name.get(&name.to_uppercase()).map(|&i| r.types[i].name)
}

/// The type of a canonical (or any accepted) name.
pub fn by_name(name: &str) -> Option<&'static TargetType> {
    let r = registry();
    r.by_name.get(&name.to_uppercase()).map(|&i| &r.types[i])
}

/// The canonical names of every type whose kinds hold `kind` (`minecraft:zombie`).
pub fn types_of_kind(kind: &str) -> impl Iterator<Item = &'static str> + '_ {
    let short = kind.strip_prefix("minecraft:").unwrap_or(kind);
    registry().types.iter().filter(move |t| t.kinds.contains(&short)).map(|t| t.name)
}

/// All canonical names, for `/sentinel addtarget` completion and help.
pub fn all_names() -> impl Iterator<Item = &'static str> {
    registry().types.iter().map(|t| t.name)
}

/// Every name `addtarget` accepts, lower case (`SentinelPlugin.targetOptions.keySet()`):
/// aliases and the plurals with an `S`.
pub fn all_accepted_names() -> Vec<String> {
    let mut names: Vec<String> = registry().by_name.keys().map(|n| n.to_lowercase()).collect();
    names.sort();
    names
}

/// The Bukkit `EntityType.name()` of a kind (`minecraft:zombie_villager` -> `ZOMBIE_VILLAGER`),
/// which `entityname:` patterns match against when the entity has no custom name.
pub fn bukkit_type_name(kind: &str) -> String {
    let short = kind.split_once(':').map_or(kind, |(_, k)| k);
    match short {
        "mooshroom" => "MOOSHROOM".into(),
        "snow_golem" => "SNOW_GOLEM".into(),
        other => other.to_uppercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_plurals() {
        assert_eq!(canonical("players"), Some("PLAYER"));
        assert_eq!(canonical("Zombies"), Some("ZOMBIE"));
        assert_eq!(canonical("npcs"), Some("NPC"));
        assert_eq!(canonical("monsters"), Some("MONSTER"));
        assert_eq!(canonical("pigzombie"), Some("ZOMBIFIED_PIGLIN"));
        assert_eq!(canonical("nonsense"), None);
    }

    #[test]
    fn kinds_belong_to_types() {
        let t: Vec<_> = types_of_kind("minecraft:zombie").collect();
        assert!(t.contains(&"ZOMBIE") && t.contains(&"MONSTER") && t.contains(&"MOB"));
        assert!(!t.contains(&"PASSIVE_MOB"));
        let t: Vec<_> = types_of_kind("minecraft:player").collect();
        assert_eq!(t, vec!["PLAYER"]);
    }
}
