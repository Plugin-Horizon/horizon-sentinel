# hmod port: sentinel
Parity: before ~80% (~200/~250) -> after ~84% (~210/~250) (method: commands, nodes, config keys, messages, integration targets, listed features; items blocked by missing engine API count as not done); blind, uncompiled.
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

Round 2 (parity audit fixes)
- Fixed: tab completion keeps the empty word being typed (`/sentinel addtarget <Tab>` completes at once) and completes the subcommand names the sender has the node for (`commands::tab_args`, `subcommand_names`, src/lib.rs `tab_complete`).
- Fixed: the command is registered with the `sentinel.basic` node (hidden from and refused to those without it); node default is operator level 1 (any operator, as Bukkit's `default: op`), was 2.
- Fixed: command blocks and functions may run `/sentinel` (as the console; they need `--id`).
- Gaps (found): drops menu, `group:`/`permission:`/`potion:`/score/team/owner targets, `sentinel.*` parent node, console selection, `wgregion`, material-name completion.

Round 3 (blind): `potion:<effect>` targets (active effects read per entity, Bukkit's old effect names mapped) and `npc_owned_by:<label>` targets (the owner's online view is attached to each NPC view) now match; the item prefixes (`helditem:`, `offhand:`, `equipped:`, `in_inventory:`) complete every item name from `item_keys()`. Open and why: `permission:` / `group:` (no node query for undeclared nodes, no group list), `sbteam:` / `sbscoreabove:` / `sbscorebelow:` (no scoreboard read API), drops menu (no way to read a closed menu's stacks and write Bukkit-format stack YAML), `sentinel.*` parent (no wildcard nodes), console selection (needs citizens console select), `wgregion` (cross-mod region lookup). All in audit/GAPS_UPSTREAM.md.
