// Sentinel's part of horizon-client-lib (`horizon_client::sentinel`): the core crate re-exported
// for the game's helpers and for the NPC library, the slot the NPC library fills with its
// bridge, and the entity views the damage and knockback hooks decide with.
//
// The decisions are made in the server's level tick (helpers/level_tick.rs), every
// `update rate` ticks, and carried out through Horizon's own helpers (`native_mob_navigate`,
// `native_mob_follow`, `native_mob_attack`, `native_hurt_entity`...), which need no goal of ours.
//
// UNVERIFIED (blind port): written against horizon b13dbf06 without a build.

pub use sentinel_core as core;

use crate::mobs::Mob;
use std::collections::HashMap;
use std::sync::Mutex;

/// A Sentinel body runs the engine's navigation: horizon-citizens spawns non-player NPCs with
/// `NoAI`, and `native_mob_navigate` keeps a path but does not walk it for such a mob.
pub fn allow_ai(m: &mut Mob) {
    m.no_ai = false;
    // A Sentinel picks its own targets.
    m.ai.targets.clear();
}

/// Forgets a body that left the level.
pub fn forget(_id: u32) {}

/// The entity views of the last level tick, by entity id. The damage, knockback and chat hooks
/// get no host, so they decide with these (at most one tick old).
static VIEWS: Mutex<Option<HashMap<i32, core::view::EntityView>>> = Mutex::new(None);

/// Replaces the cached views of `dimension` (and of every player) with `views`.
pub fn cache_views(dimension: &str, views: &[core::view::EntityView]) {
    let mut g = VIEWS.lock().unwrap_or_else(|e| e.into_inner());
    let map = g.get_or_insert_with(HashMap::new);
    map.retain(|_, v| v.dimension != dimension && v.kind != "minecraft:player");
    for v in views {
        map.insert(v.id, v.clone());
    }
}

pub fn view(id: i32) -> Option<core::view::EntityView> {
    VIEWS.lock().unwrap_or_else(|e| e.into_inner()).as_ref()?.get(&id).cloned()
}

pub fn view_by_uuid(uuid: u128) -> Option<core::view::EntityView> {
    let g = VIEWS.lock().unwrap_or_else(|e| e.into_inner());
    g.as_ref()?.values().find(|v| v.uuid == uuid).cloned()
}

/// What a Sentinel victim's hit comes to by Sentinel's armor rule, for `living_damage_pre`:
/// victim -> amount, with the time it was set (a hit that never reaches the health leaves it).
static FINAL_DAMAGE: Mutex<Vec<(i32, f32, std::time::Instant)>> = Mutex::new(Vec::new());

pub fn set_final_damage(victim: i32, amount: f32) {
    let mut f = FINAL_DAMAGE.lock().unwrap_or_else(|e| e.into_inner());
    let now = std::time::Instant::now();
    f.retain(|(v, _, at)| *v != victim && now.duration_since(*at).as_millis() < 500);
    f.push((victim, amount, now));
}

/// The amount set for `victim` in the last half second, used up.
pub fn take_final_damage(victim: i32) -> Option<f32> {
    let mut f = FINAL_DAMAGE.lock().unwrap_or_else(|e| e.into_inner());
    let i = f.iter().position(|(v, _, at)| *v == victim && at.elapsed().as_millis() < 500)?;
    Some(f.remove(i).1)
}

/// The NPC library's bridge (Citizens' registry), set once by that library at start. Without it
/// Sentinel has no NPCs and does nothing.
pub type Bridge = Box<dyn core::npc::NpcBridge + Send>;
static BRIDGE: Mutex<Option<Bridge>> = Mutex::new(None);

pub fn install_bridge(bridge: Bridge) {
    *BRIDGE.lock().unwrap_or_else(|e| e.into_inner()) = Some(bridge);
}

/// Runs `f` with the bridge, if the NPC library installed one.
pub fn with_bridge<R>(f: impl FnOnce(&mut dyn core::npc::NpcBridge) -> R) -> Option<R> {
    let mut g = BRIDGE.lock().unwrap_or_else(|e| e.into_inner());
    g.as_mut().map(|b| f(b.as_mut()))
}
