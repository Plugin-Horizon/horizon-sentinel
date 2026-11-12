//! Damage numbers: what a Sentinel deals (`getDamage`), how much its armor blocks (`getArmor`),
//! its knockback, and the decisions on a hit to or from a Sentinel (`whenAttacksAreHappening*`,
//! `whenAttacksHappened`). Translated from Sentinel (MIT, see LICENSE); the tables are Sentinel's
//! 1.16+ values, not vanilla's attribute numbers.

use crate::data::SentinelData;
use crate::view::{EntityView, ItemView, Vec3};

/// `WEAPON_DAMAGE_MULTIPLIERS` (by item path).
fn weapon_multiplier(item: &str) -> Option<f64> {
    let path = item.split_once(':').map_or(item, |(_, p)| p);
    Some(match path {
        "netherite_sword" | "trident" => 8.0,
        "diamond_sword" => 7.0,
        "iron_sword" | "book" => 6.0,
        "stone_sword" => 5.0,
        "golden_sword" | "wooden_sword" => 4.0,
        p if p.ends_with("_pickaxe") => 3.0,
        p if p.ends_with("_axe") => 9.0,
        _ => return None,
    })
}

/// `ARMOR_PROTECTION_MULTIPLIERS`.
fn armor_value(item: &str) -> f64 {
    let path = item.split_once(':').map_or(item, |(_, p)| p);
    match path {
        "netherite_helmet" | "diamond_helmet" => 0.12,
        "golden_helmet" | "iron_helmet" | "chainmail_helmet" | "turtle_helmet" => 0.08,
        "leather_helmet" => 0.04,
        "netherite_chestplate" | "diamond_chestplate" => 0.32,
        "iron_chestplate" => 0.24,
        "golden_chestplate" | "chainmail_chestplate" => 0.20,
        "leather_chestplate" => 0.12,
        "netherite_leggings" | "diamond_leggings" => 0.24,
        "iron_leggings" => 0.20,
        "chainmail_leggings" => 0.16,
        "golden_leggings" => 0.12,
        "leather_leggings" => 0.08,
        "netherite_boots" | "diamond_boots" => 0.12,
        "iron_boots" => 0.08,
        "golden_boots" | "leather_boots" | "chainmail_boots" => 0.04,
        _ => 0.0,
    }
}

pub fn is_bow(item: &str) -> bool {
    matches!(item, "minecraft:bow" | "minecraft:crossbow")
}

pub fn is_shield(item: &str) -> bool {
    item == "minecraft:shield"
}

pub fn is_axe(item: &str) -> bool {
    item.ends_with("_axe") && !item.ends_with("_pickaxe")
}

/// What the held item makes the NPC do (`SentinelItemHelper.uses*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponKind {
    Melee,
    Bow,
    Snowball,
    Trident,
    Potion,
    Egg,
    LlamaSpit,
    ShulkerBullet,
    Pearl,
    WitherSkull,
    Fireball,
    FangsBook,
    Lightning,
    Spectral,
}

/// `SentinelItemHelper`: the weapon kind of a held item, after `weapon_redirect_map`.
pub fn weapon_kind(data: &SentinelData, item: Option<&ItemView>, skull_allowed: bool) -> WeaponKind {
    let Some(item) = item else { return WeaponKind::Melee };
    let mut id = item.id.clone();
    let path = id.split_once(':').map_or(id.as_str(), |(_, p)| p).to_owned();
    if let Some(to) = data.weapon_redirects.get(&path) {
        id = format!("minecraft:{}", to.to_lowercase());
    }
    let path = id.split_once(':').map_or(id.as_str(), |(_, p)| p);
    match path {
        "bow" | "crossbow" => WeaponKind::Bow,
        "snowball" => WeaponKind::Snowball,
        "trident" => WeaponKind::Trident,
        "potion" | "splash_potion" | "lingering_potion" => WeaponKind::Potion,
        "egg" => WeaponKind::Egg,
        "white_dye" => WeaponKind::LlamaSpit,
        "shulker_shell" if skull_allowed => WeaponKind::ShulkerBullet,
        "ender_pearl" => WeaponKind::Pearl,
        "wither_skeleton_skull" | "wither_skeleton_wall_skull" if skull_allowed => WeaponKind::WitherSkull,
        "blaze_rod" => WeaponKind::Fireball,
        "book" if skull_allowed => WeaponKind::FangsBook,
        "nether_star" => WeaponKind::Lightning,
        "spectral_arrow" => WeaponKind::Spectral,
        _ => WeaponKind::Melee,
    }
}

