//! Sentinel for Horizon: combat NPCs on top of an NPC library (Citizens or its Horizon port).
//!
//! The logic of the Bukkit plugin Sentinel (by mcmonkey, MIT; see LICENSE), translated to Rust
//! with the game behind two interfaces: [`world::World`] (one NPC's body and its surroundings)
//! and [`npc::NpcBridge`] (the NPC library). The native mod's helpers implement both over
//! Horizon's `Game` and call into the process-wide [`registry::Registry`] from the hooks.
//! Nothing here depends on Horizon, so it builds and tests alone.

pub mod brain;
pub mod combat;
pub mod commands;
pub mod config;
pub mod data;
pub mod list;
pub mod node;
pub mod npc;
pub mod registry;
pub mod target;
pub mod view;
pub mod world;

use std::sync::{Mutex, MutexGuard, OnceLock};

pub use config::{Config, DEFAULT_TOML};
pub use registry::Registry;

/// The mod's id (config file `config/sentinel-common.toml`, log prefix).
pub const MOD_ID: &str = "sentinel";

fn global() -> &'static Mutex<Registry> {
    static R: OnceLock<Mutex<Registry>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(Registry::default()))
}

/// The server's Sentinels. Hooks run on the server thread one at a time, so the lock is never
/// contended; it only makes the static sound.
pub fn registry() -> MutexGuard<'static, Registry> {
    global().lock().unwrap_or_else(|e| e.into_inner())
}

/// Reads `sentinel-common.toml` (on load and on every change); a broken file keeps the old one.
pub fn load_config(text: &str) -> Result<(), String> {
    let cfg = Config::from_toml(text)?;
    registry().cfg = cfg;
    Ok(())
}
