# Status (2026-11-08, blind: no engine build, no server)

Uses pending API fragments by their `target` (each needs `"hooks": @API:<fragment>@` once stamped): `living_incoming_damage_v3`, `living_damage` (`native_living_damage_pre`), `hurt_entity`, `npc_mob_attack`, `npc_mob_navigation`, `entity_moves`, `player_bots` (`native_collision_boxes`), `remove_projectiles`, `player_status`.

`core/` builds and its unit tests pass (`cargo test` in `core/`). The bundle (`horizon-mixin.toml`,
`modules/`, `helpers/`) has never been applied or compiled; it is written against the Horizon
source at a589a3dd9.

Legend: **written** = logic done (core tested; game glue unbuilt), **partly** = some of it, or a
workaround (GAPS.md), **not started**.

## Data and config

| Feature | State |
|---|---|
| Trait data in Citizens' `saves.yml` (`traits.sentinel`), same keys, read and written (incl. 1.x `targets`/`ignores` layout, unknown keys kept) | written |
| Target lists `allTargets` / `allIgnores` / `allAvoids` (`targets`, `byNpcName`, ... `byMultiple`) | written |
| Config: `sentinel defaults`, `random`, `update rate` as `config/sentinel-common.toml`; reads Sentinel's `config.yml` too | written |
| Defaults applied to a new trait, owner ignored on attach | written |
| Bridge to horizon-citizens (NPC lookup, spawn/despawn, protected, selection; trait load/save every 100 ticks and on stop) | partly (needs re-exports and owner/waypoint calls from horizon-citizens, see GAPS) |

## Targeting

| Feature | State |
|---|---|
| Target types (`PLAYER`, `NPC`, `OWNER`, mob types, `MONSTER`, `MOB`, `PASSIVE_MOB`, plurals) | written |
| `npc:`, `player:`, `entityname:` regex targets (`.*x.*`, case-insensitive) | written |
| `helditem:`, `offhand:`, `equipped:` (material, `name:`) | written |
| `lore:`, `in_inventory:` | written (`minecraft:lore` component, `native_player_stacks`) |
| `group:` | partly (needs a permissions bridge to fill `groups`) |
| `status:angry/passive` | written |
| `event:pvp/pve/eve/pvnpc/pvsentinel/npcvnpc/guarded_fight/message:/pv:/ev:` | written (checked at the next update) |
| `multi:` and `allinone:` | written |
| `squad:` / `uuid:` / `health:` integration targets | written |
| Other integrations (towny, factions, war, sbteam, crackshot, simplepets ...) | not started (never match) |
| Creative/spectator players, invisible, armor stands, guarded entity never targeted | partly (invisibility: GAPS) |
| Current targets with `enemy target time`, `reaction slowdown`, forget on death/leave/range | written |
| Line of sight, `range`, `realistic` facing | partly (`enhance los traces`: glass, leaves and iron bars pass by name; other see-through blocks block) |

## Fighting

| Feature | State |
|---|---|
| Melee with native attack (`do native attacks`, damage -1) | written (`native_mob_attack`) |
| Melee with Sentinel damage (`damage`, `weapon_damage_map`, sharpness) | written (`native_hurt_entity` with the body as attacker) |
| `attack rate`, `reach`, quick melee target | written |
| Chasing (`chaseclose`, `chaseranged`, `chase range`, clever ticks, return to waypoints) | written (`native_mob_navigate` with `reach` as the margin, `native_mob_follow`; waypoints come from horizon-citizens) |
| Bow/crossbow arrows, snowballs, eggs, tridents, pearls, potions, llama spit, shulker bullets, wither skulls, fireballs | partly (summoned with motion and owner; ballistic aim without drag) |
| Evoker fangs (book), lightning (nether star), spectral glow | partly |
| `needs ammo` | partly (takes from the held stack; no refill from inventory) |
| `autoswitch` | not started (needs the NPC inventory) |
| `safe shot` | written |
| `accuracy` | written |
| Shield blocking | partly (damage rule only, no pose) |
| `weapon_redirect_map` | written |

## Taking damage, health, death