impl WeaponKind {
    pub fn is_ranged(self) -> bool {
        self != WeaponKind::Melee
    }
}

/// `getDamage(forRangedAttacks, weapon)`. A `weapon_damage_map` entry for the weapon also
/// becomes the NPC's damage, as in Sentinel (it writes the field).
pub fn damage(data: &mut SentinelData, ranged: bool, weapon: Option<&ItemView>) -> f64 {
    let Some(weapon) = weapon else {
        return if data.damage >= 0.0 { data.damage } else { 1.0 };
    };
    let material = weapon.id.split_once(':').map_or(weapon.id.as_str(), |(_, p)| p).to_lowercase();
    if let Some(custom) = data.weapon_damage.get(&material) {
        data.damage = *custom;
    }
    if data.damage >= 0.0 {
        return data.damage;
    }
    let multiplier = 1.0 + f64::from(weapon.sharpness) * 0.2;
    if is_bow(&weapon.id) {
        if !ranged {
            return 1.0;
        }
        return 6.0 * (1.0 + f64::from(weapon.power) * 0.3);
    }
    match weapon_multiplier(&weapon.id) {
        Some(m) => multiplier * m,
        None => multiplier,
    }
}

/// `getArmor(entity)`: the share of damage blocked, from the NPC's `armor` setting or what it
/// wears (at most 0.8).
pub fn armor(data: &SentinelData, worn: &[Option<ItemView>; 4]) -> f64 {
    if data.armor >= 0.0 {
        return data.armor;
    }
    let sum: f64 = worn.iter().flatten().map(|i| armor_value(&i.id)).sum();
    sum.min(0.80)
}

