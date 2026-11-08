//! Sentinel's trait-mod half: the [`NpcBridge`] over horizon-citizens' API, installed for the
//! mixin half (`horizon_client::sentinel`), and the trait's load and save through Citizens'
//! `saves.yml` (`npc.<id>.traits.sentinel`).
//!
//! Written blind against horizon-citizens 0.1.0 as it stood on 2026-11-06 (its `api` module) and
//! Horizon a589a3dd9; nothing here has been compiled.

use citizens::api;
use horizon_client::mod_api::{Hooks, Host, ServerStoppingEvent, TickEvent};
use horizon_client::sentinel::core::{
    self,
    data::Location,
    node::Node,
    npc::{NpcBridge, NpcId, TRAIT_NAME},
    view::Vec3,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

pub fn hooks() -> Box<dyn Hooks> {
    Box::new(Sentinel { state: Mutex::new(State::default()) })
}

struct Sentinel {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    installed: bool,
    ticks: u64,
    /// What was last written to Citizens for each NPC, to write only changes.
    written: BTreeMap<NpcId, Node>,
}

/// NPCs whose settings changed (`/sentinel ...`), written at the next save pass.
static DIRTY: Mutex<BTreeSet<NpcId>> = Mutex::new(BTreeSet::new());

// ----- the two YAML trees -----
//
// GAP in horizon-citizens: `mod_citizens` re-exports `api`, `DespawnReason` and `NpcEvent` only;
// these use `citizens::yaml` and `citizens::npc::Location`, which it would have to re-export
// (`pub use core::{npc, yaml};`).

/// Citizens' node -> Sentinel's.
fn from_citizens(n: &citizens::yaml::Node) -> Node {
    use citizens::yaml::Node as C;
    match n {
        C::Scalar(s) => {
            // Review S3: a plain `null` / `~` is no value, not the text "null".
            if s.raw.is_empty() || s.is_null() {
                Node::Null
            } else if s.raw.starts_with('\'') || s.raw.starts_with('"') {
                Node::Str(s.text())
            } else {
                Node::Scalar(s.text())
            }
        }
        C::Seq(items) => Node::List(items.iter().map(from_citizens).collect()),
        C::Map(m) => Node::Map(m.keys().filter_map(|k| Some((k.to_owned(), from_citizens(m.get(k)?)))).collect()),
    }
}

/// Sentinel's node -> Citizens'.
fn to_citizens(n: &Node) -> citizens::yaml::Node {
    use citizens::yaml::{Node as C, Scalar};
    match n {
        Node::Null => C::Scalar(Scalar::null()),
        // A plain scalar keeps its text (numbers and bools stay unquoted).
        Node::Scalar(s) => C::Scalar(Scalar { raw: s.clone() }),
        // A string that reads like a number gets quotes, as SnakeYAML writes a `String`.
        Node::Str(s) if s.parse::<f64>().is_ok() || matches!(s.as_str(), "true" | "false") => {
            C::Scalar(Scalar { raw: format!("'{s}'") })
        }
        Node::Str(s) => C::str(s),
        Node::List(items) => C::Seq(items.iter().map(to_citizens).collect()),
        Node::Map(entries) => {
            let mut m = C::map();
            for (k, v) in entries {
                if let Some(map) = m.as_map_mut() {
                    map.set(k, to_citizens(v));
                }
            }
            m
        }
    }
}

fn to_location(l: &Location) -> citizens::npc::Location {
    citizens::npc::Location { world: l.world.clone(), x: l.x, y: l.y, z: l.z, yaw: l.yaw, pitch: l.pitch, body_yaw: l.yaw }
}

fn from_location(l: &citizens::npc::Location) -> Location {
    Location { world: l.world.clone(), world_id: None, x: l.x, y: l.y, z: l.z, yaw: l.yaw, pitch: l.pitch }
}

// ----- the bridge -----

struct CitizensBridge;

impl NpcBridge for CitizensBridge {
    fn npc_of_entity(&self, entity: i32) -> Option<NpcId> {
        api::by_entity(u32::try_from(entity).ok()?)
    }
    fn entity_of(&self, npc: NpcId) -> Option<i32> {
        api::entity_of(npc).and_then(|e| i32::try_from(e).ok())
    }
    fn dimension_of(&self, npc: NpcId) -> Option<String> {
        api::npc(npc)?.dimension
    }
    fn name(&self, npc: NpcId) -> Option<String> {
        api::npc(npc).map(|n| n.name)
    }
    fn owner(&self, npc: NpcId) -> Option<u128> {
        // The `owner` trait (`traits.owner.uuid`); GAP in horizon-citizens' api: no `owner(id)`.
        let data = from_citizens(&api::trait_data(npc, "owner")?);
        core::list::parse_uuid(data.str("uuid")?)
    }
    fn is_spawned(&self, npc: NpcId) -> bool {
        api::entity_of(npc).is_some()
    }
    fn spawn(&mut self, npc: NpcId, at: Option<&Location>) -> bool {
        if let Some(at) = at {
            api::teleport(npc, to_location(at));
        }
        api::spawn(npc)
    }
    fn destroy(&mut self, npc: NpcId) {
        api::remove(npc);
    }
    fn set_spawned_flag(&mut self, npc: NpcId, spawned: bool) {
        if spawned {
            api::spawn(npc);
        } else {
            // Sets `spawned: false` in the saves (the body is gone already after a death).
            api::despawn(npc);
        }
    }
    fn set_protected(&mut self, npc: NpcId, protected: bool) {
        api::set_protected(npc, protected);
    }
    fn set_stored_location(&mut self, npc: NpcId, at: &Location) {
        // On a despawned NPC this moves its saved location only.
        api::teleport(npc, to_location(at));
    }
    fn stored_location(&self, npc: NpcId) -> Option<Location> {
        api::npc(npc)?.location.as_ref().map(from_location)
    }
    fn selected(&self, player: u128) -> Option<NpcId> {
        api::selected(player)
    }
    fn waypoints(&self, _npc: NpcId) -> Vec<Vec3> {
        // GAP in horizon-citizens' api: the waypoints of an NPC (`Npc::waypoints` is internal).
        Vec::new()
    }
    fn pause_waypoints(&mut self, _npc: NpcId, _paused: bool) {
        // GAP in horizon-citizens' api: pausing its waypoint walking during a fight.
    }
    fn mark_dirty(&mut self, npc: NpcId) {
        DIRTY.lock().unwrap_or_else(|e| e.into_inner()).insert(npc);
    }
}

impl Sentinel {
    /// Once the NPC registry is up: installs the bridge and listens to NPC events.
    fn install(&self, st: &mut State) {
        if st.installed || !registry_ready() {
            return;
        }
        st.installed = true;
        horizon_client::sentinel::install_bridge(Box::new(CitizensBridge));
        api::subscribe(|event| {
            use citizens::NpcEvent;
            match event {
                NpcEvent::Despawned { npc, .. } => core::registry().despawned(*npc),
                NpcEvent::Removed { npc } => {
                    core::registry().sentinels.remove(npc);
                }
                _ => {}
            }
        });
    }

    /// Loads traits Citizens has that Sentinel doesn't, attaches new ones, drops removed ones.
    fn sync(&self, st: &mut State) {
        let with: BTreeSet<NpcId> = api::with_trait(TRAIT_NAME).into_iter().collect();
        let mut reg = core::registry();
        for &npc in &with {
            if reg.is_sentinel(npc) {
                continue;
            }
            match api::trait_data(npc, TRAIT_NAME).map(|n| from_citizens(&n)) {
                Some(node) if !node.is_empty() => {
                    reg.load(npc, &node);
                    st.written.insert(npc, node);
                }
                // `/trait sentinel` on an NPC: a new trait with the config's defaults.
                _ => {
                    reg.attach(npc);
                    DIRTY.lock().unwrap_or_else(|e| e.into_inner()).insert(npc);
                }
            }
        }
        let gone: Vec<NpcId> = reg.sentinels.keys().copied().filter(|n| !with.contains(n)).collect();
        for npc in gone {
            reg.sentinels.remove(&npc);
            st.written.remove(&npc);
            // `/trait remove sentinel` leaves the NPC protected, as Sentinel's `onRemove`.
            api::set_protected(npc, true);
        }
    }

    /// Writes changed traits into Citizens' saves (stats change every tick, so every few seconds).
    fn save(&self, st: &mut State, all: bool) {
        let dirty: BTreeSet<NpcId> = std::mem::take(&mut *DIRTY.lock().unwrap_or_else(|e| e.into_inner()));
        let reg = core::registry();
        for (&npc, _) in reg.sentinels.iter() {
            if !all && !dirty.contains(&npc) && st.ticks % 100 != 0 {
                continue;
            }
            let Some(node) = reg.save(npc) else { continue };
            if st.written.get(&npc) == Some(&node) {
                continue;
            }
            if api::set_trait_data(npc, TRAIT_NAME, to_citizens(&node)) {
                st.written.insert(npc, node);
            }
        }
    }
}

/// Whether the NPC registry answers (it does once the world is open, even with no NPCs).
fn registry_ready() -> bool {
    api::with_world(|_| ()).is_some()
}

impl Hooks for Sentinel {
    fn tick(&self, _host: &mut dyn Host, _event: &mut TickEvent) {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        self.install(&mut st);
        if !st.installed {
            return;
        }
        st.ticks += 1;
        if st.ticks % 5 == 1 {
            self.sync(&mut st);
        }
        self.save(&mut st, false);
    }

    fn server_stopping(&self, _host: &mut dyn Host, _event: &mut ServerStoppingEvent) {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if st.installed {
            // Before Citizens writes saves.yml (Sentinel depends on Citizens, so its hook runs
            // after Citizens' own: GAP, the order may need Citizens to save on its next pass).
            self.save(&mut st, true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nodes_survive_both_ways() {
        let text = "respawnTime: 0\nallIgnores:\n  byNpcName:\n  - '3'\n  targets:\n  - OWNER\n";
        let node = core::node::parse_yaml(text).unwrap();
        assert_eq!(from_citizens(&to_citizens(&node)), node);
    }
}
