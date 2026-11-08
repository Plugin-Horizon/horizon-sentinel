# Gaps: what Horizon (a589a3dd9) lacks for Sentinel

Each line: the hook or helper this port needs, the feature it serves, and what the port does
today. Signatures are proposals in the style of `crates/horizon-client/src/game/native_*.rs`.

## Combat

- [x] (`hurt_entity` †; host uses it for `damage`, `workaround damage` and below-world damage) `Game::native_hurt_entity(&mut self, dimension: &str, target: u32, amount: f32, damage_type: &str, attacker: Option<u32>) -> bool` (vanilla `LivingEntity.hurtServer(level, damageSources().mobAttack(attacker), amount)` on any living entity, players included) — needed for Sentinel's own damage (`damage` set, `workaround damage`, lightning weapon, below-world self damage) — today: `native_run_command("damage <uuid> <amount> minecraft:mob_attack by <npc uuid>")`.
- [x] (`npc_mob_attack` †; no `LanHost.mob_players` binding any more) `Game::native_mob_attack(&mut self, dimension: &str, mob: u32, target: u32) -> bool` (vanilla `Mob.doHurtTarget(level, target)` against any living entity, a given player included) — needed for native attacks (`do native attacks`, the default on our fighters) — today: a `Goal` added to the body calls `mobs::ai::do_hurt_target(m, who, cx)`; for a player target the mob is bound to that guest by writing `LanHost.mob_players` (game/lan_mobs.rs `player_for` keeps a bound guest only while `m.ai.target == Some(Who::Player)`), which is fragile.
- [ ] `native_living_incoming_damage` with a host: a `&mut self` variant firing everywhere v1 fires (players, projectiles, mob attacks, environment), like `living_incoming_damage_v2` but not limited to player-to-mob melee — needed for protection, armor, fightback and safe shot decisions that need the current entities — today: decisions use entity views cached at the last level tick (at most one tick old).
- [x] (`living_incoming_damage_v3` †: `attacker`, `direct`; the 500 ms blow matching, `attack` and `projectile` injections are gone) The causing entity in `native_living_incoming_damage` for a player's melee or arrow on a mob and for a mob's melee on a player (both 0 today, see docs/native-mods.md API 50) — needed for fightback, `protected`, safe shot and stats — today: the attacker is noted from `native_attack_entity`, `native_projectile_impact` and Sentinel's own goal, and matched to the next hit on that victim within 500 ms.
- [ ] (partial: `native_living_damage_pre` † sets the health loss to Sentinel's amount after vanilla's armor, so a worn armor no longer reduces it twice; absorption and Resistance are still vanilla's) `native_living_incoming_damage` modifier breakdown (`EntityDamageEvent.DamageModifier`: base, armor, blocking, resistance, magic, absorption) — needed for Sentinel's armor rule, which zeroes vanilla's modifiers and applies its own share — today: Sentinel's share is applied to the incoming amount and vanilla armor still applies after it (NPCs with worn armor take less than on Paper).
- [ ] `Game::native_living_use_item(&mut self, dimension: &str, entity: u32, hand: Hand, using: bool) -> bool` (`LivingEntity.startUsingItem` / `stopUsingItem` for mobs and mannequins) — needed for raising a shield (`tryDefendFrom`) — today: the block only counts in the damage rule; no pose is shown.
- [ ] `Game::native_swing(&mut self, dimension: &str, entity: u32, hand: Hand)` (`ClientboundAnimatePacket`) — needed for the arm swing of Sentinel's own damage and ranged shots — today: only native attacks swing (through `do_hurt_target`).
- [ ] (partial: `native_entity_push` † moves mobs and NPC bodies, not players: it returns false for a player) `Game::native_push_living(&mut self, dimension: &str, entity: u32, velocity: [f64; 3]) -> bool` for players by entity id (`native_push_player` works on the acting player only) — needed for Sentinel's own knockback on players (`workaround damage`) — today: players get no knockback on that path.

## Movement

- [x] (`npc_mob_navigation` †: the host calls them each tick; no goal of ours any more, `no_ai` is cleared) `Game::native_mob_navigate(&mut self, dimension: &str, mob: u32, to: [f64; 3], speed: f64, margin: f64) -> bool`, `native_mob_follow(&mut self, dimension: &str, mob: u32, target: u32, speed: f64) -> bool`, `native_mob_stop(&mut self, dimension: &str, mob: u32)`, `native_mob_navigating(&self, dimension: &str, mob: u32) -> bool` (Citizens' `Navigator`: `setTarget(location)`, `setTarget(entity)`, `cancelNavigation`, `isNavigating`, `distanceMargin`) — needed for chasing, guarding, returning to waypoints and running away — today: a `Goal` (MOVE|LOOK, priority -10) reads intents the update writes (modules/sentinel_lib.rs); no stop margin.
- [x] (`native_mob_look_at` †) `Game::native_mob_look_at(&mut self, dimension: &str, mob: u32, at: [f64; 3])` (`npc.faceLocation`) — needed for aiming and facing the target — today: through the goal (`Mob::look_at`).
- [ ] Goals on mannequins: confirm `GoalSelector` ticks for `minecraft:mannequin` (vanilla's `Mannequin` is no `Mob` and has no goals) — needed for player-shaped fighters (the common Citizens NPC type) — today: assumed; if not, the NPC library must use a body whose AI runs, or the helpers above must work for non-`Mob` livings.
- [ ] `Game::native_set_mob_max_health(&mut self, dimension: &str, mob: u32, max: f32) -> bool` (attribute `minecraft:max_health` base) — needed for `health` — today: `Mob.attrs.max_health` is written directly (not an attribute modifier; may be reset by an attribute refresh).
- [ ] (partial: `enhance los traces` walks a ray over `native_collision_boxes` and lets blocks named glass, leaves or iron bars pass; there is no block-opacity query, so other see-through blocks block) `Game::native_line_of_sight(&self, dimension: &str, from: [f64; 3], to: [f64; 3], see_through: bool) -> bool` (`LivingEntity.hasLineOfSight`, plus Sentinel's `checkLineOfSightWithTransparency` that lets glass, leaves and other see-through blocks pass) — needed for `canSee` with `enhance los traces` — today: `Game::clip_ray`, blocks with a collision shape block sight (glass blocks it).

## Reading entities

- [x] (`NativeServerPlayer.eye_height`, `.sneaking`; the views use `eye_height`) `NativeServerPlayer.eye_height: f32` (or `sneaking: bool`) — needed for eye-to-eye range and aiming at players — today: 1.62 above the feet.
- [ ] `Game::native_living_effects(&self, entity: EntityRef) -> Vec<(String, i32, i32)>` (id, amplifier, ticks) — needed for invisibility (`ignore invisible targets`) and the `health:`/status targets — today: `Mob.invisible` (the flag, not the effect); players are never seen as invisible.
- [x] (`Host::component(stack, "minecraft:lore")`, read in `item_view`) `ItemStack::lore_plain(&self) -> Vec<String>` — needed for `lore:` item targets — today: lore never matches.
- [x] (`native_player_stacks(uuid)`, filled into `EntityView.inventory`) A player's storage slots by UUID (`Game::native_player_stacks` reads the acting player) — needed for `in_inventory:` targets — today: never matches for players.

## Deaths and drops

- [ ] `native_living_experience_drop(&mut self, entity: u32, kind: &str, xp: &mut i32)` (NeoForge `LivingExperienceDropEvent`) — needed for `death xp` and "no xp from enemies" — today: not possible; vanilla xp drops.
- [ ] `native_death_message(&mut self, entity: u32, kind: &str, message: &mut Option<Text>)` (Bukkit `PlayerDeathEvent.setDeathMessage`) — needed for `death messages: false` and the NPC name in "killed by" lines — today: not possible.
- [x] (`item_from_nbt`: `DropItem` reads the stack's `meta` name, lore and enchantments, `drop_stack` builds the stack) `ItemStack` from a Bukkit-serialized stack, or `ItemStack::from_nbt(&Compound)` — needed for `drops` with names and enchantments — today: material and amount only.
- [x] (`native_remove_entity` takes projectiles †; the host removes a Sentinel's arrows after `arrow cleanup time` ticks) `Game::native_remove_projectile(&mut self, dimension: &str, id: u32) -> bool` (`native_remove_entity` takes displays, markers and interactions only) — needed for `arrow cleanup time` — today: not started.

## Other events

- [ ] `native_entity_combust(&mut self, entity: u32, kind: &str, cause: &str /* "sun", "block", "entity" */, ticks: &mut i32, canceled: &mut bool)` (Bukkit `EntityCombustEvent`) — needed for `block sunburn` (zombie and skeleton Sentinels in daylight) — today: not possible.
- [ ] `native_explosion_detonate` with the exploding entity's owner (the shooter of a wither skull or fireball) — needed for `prevent explosion block damage` — today: not started (check what the existing collect hook gives).
- [ ] A visual-only lightning bolt in `native_summon` (`LightningBolt.setVisualOnly`, Bukkit `strikeLightningEffect`) — needed for the lightning (`nether_star`) weapon — today: a real bolt is summoned (it sets fire).

## Not Horizon's: horizon-citizens (the NPC library)

`bridge/` implements `NpcBridge` over `mod_citizens::api` (horizon-citizens 0.1.0, 2026-11-06)
and keeps the trait in its `saves.yml` (`api::trait_data` / `set_trait_data`). What its API lacks:

- [ ] `pub use core::{npc, yaml};` in `mod_citizens` — needed to convert trait data and locations — today: `bridge/` names `citizens::yaml::Node` and `citizens::npc::Location` and will not compile without it.
- [ ] `api::owner(id) -> Option<u128>` — needed for `OWNER` targets and command ownership — today: read from `trait_data(id, "owner")["uuid"]`.
- [ ] `api::waypoints(id) -> Vec<[f64; 3]>` and `api::pause_waypoints(id, paused: bool)` — needed for "return to the nearest path point", chase range around waypoints, and stopping the walk during a fight — today: no waypoints (chase range is measured from the NPC itself).
- [ ] `api::inventory(id)` / `set_held(id, slot)` (Citizens' `Inventory` trait) — needed for `autoswitch` and ammo refills — today: not started.
- [ ] A way to keep Citizens from moving or turning a Sentinel body while it fights (its own `navigate_to`, look-close) — today: both may act on the same mob; Sentinel's goal runs at priority -10 and its intents win the tick they are given.
- [ ] Non-player NPC bodies are spawned with `NoAI:1b` — Sentinel clears `Mob.no_ai` on its bodies (modules/sentinel_lib.rs `ensure_goal`); horizon-citizens should know a body may have AI.
- [ ] Order of `server_stopping`: Sentinel writes its trait data into Citizens' saves in its own `server_stopping`, which runs after Citizens' (dependency order), so the last changes may miss the final `saves.yml` write — today: changes are also written every 100 ticks.

## Commands (checked against Sentinel's source, MIT, 2026-11-08)

- [x] Message wording: every feedback line in `core/src/commands.rs` is now the original's text.
- [ ] Citizens' own command-framework texts ("Too few arguments.", `Usage: /sentinel <usage>`, "You don't have permission to execute that command.", the `info` `Paginator` layout and "Page not found.") live in Citizens' API, not in Sentinel's repository: they are written from memory of Citizens and unverified.
- [ ] `forgive [id/name]`: the original takes a player name or a 36-character UUID (also for NPC bodies); the port takes the same, but only players are looked up by name, and an NPC body's entity UUID is not mapped back to an NPC. Needs nothing from Horizon, only the bridge's entity lookup.
- [ ] `/sentinel drops` menu: the hooks exist (`native_open_menu`, `menu_clicked`, `menu_closed`); not built.
- [ ] Material names: `weapondamage` / `weaponredirect` check the spelling only (`Material.valueOf`), and the tab completion after `helditem:`, `offhand:`, `equipped:`, `in_inventory:` offers `lore:` / `name:` only. Needs a list of item ids from Horizon (none found in `horizon-items`).
- [ ] Target prefixes that need data the entity view lacks: `permission:` (a player's permission check), `potion:` (active effects, see "Reading entities"), `sbteam:` / `sbscoreabove:` / `sbscorebelow:` (scoreboard), `npc_owned_by:` (the NPC's owner as a player, matched against a nested label). They are accepted and saved, and never match.
- [x] Permission nodes: `permission_check` is a real fragment now (`"hooks": @API:permission_check@`); the host asks `native_has_permission`, no bundle key is declared here.
- [ ] `native_tab_complete_v2` injection (`helpers/tab_complete.rs`): the usages declare the target argument as `word?`; whether Horizon sends a client-side completion request for a `word?` that follows a literal chain is unverified.