/// `SentinelWeaponHelper.knockback`: the velocity the hit entity gets.
pub fn knockback(attacker: Vec3, victim: Vec3, victim_velocity: Vec3, resistance: f64, force: f64) -> Vec3 {
    let mut force = force;
    if resistance > 0.0 && resistance <= 1.0 {
        force *= 1.0 - resistance;
    }
    let mut d = [victim[0] - attacker[0], victim[1] - attacker[1], victim[2] - attacker[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if len > 0.0 {
        d = [d[0] / len, d[1] / len, d[2] / len];
    }
    let speed = (victim_velocity[0].powi(2) + victim_velocity[1].powi(2) + victim_velocity[2].powi(2)).sqrt();
    let scale = 0.5 / speed.max(1.0);
    let rel = [d[0] * scale * force, 0.75 * scale, d[2] * scale * force];
    [
        victim_velocity[0] * 0.25 + rel[0],
        victim_velocity[1] * 0.25 + rel[1],
        victim_velocity[2] * 0.25 + rel[2],
    ]
}

/// What happens to a hit (the decision of the damage hooks).
#[derive(Clone, Debug, PartialEq)]
pub enum HitDecision {
    /// Let it through unchanged.
    Pass,
    /// Let it through with this amount.
    Amount(f32),
    Cancel,
}

/// The damage types Sentinel treats as burning (`whenImHurt`).
pub fn is_burn(damage_type: &str) -> bool {
    matches!(damage_type, "minecraft:in_fire" | "minecraft:on_fire" | "minecraft:lava" | "minecraft:campfire")
}

/// A hit on a Sentinel, before vanilla applies it (`whenAttacksAreHappeningToMe` with
/// `whenImHurt`): fire is taken at most once a second, armor and a raised shield reduce it.
/// Vanilla armor still applies after this; Sentinel zeroed vanilla's modifiers instead (see
/// GAPS.md: the hook has no modifier breakdown).
pub fn incoming_to_sentinel(
    data: &SentinelData,
    worn: &[Option<ItemView>; 4],
    damage_type: &str,
    amount: f32,
    ticks_since_burn: &mut i64,
    blocked_by_shield: bool,
) -> HitDecision {
    if is_burn(damage_type) {
        if *ticks_since_burn <= 20 {
            return HitDecision::Cancel;
        }
        *ticks_since_burn = 0;
    }
    if data.invincible {
        return HitDecision::Cancel;
    }
    let mut level = armor(data, worn);
    if blocked_by_shield {
        level = (1.0 - level) * 0.5 + level;
    }
    if level <= 0.0 {
        return HitDecision::Pass;
    }
    HitDecision::Amount(((1.0 - level) * f64::from(amount)) as f32)
}

/// `hitIsBlocked`: a raised shield turned towards the attacker blocks, unless an axe disabled it.
pub fn shield_blocks(blocking: bool, me: &EntityView, attacker: &EntityView) -> bool {
    blocking
        && me.off_hand.as_ref().or(me.main_hand.as_ref()).is_some_and(|i| is_shield(&i.id))
        && crate::view::looking_towards(me.eye, me.yaw, me.pitch, attacker.pos, 45.0, 45.0)
}

/// Rolls the drops of a dead Sentinel (`getDrops`): each with its chance (missing = always).
pub fn roll_drops(data: &SentinelData, mut random: impl FnMut() -> f64) -> Vec<crate::data::DropItem> {
    data.drops
        .iter()
        .enumerate()
        .filter(|(i, _)| match data.drop_chances.get(*i) {
            Some(chance) => random() <= *chance,
            None => true,
        })
        .filter_map(|(_, n)| crate::data::DropItem::from_node(n))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damage_from_weapons() {
        let mut d = SentinelData::default();
        assert_eq!(damage(&mut d, false, None), 1.0);
        let mut sword = ItemView::of("minecraft:diamond_sword");
        assert_eq!(damage(&mut d, false, Some(&sword)), 7.0);
        sword.sharpness = 5;
        assert!((damage(&mut d, false, Some(&sword)) - 14.0).abs() < 1e-9);
        let bow = ItemView::of("minecraft:bow");
        assert_eq!(damage(&mut d, false, Some(&bow)), 1.0);
        assert_eq!(damage(&mut d, true, Some(&bow)), 6.0);
        d.damage = 3.0;
        assert_eq!(damage(&mut d, false, Some(&sword)), 3.0);
    }

    #[test]
    fn weapon_damage_map_overrides() {
        let mut d = SentinelData::default();
        d.weapon_damage.insert("stick".into(), 12.0);
        assert_eq!(damage(&mut d, false, Some(&ItemView::of("minecraft:stick"))), 12.0);
    }

    #[test]
    fn armor_from_worn_items_is_capped() {
        let d = SentinelData::default();
        let full = [
            Some(ItemView::of("minecraft:netherite_helmet")),
            Some(ItemView::of("minecraft:netherite_chestplate")),
            Some(ItemView::of("minecraft:netherite_leggings")),
            Some(ItemView::of("minecraft:netherite_boots")),
        ];
        assert!((armor(&d, &full) - 0.8).abs() < 1e-9);
        let none: [Option<ItemView>; 4] = Default::default();
        assert_eq!(armor(&d, &none), 0.0);
    }

    #[test]
    fn burning_once_a_second() {
        let d = SentinelData::default();
        let none: [Option<ItemView>; 4] = Default::default();
        let mut t = 0;
        assert_eq!(incoming_to_sentinel(&d, &none, "minecraft:on_fire", 1.0, &mut t, false), HitDecision::Cancel);
        t = 25;
        assert_eq!(incoming_to_sentinel(&d, &none, "minecraft:on_fire", 1.0, &mut t, false), HitDecision::Pass);
        assert_eq!(t, 0);
    }

    #[test]
    fn redirects_change_the_weapon_kind() {
        let mut d = SentinelData::default();
        assert_eq!(weapon_kind(&d, Some(&ItemView::of("minecraft:stick")), true), WeaponKind::Melee);
        d.weapon_redirects.insert("stick".into(), "BOW".into());
        assert_eq!(weapon_kind(&d, Some(&ItemView::of("minecraft:stick")), true), WeaponKind::Bow);
    }
}
