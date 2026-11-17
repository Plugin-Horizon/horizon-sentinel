# hmod port: sentinel
Round 1
- Fixed: src/lib.rs build_views: `entity.kind` is an `EntityTypeKey`; mapped to its id string (`k.id(host)`) so `kind` stays a `String`.
- Fixed: src/lib.rs `Body::push`: a player is pushed with `EntityRef::push` (`entity_push`) and the same delta as other entities (`velocity - target.velocity`). `set_velocity` does not exist on `PlayerRef`, and Horizon refuses velocity writes to players.
- Fixed: src/lib.rs `Look::grants`: string `permission_value` replaced by typed Boolean `permission_bool`. Answers for the sender are read in `sentinel_command` (`sentinel_grants`) before the command runs, because the query needs `&mut` host. Any error is a denial.
- Fixed: src/lib.rs `Mod::permission_gather_nodes` (new): declares every node from `sentinel_node_names()`.
- Fixed: src/sentinel_core/commands.rs: `sub_node` made `pub` so the mod can list the nodes.
- Permission nodes: `sentinel.basic`, `npc.select`, `citizens.admin`, `sentinel.<sub>` for each subcommand (`info`, `greet`, `chase`, `drops` and the rest) -> `Level(2)`, declared in `Mod::permission_gather_nodes` (src/lib.rs).
- Gaps: none
- Unsure:
  - `citizens` imports (src/lib.rs lines 10-12 and in `install`) are reported unresolved by the check workspace. The launcher adds the `citizens` alias from horizon-mod.toml, so Cargo.toml was left unchanged as the docs say. Not verified.
  - Player push: a delta push (`entity_push`) replaces the absolute velocity set. The result differs by the error between the view's velocity snapshot and the player's real velocity.
  - `npc.select` and `citizens.admin` may also be declared by the citizens mod. A duplicate declaration is ignored here (`let _ =`); the default level 2 is assumed to match.
  - `EntityTypeKey::id(&*host)` is taken from `entity_capability.rs` in the vocab crate; not compiled.
