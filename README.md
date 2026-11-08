# horizon-sentinel

> **Unofficial Rust port of Sentinel for the Horizon server (https://github.com/horizon-rust/horizon). Not affiliated with or endorsed by the Sentinel authors.**

Combat NPCs for [Horizon](https://github.com/horizon-rust/horizon) servers, as a native mod: a
Rust rebuild of the Bukkit plugin [Sentinel](https://github.com/mcmonkeyprojects/Sentinel) on top
of a Horizon NPC library. **Not affiliated with Sentinel, its author, or Citizens.**

Derived from Sentinel (https://github.com/mcmonkeyprojects/Sentinel, MIT; see [NOTICE](NOTICE)).

**State: written blind.** The logic (`core/`) built and its tests passed before the commands, target
prefixes and tab completion were reworked from Sentinel's source (not compiled since); the game glue has not been
applied or compiled yet. See [STATUS.md](STATUS.md) for every feature and [GAPS.md](GAPS.md) for
what Horizon still lacks.

## What it does

An NPC with the `sentinel` trait guards, hunts and fights:

- **Targets, ignores, avoids**: entity types (`players`, `npcs`, `monsters`, `zombies` ...), the
  owner, names by pattern (`player:`, `npc:`, `entityname:`), held or worn items, events (`pvp`,
  `pve`, `message:<text>` ...), groups and combinations (`multi:`, `allinone:`). Patterns match
  anywhere in the name, case-insensitive: `npc:3` ignores every NPC whose name holds a `3`.
- **Fighting**: chases within `chase range`, hits in `reach` every `attack rate` with the vanilla
  attack of its weapon (or a fixed `damage`), shoots bows, tridents, snowballs, potions and more.
- **Health**: `health`, `heal rate`, `armor`, `invincible`, `protected` (no damage from ignored
  entities), knockback on/off, fire at most once a second.
- **Death**: its own `drops`, then respawns after `respawn time` (0 = stays dead, -1 = deleted),
  at its `spawnpoint` if set.
- **Guarding and running away**: follows a player or NPC; runs from avoid targets to a return point.
- **Greetings and warnings** for players who come near.
- **`/sentinel <subcommand>`** for the NPC selected with the NPC library (or `--id <npc>`):
  `addtarget`, `addignore`, `respawntime`, `health`, `info`, `stats` and the rest of the original's.

Saved data stays where Citizens keeps it (`npc.<id>.traits.sentinel` in `saves.yml`), with the
same keys, so a server can move between Paper and Horizon with its NPCs.

## Needs

horizon-citizens (mod id `citizens`, a required dependency): Sentinel's
`bridge/` package implements `sentinel_core::npc::NpcBridge` over its `api` and keeps the trait in
its `saves.yml`. Another NPC library can install its own bridge with
`horizon_client::sentinel::install_bridge`. What horizon-citizens' API still lacks is at the end of
[GAPS.md](GAPS.md).

## Config

`config/sentinel-common.toml`, written with defaults when missing and read again within a second
of a save. It holds Sentinel's `config.yml` sections (`[defaults]` = `sentinel defaults`,
`[random]`, `update-rate`) with dashes for spaces. `Config::from_sentinel_yaml` reads an existing
`config.yml`.

## Layout

| Path | What |
|---|---|
| `core/` | The logic, no game dependency: `list` (targets), `data` (saved trait), `combat`, `brain` (one NPC's update), `registry` (all NPCs, hits, deaths), `commands`, `config`, `node` (Citizens' YAML tree) |
| `modules/sentinel_lib.rs` | `horizon_client::sentinel`: core re-export, bridge slot, view cache, the goal that moves and swings a body |
| `modules/sentinel_host.rs` | `crate::game::sentinel_host`: the core's `World` over Horizon's `Game` |
| `bridge/` | The trait-mod half (`[api]`): the bridge to horizon-citizens, trait load and save |
| `helpers/` | One HEAD injection each: level and server tick, damage, knockback, death, loot, commands, chat, config |
| `horizon-mixin.toml` | The bundle, pinned to Horizon a589a3dd9 |

## Build

The core alone: `cargo test` in `core/` (crates: `regex`, `toml`, both in Horizon's `Cargo.lock`).

The mod: with the Horizon launcher (`horizon-launcher native-build <this folder> --server-output DIR`)
or `horizon-mixins apply` on an exported Horizon source, then build the server. Untested.

## Licence

MIT, see [LICENSE](LICENSE). Sentinel is MIT-licensed (Copyright (c) 2019-2022 Alex "mcmonkey"
Goodwin); its source was read and parts of it are translated here, so its notice is kept in
LICENSE. Citizens' source was not used.

## Contributing

Pull requests and improvements are always welcome! Open an issue or a PR -- direct pushes to main are disabled, every change goes through review.
