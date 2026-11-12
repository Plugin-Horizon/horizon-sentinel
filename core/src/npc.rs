//! The minimal NPC interface Sentinel needs from an NPC library (Citizens, or its Horizon port).
//!
//! Sentinel is a Citizens trait: Citizens owns the NPCs (ids, names, owners, bodies, saving), and
//! Sentinel adds behaviour to those that carry its trait. On Horizon the NPC library implements
//! [`NpcBridge`] and calls the [`crate::registry::Registry`] at the moments Citizens calls a
//! trait (`load`, `save`, `onAttach`, `onSpawn`, `onDespawn`, `onRemove`). See GAPS.md for what
//! is assumed of the library.

use crate::data::Location;
use crate::view::Vec3;

/// A Citizens NPC id (`npc.getId()`).
pub type NpcId = i32;

/// What Sentinel asks of the NPC library.
pub trait NpcBridge {
    /// The NPC whose body is the entity `entity` (`CitizensAPI.getNPCRegistry().getNPC`).
    fn npc_of_entity(&self, entity: i32) -> Option<NpcId>;
    /// The entity id of a spawned NPC's body.
    fn entity_of(&self, npc: NpcId) -> Option<i32>;
    /// The dimension a spawned NPC's body is in.
    fn dimension_of(&self, npc: NpcId) -> Option<String>;
    /// The NPC's full name.
    fn name(&self, npc: NpcId) -> Option<String>;
    /// The `Owner` trait's UUID, if a player owns it.
    fn owner(&self, npc: NpcId) -> Option<u128>;
    fn is_spawned(&self, npc: NpcId) -> bool;
    /// Spawns the NPC at `at` (its stored location when None); false when it could not.
    fn spawn(&mut self, npc: NpcId, at: Option<&Location>) -> bool;
    /// `npc.destroy()`: removes the NPC for good (`respawn time` -1).
    fn destroy(&mut self, npc: NpcId);
    /// `Spawned.setSpawned(false)`: stays despawned after a restart (`respawn time` 0).
    fn set_spawned_flag(&mut self, npc: NpcId, spawned: bool);
    /// `npc.setProtected`: Citizens cancels all damage to a protected NPC.
    fn set_protected(&mut self, npc: NpcId, protected: bool);
    /// `CurrentLocation.setLocation` / `getStoredLocation`.
    fn set_stored_location(&mut self, npc: NpcId, at: &Location);
    fn stored_location(&self, npc: NpcId) -> Option<Location>;
    /// The NPC a player has selected (`/npc select`), for `/sentinel` commands.
    fn selected(&self, player: u128) -> Option<NpcId>;
    /// Positions of the NPC's waypoints (linear or wander centres).
    fn waypoints(&self, npc: NpcId) -> Vec<Vec3>;
    fn pause_waypoints(&mut self, npc: NpcId, paused: bool);
    /// Marks the NPC's saves dirty, so the library writes them.
    fn mark_dirty(&mut self, npc: NpcId);
}

/// Sentinel's trait name in Citizens' `traitnames` and `traits.<name>`.
pub const TRAIT_NAME: &str = "sentinel";
