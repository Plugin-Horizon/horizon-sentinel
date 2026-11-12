//! What a Sentinel needs from the game, for one NPC at a time. The mod's helpers implement it
//! over Horizon's `Game`; the tests implement it over a small fake world.

use crate::view::{EntityView, Vec3};

/// Projectiles a Sentinel can launch (`SentinelWeaponHelper.fire*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Projectile {
    /// `minecraft:arrow` or `minecraft:spectral_arrow` / tipped, by the item id given.
    Arrow,
    Snowball,
    Trident,
    Potion,
    Egg,
    LlamaSpit,
    ShulkerBullet,
    Pearl,
    WitherSkull,
    SmallFireball,
    EvokerFangs,
}

impl Projectile {
    /// The entity kind the host summons.
    pub fn kind(self) -> &'static str {
        match self {
            Projectile::Arrow => "minecraft:arrow",
            Projectile::Snowball => "minecraft:snowball",
            Projectile::Trident => "minecraft:trident",
            Projectile::Potion => "minecraft:splash_potion",
            Projectile::Egg => "minecraft:egg",
            Projectile::LlamaSpit => "minecraft:llama_spit",
            Projectile::ShulkerBullet => "minecraft:shulker_bullet",
            Projectile::Pearl => "minecraft:ender_pearl",
            Projectile::WitherSkull => "minecraft:wither_skull",
            Projectile::SmallFireball => "minecraft:small_fireball",
            Projectile::EvokerFangs => "minecraft:evoker_fangs",
        }
    }
}

/// The game, as seen from one Sentinel NPC's body.
pub trait World {
    /// Living entities (players, mobs, NPC bodies) whose position is within `range` (a box, as
    /// Bukkit's `getNearbyEntities`) of `center`, in the NPC's level, the NPC itself included.
    fn nearby(&self, center: Vec3, range: f64) -> Vec<EntityView>;
    /// A living entity by UUID in any level (current targets are kept by UUID).
    fn entity(&self, uuid: u128) -> Option<EntityView>;
    /// Whether nothing solid is between the two points (`hasLineOfSight`, with
    /// `enhance los traces`: glass and other see-through blocks don't block).
    fn line_of_sight(&self, from: Vec3, to: Vec3) -> bool;
    /// The lowest y of the NPC's level.
    fn min_y(&self) -> f64;
    /// A random number in 0..1.
    fn random(&mut self) -> f64;

    /// Sets the body's maximum health (and cuts its health to it).
    fn set_max_health(&mut self, max: f64);
    fn set_health(&mut self, health: f64);
    /// Hurts the body (generic damage), as `damage(1)` does below the world.
    fn hurt_self(&mut self, amount: f64);
    fn teleport_self(&mut self, to: Vec3);
    /// Zeroes the body's motion (`allow knockback` off).
    fn stop_motion(&mut self);
    /// Walks towards a point (`navigator.setTarget(location)`), stopping within `margin`.
    fn navigate_to(&mut self, to: Vec3, speed: f64, margin: f64);
    /// Follows an entity (`navigator.setTarget(entity, false)`).
    fn navigate_to_entity(&mut self, target: &EntityView, speed: f64);
    fn cancel_navigation(&mut self);
    fn is_navigating(&self) -> bool;
    /// Turns the head and body to look at a point.
    fn face(&mut self, at: Vec3);
    /// Swings the main hand (arm swing animation).
    fn swing(&mut self);
    /// Raises or lowers a shield.
    fn set_blocking(&mut self, blocking: bool);
    /// The vanilla attack of the body with what it holds (`LivingEntity.attack`): damage,
    /// enchantments, knockback and the attack sound as a player's or zombie's hit would do.
    /// False when the host can't do it for this body; then Sentinel deals its own damage.
    fn native_attack(&mut self, target: &EntityView) -> bool;
    /// Hurts `target` by `amount` as an attack of this NPC (`entity.damage(amount, npc)`).
    fn hurt(&mut self, target: &EntityView, amount: f64);
    /// Adds to `target`'s velocity (Sentinel's own knockback).
    fn push(&mut self, target: &EntityView, velocity: Vec3);
    /// Launches a projectile from `from` with `velocity`, owned by the NPC; `item` is the item it
    /// carries (the potion, the tipped arrow), when it has one.
    fn launch(&mut self, kind: Projectile, from: Vec3, velocity: Vec3, item: Option<&str>);
    /// A lightning effect without damage at `at`.
    fn lightning_effect(&mut self, at: Vec3);
    /// Gives `target` the glowing effect for `ticks`.
    fn make_glow(&mut self, target: &EntityView, ticks: i32);
    fn play_sound(&mut self, sound: &str, at: Vec3);
    /// Sends a chat line to one player.
    fn tell(&mut self, player: u128, text: &str);
    /// Takes one item from the main hand stack (`needs ammo`); false when nothing was there.
    fn take_held_item(&mut self) -> bool;
    /// Swaps the held item to the first inventory item that satisfies `pick` (autoswitch);
    /// true when one was found.
    fn swap_held(&mut self, pick: &dyn Fn(&str) -> bool) -> bool;

    /// The NPC's waypoints (`Waypoints` trait) for "nearest path point"; empty when none.
    fn waypoints(&self) -> Vec<Vec3>;
    /// Pauses or resumes the NPC's waypoint walking.
    fn pause_waypoints(&mut self, paused: bool);
}
