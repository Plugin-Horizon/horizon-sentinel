# hmod port: sentinel
Parity: 85% now / 85% with our PRs / 86% with engine changes / 87% with new SDK (213/250 items now; 217/250 with new SDK) (before round 4: 84%, 210/250; method: commands, nodes, config keys, messages, integration targets, listed features; items blocked by missing engine API count as not done; the engine levels follow audit/FINISH_QUEUE.md, HOOK_BACKLOG.md and the merged list in HANDOVER_MAIN.md); blind, uncompiled.
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

Round 4 (blind, 2026-11-20): three items closed, all committed in this repo.
- Fixed: a dead Sentinel's own drops are placed where its body fell (`living_drops` builds each drop with `item_key` + `ItemStack::of` + `set_count`; picked up after 10 ticks). Before, the vanilla list stayed. Commit 55e3798.
- Fixed: the console acts on its own Citizens selection (`bridge.selected(u128::MAX)`, Citizens' console key), so a bare `/sentinel` from the console works after `/npc select`. Commit 769a88c.
- Fixed: `/sentinel drops` opens the 4-row menu (`open_menu`, Sentinel's `9 * 4` chest) with the NPC's drops. A server tick reads the open menu and writes a changed stack list back (plain Bukkit-style `type`/`amount`; an unchanged entry keeps its saved fields); `menu_closed` drops the menu from the list. The close event carries no contents, so the last tick's read is what is kept. Core: `Effect::OpenDrops`, test `drops_opens_the_menu_for_players_only`. Commit 1f5185a.
- Gaps (no API, or branch only): see Checklist round 100.
- Unsure (not compiled):
  - `ItemPlace::MenuSlot { menu, slot }` assumed to take a `usize` slot and `MenuRef` to be `Copy` (invsee uses both shapes).
  - `Text::literal(String)` for the menu title (the original is green `Sentinel <id>`; the port's title is plain).
  - `bridge.selected(u128::MAX)` relies on Citizens' console key being `u128::MAX` (read from the citizens core `skey`, not from a documented API).
  - A menu change made and closed inside one tick is lost (the last read is one tick old). Only a player who closes within 50 ms of the last click can hit this.
  - A changed or moved entry loses its lore, name and enchantments; unknown item ids keep no slot and are dropped on the next save.
  - Drop velocity is `[0,0,0]` and pickup delay 10 ticks; Bukkit's `dropItemNaturally` adds a random velocity.

Checklist round 100 (what the audit and this round checked; `[ ]` = open, with the reason)
- [x] 56 subcommands, usages, argument counts, nodes, messages (audit R1-R3)
- [x] tab completion: trailing empty word, subcommand names, material/item names, potion and owner prefixes (R2, R3)
- [x] `sentinel.basic` on the command, operator level 1 default, command blocks and functions may run it (R2)
- [x] console selection (R4), drops menu (R4), a Sentinel's own drops on death (R4)
- [x] `potion:` and `npc_owned_by:` targets (R3)
- [ ] `permission:` target and `sentinel.*` parent: needs #18822 (node query for an undeclared node) and #18818 (final `*` in node names); branch only, see GAPS_UPSTREAM.md
- [ ] `group:` target: no group list in the reference (GAPS_UPSTREAM.md, `permission_groups`)
- [ ] `sbteam:`, `sbscoreabove:`, `sbscorebelow:` targets: no scoreboard read API (GAPS_UPSTREAM.md)
- [ ] weapon rules see custom names and lore: branch #18812 (`ItemStack` name/lore read); enchantment levels: no API
- [ ] drops keep names, lore and enchantments when written: `ItemStack::set_display` (#17896) merged upstream, not in the pin
- [ ] `wgregion`: cross-mod WorldGuard region limit (GAPS_UPSTREAM.md)
- [ ] mannequin path-finding, follow, look-at; shield raising; vanilla body attack; visual lightning; experience drop; hidden death message: no API (GAPS_UPSTREAM.md)
- [ ] NPC's own inventory (auto-switch): Citizens does not expose it
- [ ] Sentinel's events and integration registration for other plugins: no equivalent (audit/sentinel.md)
- [ ] `/sentinel help` paging and Citizens' header: Citizens' CommandManager output; not ported, Citizens source is clean-room only
- [ ] not re-walked this round: SentinelTrait listeners (greet on teleport/move) and SentinelEventHandler's other listeners (damage-item cancel); the audit's R1 listing stands for them