| Feature | State |
|---|---|
| `health` (max health), `heal rate` | partly (max health written to the field) |
| `armor` (setting or worn) | partly (`living_damage_pre` sets Sentinel's amount after vanilla's armor; absorption and Resistance stay vanilla's) |
| `invincible` | written (cancel + `set_protected` on the bridge) |
| `protected` (from ignores), `protect from range` | written |
| `fightback`, `runaway`, guard fightback, `no guard damage` | written |
| Fire taken once a second | written |
| `allow knockback` | written |
| Below-world self damage | written (`native_hurt_entity`) |
| `respawn time` (>0 timer, 0 stay dead + `Spawned` false, -1 destroy), `spawnpoint` | written |
| `drops` / `dropchance` | written (material, amount, custom name, lore, enchantments) |
| `enemy drops` off | written (drops cleared through the loot hook) |
| `death xp`, `death messages` | not started (GAPS: no hooks) |
| Stats (`stats_*`) | written |

## Other behaviour

| Feature | State |
|---|---|
| Guarding a player or NPC (follow, teleport when far, `guard distance`) | written |
| Avoiding (`addavoid`, `avoid range`, `avoidreturnpoint`, `run away time`) | partly (run direction without A* check) |
| Greeting / warning texts, `greet range`, `greet rate` | written (checked each update instead of on move) |
| Waypoint pausing during combat | written (through the bridge) |
| `arrow cleanup time` | written (`native_remove_entity` takes projectiles) |
| `block sunburn`, explosion block damage | not started (GAPS) |
| Pathfinder auto-correct, WorldGuard region, PlaceholderAPI, bStats | not started (no counterpart) |

## Commands

Read against Sentinel's source (MIT) on 2026-11-08: every subcommand, argument range, permission node and message text.

| Feature | State |
|---|---|
| `/sentinel` and its 59 subcommands as `@Command` declares them (usage, `min`/`max` argument count, "Too few/many arguments." plus `Usage:` line), `help`, "Unknown command. Did you mean:" (Levenshtein, as Citizens) | written |
| Check order of `SentinelCommand.onCommand`: `sentinel.basic` (Bukkit's refusal text), unknown command, `--id` (needs `npc.select`; "ID input is invalid (not an integer)!"), "Must have a Sentinel NPC selected!", "Selected NPC is not a Sentinel! ...", ownership ("You do not own this NPC (and you are not an admin)."; admin is `citizens.admin`), the subcommand's node | written |
| Every message of the original, with Java's number parsing and error text (`For input string: "x"`), Java's double printing, ranges per command (`range` 0 < r < 200, `health` 0.01..2048, `avoidrange` 4..100, `guarddistance` 1..100 / 1..25 ...) | written |
| `info` (all lines in the original's order, paged 8 lines, "Calculated" damage and armor), `stats`, `targets`/`ignores`/`avoids` (every category, all-in-one and multi) | written (page layout is a guess: Citizens' `Paginator` text is not in this source) |
| Permissions as plugin.yml names them (`sentinel.info` also for `targets`/`ignores`/`avoids`, `sentinel.greet`, `sentinel.chase`, `sentinel.drops` for `dropchance`, `sentinel.avoidrange` ...); default operators | written |
| `invincible` sets the NPC protected at once; `kill` on a dead NPC says "NPC is already dead!"; `guard` with an unknown player says "NPC now guarding its area!"; `wgregion` answers "WorldGuard not loaded!" | written |
| `/sentinel drops` (inventory menu) | not started (the menu hooks exist: `native_open_menu`, `menu_clicked`, `menu_closed`) |
| One-time "Notice: ... removeignore owner" when the owner hits a fresh NPC they have selected | written |
| Tab completion: one `NativeCommand` usage per subcommand path with typed arguments; the target argument of add/remove target/ignore/avoid is `word?` and answered by `native_tab_complete_v2` exactly as `SentinelCommand.onTabComplete` (prefixes, every type name and plural, `event:`/`status:` forms, what the NPC's list holds for removal) | written (material names after `helditem:` etc. are missing, see GAPS) |
| Target prefixes: Sentinel's own integrations `healthabove:`, `healthbelow:`, `uuid:`, `squad:` answer; `permission:`, `potion:`, `sbteam:`, `sbscoreabove:`, `sbscorebelow:`, `npc_owned_by:` are valid and saved but never match (GAPS); Towny/Factions/War/SimpleClans/SimplePets/CrackShot prefixes are unknown, as on a server without those plugins | partly |
| Greeting and warning lines: `[name]: text` in aqua and yellow, text stored and sent as typed (no `&` translation) | written |

## Test first, once a build is possible

1. The bundle applies and compiles (module paths, `dependent = "horizon-client-lib"`, private
   field access from `crate::game::sentinel_host`, the `[api]` bridge seeing `horizon_client::sentinel`
   and `citizens::{api, yaml, npc}`).
2. A mannequin NPC body walks (`native_mob_navigate` with `no_ai` cleared) and attacks
   (`native_mob_attack` against a mob, then against a player).
3. Two fighters with `addtarget npc` and `addignore npc:<id>` for portal NPCs: they fight each
   other and leave the portal NPCs alone (core test `fighter_attacks_npcs_but_not_ignored_portal_npcs`
   is the same scenario).
4. `respawntime 0`: a killed fighter stays dead and is `spawned: false` after a restart.
5. A trait written by Paper's Sentinel reads, saves and reads again in Paper unchanged
   (compare `/sentinel info` on both; check whether Citizens leaves default values out of the file,
   which decides whether missing keys mean the Java defaults, as assumed here).
