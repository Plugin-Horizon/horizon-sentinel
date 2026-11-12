# horizon-sentinel

> **Unofficial Rust port of Sentinel for the Horizon server (https://github.com/arismoko/horizon). Not affiliated with or endorsed by the Sentinel authors.**

Combat NPCs for [Horizon](https://github.com/arismoko/horizon) servers, as a native mod: a
Rust rebuild of the Bukkit plugin [Sentinel](https://github.com/mcmonkeyprojects/Sentinel) on top
of a Horizon NPC library. **Not affiliated with Sentinel, its author, or Citizens.**

Derived from Sentinel (https://github.com/mcmonkeyprojects/Sentinel, MIT; see [NOTICE](NOTICE)).

> **Built for and working only with [arismoko/horizon](https://github.com/arismoko/horizon)** (the Horizon fork that carries the mod API). [horizon-rust/horizon](https://github.com/horizon-rust/horizon) is experimental for mods and not supported: the earlier version for its native hooks / horizon-mixins is frozen on the `horizon-plain` branch (work stopped on 2026-11-12).

## Which Horizon

This is a native trait mod: `horizon-mod.toml`, a `Cargo.toml` that depends on `horizon-mod-api`, `src/lib.rs` with the hooks, and the plugin logic in the `core/` crate (plain Rust, no engine needed for its tests).

- Pin: `[native] horizon = "54090ee36c4e2c7b43b66db864220c577b94b7e0"` in `horizon-mod.toml` (game `26.3`). That is a development build of arismoko/horizon `main` plus mod API additions that are not all merged upstream yet, so a stock `main` may lack some host methods this mod calls until they land.
- It compiled and loaded on that build and was smoke-tested there; it has not seen a long run with real players. Treat it as experimental.
- Feature descriptions below were written for the earlier horizon-rust version; where they name engine calls (`native_*`) or old file paths, read them as descriptions only. The Known gaps list is current.

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

## Config

`config/sentinel-common.toml`, written with defaults when missing and read again within a second
of a save. It holds Sentinel's `config.yml` sections (`[defaults]` = `sentinel defaults`,
`[random]`, `update-rate`) with dashes for spaces. `Config::from_sentinel_yaml` reads an existing
`config.yml`.

## Build

Build it as a native trait mod with arismoko's launcher (see the "native mods" how-to in the arismoko/horizon docs):

```sh
horizon-launcher native-build <path to this folder> --server-output <dir> --version <tag> --trust-native-code
```

`--trust-native-code` is your consent to compile and run this code inside the server. The logic crate tests run without the engine: `(cd core && cargo test)`.

## Known gaps

What does not work yet on arismoko/horizon (each is marked `TODO(arismoko-api: ...)` in `src/lib.rs`):

- Custom names, lore and enchantment levels are not read, so weapon rules do not see them.
- Mannequin NPC bodies cannot path-find, follow or turn to their target: they walk to the target's position each tick.
- Shield raising, the vanilla attack of the body and visual-only lightning are not available.
- An NPC's own inventory is not exposed by Citizens yet, so auto-switching finds nothing; potions' contents are not copied.
- Experience drops, the hidden death message and custom drop stacks are not applied.
- Needs the arismoko `citizens` mod (see `horizon-citizens`).

## Licence

MIT, see [LICENSE](LICENSE). Sentinel is MIT-licensed (Copyright (c) 2019-2022 Alex "mcmonkey"
Goodwin); its source was read and parts of it are translated here, so its notice is kept in
LICENSE. Citizens' source was not used.
