//! `/sentinel <subcommand>` (`SentinelCommand` and the `Sentinel*Commands` classes): the
//! settings of the selected NPC (`/npc select`, or `--id <npc>`). Times are typed in seconds and
//! kept in ticks, as in Sentinel. Translated from Sentinel (MIT, see LICENSE): the checks run in
//! the order of `SentinelCommand.onCommand` and Citizens' command manager, and every message is
//! the original's text.

use crate::data::{Location, SentinelData, ATTACK_RATE_MAX, HEAL_RATE_MAX};
use crate::list::{parse_uuid, TargetLabel, TargetList};
use crate::node::java_double;
use crate::npc::NpcId;
use crate::registry::Registry;
use crate::view::EntityView;

/// `SentinelCommand.prefixGood` / `prefixBad` / `colorEmphasis` / `colorBasic`.
const GOOD: &str = "\u{a7}2[Sentinel] \u{a7}e";
const BAD: &str = "\u{a7}2[Sentinel] \u{a7}c";
const EMPH: &str = "\u{a7}b";
const BASIC: &str = "\u{a7}e";
const RESET: &str = "\u{a7}r";

/// `SentinelPlugin.maxHealth` (Spigot's `maxHealth`, 2048 unless configured otherwise).
const MAX_HEALTH: f64 = 2048.0;
/// `SentinelTrait.healthMin`.
const HEALTH_MIN: f64 = 0.01;
/// Lines of `/sentinel info` per page (Citizens' `Paginator`).
const INFO_PER_PAGE: usize = 8;

/// Who runs the command and on what.
pub struct Sender {
    /// None for the console.
    pub player: Option<u128>,
    /// The sender's permission level (console = 4).
    pub op_level: u8,
    /// The NPC the sender has selected (`--id` overrides it).
    pub selected: Option<NpcId>,
}

/// Things the command asks the game to do, besides changing the settings.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// Kill the NPC's body (`/sentinel kill`).
    Kill(NpcId),
    /// Spawn the NPC at its spawn point (`/sentinel respawn`).
    Respawn(NpcId),
    /// Read the config file again.
    Reload,
    /// The settings changed: save the NPC.
    Dirty(NpcId),
    /// Set the body's health to its new maximum.
    Health(NpcId, f64),
    /// `npc.setProtected` (`SentinelTrait.setInvincible`).
    Protected(NpcId, bool),
}

/// The result: lines for the sender and effects for the game.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    pub lines: Vec<String>,
    pub effects: Vec<Effect>,
}

impl Outcome {
    fn good(&mut self, s: impl AsRef<str>) {
        self.lines.push(format!("{GOOD}{}", s.as_ref()));
    }
    fn bad(&mut self, s: impl AsRef<str>) {
        self.lines.push(format!("{BAD}{}", s.as_ref()));
    }
    /// A red line without Sentinel's prefix (Citizens' command manager and Bukkit speak this way).
    fn red(&mut self, s: impl AsRef<str>) {
        self.lines.push(format!("\u{a7}c{}", s.as_ref()));
    }
}

/// What the game can tell the command about the world.
pub trait Lookup {
    /// The NPC's body, when spawned.
    fn body(&self, npc: NpcId) -> Option<EntityView>;
    /// An online player by name (case-insensitive): their UUID.
    fn player_uuid(&self, name: &str) -> Option<u128>;
    fn player_name(&self, uuid: u128) -> Option<String>;
    /// The NPC's owner.
    fn owner(&self, npc: NpcId) -> Option<u128>;
    /// The world name (`world`, `world_nether`) of a dimension, as locations save it.
    fn world_name(&self, dimension: &str) -> String;
    /// A permission node's answer for a player (a permissions mod decides; the game falls back
    /// to the operator level against `default_level`). None: no answer, the sender's level decides.
    fn grants(&self, _player: u128, _node: &str, _default_level: u8) -> Option<bool> {
        None
    }
    /// `NPC.getFullName()`: None when the library does not know the NPC.
    fn npc_name(&self, _npc: NpcId) -> Option<String> {
        None
    }
    /// `CitizensAPI.getNPCRegistry().getById(id) != null`.
    fn npc_exists(&self, _npc: NpcId) -> bool {
        true
    }
    /// `Material.valueOf(name.toUpperCase())` succeeds. The default only checks the spelling
    /// (letters, digits, `_`): a namespaced id such as `minecraft:stick` is no Bukkit material name.
    fn is_material(&self, name: &str) -> bool {
        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    }
}

/// Whether the sender may run a command that a permission node decides. The console always may.
fn allowed(sender: &Sender, look: &dyn Lookup, node: &str, default_level: u8) -> bool {
    match sender.player {
        None => true,
        Some(p) => look.grants(p, node, default_level).unwrap_or(sender.op_level >= default_level),
    }
}

/// Every permission node defaults to operators: `sentinel.*` has `default: op` in the real
/// plugin.yml and its children are granted through it (`npc.select` and `citizens.admin` are
/// Citizens' and default to operators too).
const NODE_LEVEL: u8 = 2;

/// One subcommand as its `@Command` annotation declares it. `min` and `max` count the arguments
/// including the subcommand word itself (`argsLength()`).
pub struct Sub {
    pub name: &'static str,
    pub usage: &'static str,
    pub desc: &'static str,
    pub min: usize,
    pub max: usize,
    /// `@Requirements(traits = SentinelTrait.class, ownership = true)`.
    pub sentinel: bool,
}

const fn sub(name: &'static str, usage: &'static str, desc: &'static str, min: usize, max: usize) -> Sub {
    Sub { name, usage, desc, min, max, sentinel: true }
}

/// The subcommands, in the order the original registers its command classes
/// (attack, chase, greeting, health, info, intelligence, target), for the command tree, `help`
/// and the "did you mean" suggestion.
pub const SUBCOMMANDS: &[Sub] = &[
    sub("enemydrops", "enemydrops ['true'/'false']", "Toggles whether enemy mobs of this NPC drop items.", 1, 2),
    sub("accuracy", "accuracy OFFSET", "Sets the accuracy of an NPC (0 means perfectly accurate, 5 means pretty inaccurate).", 1, 2),
    sub("reach", "reach REACH", "Sets the NPC's reach (how far it can punch).", 1, 2),
    sub("projectilerange", "projectilerange RANGE", "Sets the NPC's projectile range (how far it is willing to shoot projectiles).", 1, 2),
    sub("attackrate", "attackrate RATE ['ranged']", "Changes the rate at which the NPC attacks, in seconds - either ranged or close modes.", 1, 3),
    sub("range", "range RANGE", "Sets the NPC's maximum attack range.", 1, 2),
    sub("damage", "damage DAMAGE", "Sets the NPC's attack damage.", 1, 2),
    sub("weapondamage", "weapondamage MATERIAL DAMAGE", "Sets the NPC's attack damage for a specific weapon material.", 2, 3),
    sub("weaponredirect", "weaponredirect MATERIAL_ONE MATERIAL_TWO", "Sets the NPC to treat material one as though it's material two.", 2, 3),
    sub("safeshot", "safeshot ['true'/'false']", "Toggles whether the NPC will avoid damaging non-targets.", 1, 2),
    sub("speed", "speed SPEED", "Sets the NPC's movement speed modifier.", 1, 2),
    sub("chaserange", "chaserange RANGE", "Changes the maximum distance an NPC will run before returning to base.", 1, 2),
    sub("wgregion", "wgregion REGION_NAME", "Limits the NPC to only chase inside of a WorldGuard region.", 1, 2),
    sub("chaseclose", "chaseclose ['true'/'false']", "Toggles whether the NPC will chase while in 'close quarters' fights.", 1, 2),
    sub("chaseranged", "chaseranged ['true'/'false']", "Toggles whether the NPC will chase while in ranged fights.", 1, 2),
    sub("greetrange", "greetrange GREETRANGE", "Sets how far a player can be from an NPC before they are greeted.", 1, 2),
    sub("greetrate", "greetrate GREETRATE", "Sets how quickly (in seconds) the Sentinel may re-greet any player.", 1, 2),
    sub("greeting", "greeting GREETING", "Sets a greeting message for the NPC to say.", 1, 9999),
    sub("warning", "warning WARNING", "Sets a warning message for the NPC to say.", 1, 9999),
    sub("armor", "armor ARMOR", "Sets the NPC's armor level.", 1, 2),
    sub("health", "health HEALTH", "Sets the NPC's health level.", 1, 2),
    sub("healrate", "healrate RATE", "Changes the rate at which the NPC heals, in seconds.", 1, 2),
    sub("respawntime", "respawntime TIME", "Changes the time it takes for the NPC to respawn, in seconds.", 1, 2),
    sub("invincible", "invincible ['true'/'false']", "Toggles whether the NPC is invincible.", 1, 2),
    sub("knockback", "knockback ['true'/'false']", "Toggles whether the NPC can receive knockback.", 1, 2),
    sub("kill", "kill", "Kills the NPC.", 1, 1),
    sub("respawn", "respawn", "Respawns the NPC.", 1, 1),
    sub("drops", "drops", "Changes the drops of the current NPC.", 1, 1),
    sub("dropchance", "dropchance ID CHANCE", "Changes the chance of a drop.", 1, 3),
    sub("deathxp", "deathxp XP", "Sets the amount of XP to be dropped when the NPC dies..", 1, 2),
    sub("spawnpoint", "spawnpoint", "Changes the NPC's spawn point to its current location, or removes it if it's already there.", 1, 1),
    sub("info", "info", "Shows info on the current NPC.", 1, 2),
    sub("stats", "stats", "Shows statistics about the current NPC.", 1, 1),
    Sub { name: "debug", usage: "debug", desc: "Toggles debugging.", min: 1, max: 1, sentinel: false },
    Sub { name: "reload", usage: "reload", desc: "Reloads the configuration file.", min: 1, max: 1, sentinel: false },
    sub("squad", "squad SQUAD", "Sets the NPC's squad name (give blank name for none).", 1, 9999),
    sub("guard", "guard [PLAYERNAME]/npc:[ID]", "Makes the NPC guard a specific player or NPC - don't specify a player to stop guarding.", 1, 2),
    sub("needammo", "needammo ['true'/'false']", "Toggles whether the NPC will need ammo.", 1, 2),
    sub("autoswitch", "autoswitch ['true'/'false']", "Toggles whether the NPC automatically switches items.", 1, 2),
    sub("realistic", "realistic ['true'/'false']", "Toggles whether the NPC should use 'realistic' targeting logic (don't attack things you can't see.)", 1, 2),
    sub("guarddistance", "guarddistance MINIMUM_DISTANCE [SELECTION_RANGE]", "Sets the NPC's minimum guard distance (how far you must go before the NPC moves to keep up) and selection range (how close it will try to get to you).", 1, 3),
    sub("addtarget", "addtarget TYPE", "Adds a target.", 2, 2),
    sub("removetarget", "removetarget TYPE", "Removes a target.", 2, 2),
    sub("addignore", "addignore TYPE", "Ignores a target.", 2, 2),
    sub("removeignore", "removeignore TYPE", "Allows targeting a target.", 2, 2),
    sub("addavoid", "addavoid TYPE", "Avoids a target.", 2, 2),
    sub("removeavoid", "removeavoid TYPE", "Stops avoiding a target.", 2, 2),
    sub("avoidrange", "avoidrange RANGE", "Sets the distance to try to keep from threats.", 1, 2),
    sub("targettime", "targettime TIME", "Sets the NPC's enemy target time limit in seconds.", 1, 2),
    sub("runaway", "runaway ['true'/'false']", "Toggles whether the NPC will run away when attacked.", 1, 2),
    sub("protected", "protected ['true'/'false']", "Toggles whether the NPC should be protected from damage by ignore targets.", 1, 2),
    sub("protectfromrange", "protectfromrange RANGE", "Sets the maximum range after which damage should start being ignored. If zero, this does nothing. Set eg 100 to ignore all damage from over 100 blocks away.", 1, 2),
    sub("fightback", "fightback ['true'/'false']", "Toggles whether the NPC will fight back.", 1, 2),
    sub("forgive", "forgive (id)", "Forgives all current targets.", 1, 2),
    sub("targets", "targets", "Shows the targets of the current NPC.", 1, 1),
    sub("ignores", "ignores", "Shows the ignore targets of the current NPC.", 1, 1),
    sub("avoids", "avoids", "Shows the avoid targets of the current NPC.", 1, 1),
    sub("avoidreturnpoint", "avoidreturnpoint", "Changes the location the NPC runs to when avoid mode is activated, or removes it if the NPC is already there.", 1, 1),
];

/// The permission node of a subcommand, as the `permission` of its `@Command` names it.
fn sub_node(sub: &str) -> String {
    match sub {
        "stats" | "targets" | "ignores" | "avoids" => "sentinel.info".into(),
        "greeting" | "warning" | "greetrange" | "greetrate" => "sentinel.greet".into(),
        "chaseclose" | "chaseranged" => "sentinel.chase".into(),
        "dropchance" => "sentinel.drops".into(),
        other => format!("sentinel.{other}"),
    }
}

/// Java's `Double.parseDouble`, with its error text (`For input string: "x"`).
fn jd(s: &str) -> Result<f64, String> {
    let fail = || format!("For input string: \"{s}\"");
    let t = s.trim_matches(|c: char| c <= ' ');
    if t.is_empty() {
        return Err("empty String".into());
    }
    let unsigned = t.strip_prefix(['+', '-']).unwrap_or(t);
    if unsigned == "NaN" {
        return Ok(f64::NAN);
    }
    if unsigned == "Infinity" {
        return Ok(if t.starts_with('-') { f64::NEG_INFINITY } else { f64::INFINITY });
    }
    // A `d` / `f` suffix is allowed after digits.
    let body = match t.strip_suffix(['d', 'D', 'f', 'F']) {
        Some(b) if b.ends_with(|c: char| c.is_ascii_digit() || c == '.') => b,
        _ => t,
    };
    if !body.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | '+' | '-' | 'e' | 'E')) {
        return Err(fail());
    }
    body.parse::<f64>().map_err(|_| fail())
}

/// Java's `Integer.parseInt`, with its error text.
fn ji(s: &str) -> Result<i64, String> {
    s.parse::<i32>().map(i64::from).map_err(|_| format!("For input string: \"{s}\""))
}

/// `args.argsLength() > 1 && "true".equalsIgnoreCase(args.getString(1))`, then the same for false.
fn toggle_mode(arg: Option<&&str>, current: bool) -> bool {
    match arg {
        Some(a) if a.eq_ignore_ascii_case("true") => true,
        Some(a) if a.eq_ignore_ascii_case("false") => false,
        _ => !current,
    }
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i];
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur.push((prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost));
        }
        prev = cur;
    }
    prev[b.len()]
}

/// `CommandManager.getClosestCommandModifier`.
fn closest_subcommand(modifier: &str) -> &'static str {
    let mut best = ("", usize::MAX);
    for s in SUBCOMMANDS {
        let d = levenshtein(modifier, s.name);
        if d < best.1 {
            best = (s.name, d);
        }
    }
    best.0
}

/// Citizens' `CommandContext`: `--name value` and `-x` flags are not arguments. Returns the
/// arguments and the value of `--id`.
fn split_flags<'a>(raw: &[&'a str]) -> (Vec<&'a str>, Option<&'a str>) {
    let mut words = Vec::new();
    let mut id = None;
    let mut i = 0;
    while i < raw.len() {
        let w = raw[i];
        if i > 0 && w.len() > 2 && w.starts_with("--") {
            if let Some(v) = raw.get(i + 1) {
                if w[2..].eq_ignore_ascii_case("id") {
                    id = Some(*v);
                }
                i += 2;
                continue;
            }
        } else if i > 0 && w.len() >= 2 && w.starts_with('-') && w[1..].starts_with(|c: char| c.is_alphabetic()) {
            i += 1;
            continue;
        }
        words.push(w);
        i += 1;
    }
    (words, id)
}

/// Runs `/sentinel <args>` (without the command name).
pub fn run(reg: &mut Registry, sender: &Sender, args: &str, look: &dyn Lookup) -> Outcome {
    let mut out = Outcome::default();
    // Bukkit checks the command's own permission (`sentinel.basic`) before `onCommand`.
    if !allowed(sender, look, "sentinel.basic", NODE_LEVEL) {
        out.red("I'm sorry, but you do not have permission to perform this command. Please contact the server administrators if you believe that this is in error.");
        return out;
    }
    let raw: Vec<&str> = args.split_whitespace().collect();
    let modifier = raw.first().copied().unwrap_or("");
    let name = modifier.to_lowercase();
    let spec = SUBCOMMANDS.iter().find(|s| s.name == name);
    if name.is_empty() || name == "help" {
        help(&mut out);
        return out;
    }
    let Some(spec) = spec else {
        out.bad("Unknown command. Did you mean:");
        out.good(format!(" /sentinel {}", closest_subcommand(&name)));
        return out;
    };
    let (words, id_flag) = split_flags(&raw);
    // `--id <npc>` picks the NPC instead of the selection, for those who may select NPCs.
    let mut npc = sender.selected;
    if let Some(v) = id_flag {
        if allowed(sender, look, "npc.select", NODE_LEVEL) {
            match v.parse::<i32>() {
                Ok(id) => npc = look.npc_exists(id).then_some(id),
                Err(_) => {
                    out.bad("ID input is invalid (not an integer)!");
                    return out;
                }
            }
        }
    }
    let mut npc_id = 0;
    if spec.sentinel {
        let Some(id) = npc else {
            out.bad("Must have a Sentinel NPC selected!");
            return out;
        };
        if !reg.is_sentinel(id) {
            out.bad("Selected NPC is not a Sentinel! Use /trait sentinel to ensure an NPC becomes a Sentinel.");
            return out;
        }
        // Citizens' ownership requirement: the owner, or whoever holds `citizens.admin`.
        let owner = look.owner(id);
        if sender.player.is_some() && owner != sender.player && !allowed(sender, look, "citizens.admin", NODE_LEVEL) {
            out.bad("You do not own this NPC (and you are not an admin).");
            return out;
        }
        npc_id = id;
    }
    // Citizens' command manager: the subcommand's permission, then the argument count.
    if !allowed(sender, look, &sub_node(spec.name), NODE_LEVEL) {
        out.red("You don't have permission to execute that command.");
        return out;
    }
    if words.len() < spec.min || words.len() > spec.max {
        out.red(if words.len() < spec.min { "Too few arguments." } else { "Too many arguments." });
        out.red(format!("Usage: /sentinel {}", spec.usage));
        return out;
    }
    let tick_rate = reg.cfg.update_rate;
    match spec.name {
        "debug" => {
            reg.debug = !reg.debug;
            out.good(format!("Toggled: {}!", reg.debug));
            return out;
        }
        "reload" => {
            out.effects.push(Effect::Reload);
            out.good("Reloaded the config file.");
            return out;
        }
        _ => {}
    }
    let npc = npc_id;
    let body = look.body(npc);
    let owner_text = owner_text(look, npc);
    let full_name = look.npc_name(npc).unwrap_or_else(|| format!("NPC {npc}"));
    let s = reg.get_mut(npc).expect("checked");
    let d = &mut s.data;
    let a = &words[1..];
    let has = !a.is_empty();
    let mut changed = true;
    match spec.name {
        "info" => {
            changed = false;
            info(&mut out, &full_name, d, body.as_ref(), look, npc, a.first().copied());
        }
        "stats" => {
            changed = false;
            let st = &d.stats;
            out.lines.push(format!("{GOOD}{RESET}{full_name}{BASIC}: owned by {RESET}{owner_text}"));
            let minutes = st.ticks_spawned as f64 / (20.0 * 60.0);
            for (k, v) in [
                ("Arrows fired", st.arrows_fired.to_string()),
                ("Potions thrown", st.potions_thrown.to_string()),
                ("Fireballs launched", st.fireballs_fired.to_string()),
                ("Snowballs thrown", st.snowballs_thrown.to_string()),
                ("Eggs thrown", st.eggs_thrown.to_string()),
                ("Pearls used", st.pearls_used.to_string()),
                ("Skulls thrown", st.skulls_thrown.to_string()),
                ("Llama spits spat", st.llama_spit_shot.to_string()),
                ("Shulker bullets shot", st.shulker_bullets_shot.to_string()),
                ("Evoker fangs spawned", st.evoker_fangs_spawned.to_string()),
                ("Punches", st.punches.to_string()),
                ("Times spawned", st.times_spawned.to_string()),
                ("Damage Given", java_double(st.damage_given)),
                ("Damage Taken", java_double(st.damage_taken)),
                ("Minutes spawned", format!("{minutes:.2}")),
            ] {
                out.good(format!("{k}: {EMPH}{v}"));
            }
        }
        "targets" | "ignores" | "avoids" => {
            changed = false;
            let (list, prefix) = match spec.name {
                "targets" => (&d.all_targets, "Targeted"),
                "ignores" => (&d.all_ignores, "Ignored"),
                _ => (&d.all_avoids, "Avoided"),
            };
            out.lines.push(format!("{GOOD}{RESET}{full_name}{BASIC}: owned by {RESET}{owner_text}"));
            list_targets(&mut out, list, prefix);
        }
        "addtarget" | "removetarget" | "addignore" | "removeignore" | "addavoid" | "removeavoid" => {
            let label = TargetLabel::parse(a[0]);
            if !test_label(&mut out, &label) {
                return out;
            }
            let adding = spec.name.starts_with("add");
            let (list, what): (&mut TargetList, &str) = if spec.name.ends_with("target") {
                (&mut d.all_targets, "target")
            } else if spec.name.ends_with("ignore") {
                (&mut d.all_ignores, "ignore")
            } else {
                (&mut d.all_avoids, "avoid")
            };
            let ok = if adding { label.add_to(list) } else { label.remove_from(list) };
            changed = ok;
            match (adding, ok, what) {
                (true, true, "target") => out.good("Tracking new target!"),
                (true, false, "target") => out.bad("Already tracking that target!"),
                (false, true, "target") => out.good("No longer tracking that target!"),
                (false, false, "target") => out.bad("Was already not tracking that target!"),
                (true, true, "ignore") => out.good("Ignoring new target!"),
                (true, false, "ignore") => out.bad("Already ignoring that target!"),
                (false, true, "ignore") => out.good("No longer ignoring that target!"),
                (false, false, "ignore") => out.bad("Was already not ignoring that target!"),
                (true, true, _) => out.good("Avoiding a new target!"),
                (true, false, _) => out.bad("Already avoiding that target!"),
                (false, true, _) => out.good("No longer avoiding that target!"),
                // The original's own wording (it says "tracking" here too).
                (false, false, _) => out.bad("Was already not tracking that target!"),
            }
        }
        "forgive" => {
            changed = false;
            if has {
                let who = a[0];
                let id = if who.len() == 36 && who.contains('-') { parse_uuid(who) } else { look.player_uuid(who) };
                let Some(id) = id else {
                    out.bad("Invalid player target input.");
                    return out;
                };
                let r1 = s.rt.targets.remove(&id).is_some();
                let r2 = s.rt.avoids.remove(&id).is_some();
                let r3 = s.rt.chasing == Some(id);
                if r3 {
                    s.rt.chasing = None;
                }
                if r1 || r2 || r3 {
                    out.good("Specified target forgiven.");
                } else {
                    out.good("Specified entity is already not a current target.");
                }
            } else {
                s.rt.targets.clear();
                s.rt.avoids.clear();
                s.rt.chasing = None;
                out.good("Targets forgiven.");
            }
        }
        "enemydrops" => {
            d.enemy_drops = toggle_mode(a.first(), d.enemy_drops);
            out.good(if d.enemy_drops { "NPC enemy mobs now drop items and XP!" } else { "NPC enemy mobs no longer drop items and XP!" });
        }
        "safeshot" => {
            d.safe_shot = toggle_mode(a.first(), d.safe_shot);
            out.good(if d.safe_shot { "NPC now is a safe shot!" } else { "NPC is no longer a safe shot!" });
        }
        "chaseclose" => {
            d.close_chase = toggle_mode(a.first(), d.close_chase);
            out.good(if d.close_chase { "NPC now will chase while close!" } else { "NPC no longer will chase while close!" });
        }
        "chaseranged" => {
            d.ranged_chase = toggle_mode(a.first(), d.ranged_chase);
            out.good(if d.ranged_chase { "NPC now will chase while ranged!" } else { "NPC no longer will chase while ranged!" });
        }
        "invincible" => {
            d.invincible = toggle_mode(a.first(), d.invincible);
            out.effects.push(Effect::Protected(npc, d.invincible));
            out.good(if d.invincible { "NPC now invincible!" } else { "NPC no longer invincible!" });
        }
        "knockback" => {
            d.allow_knockback = toggle_mode(a.first(), d.allow_knockback);
            out.good(if d.allow_knockback { "NPC can now receive knockback!" } else { "NPC no longer can receive knockback!" });
        }
        "needammo" => {
            d.needs_ammo = toggle_mode(a.first(), d.needs_ammo);
            out.good(if d.needs_ammo { "NPC now needs ammo!" } else { "NPC no longer needs ammo!" });
        }
        "autoswitch" => {
            d.autoswitch = toggle_mode(a.first(), d.autoswitch);
            out.good(if d.autoswitch { "NPC now automatically switches items!" } else { "NPC no longer automatically switches items!" });
        }
        "realistic" => {
            d.realistic = toggle_mode(a.first(), d.realistic);
            out.good(if d.realistic { "NPC now targets realistically!" } else { "NPC no longer targets realistically!" });
        }
        "runaway" => {
            d.runaway = toggle_mode(a.first(), d.runaway);
            out.good(if d.runaway { "NPC now runs away!" } else { "NPC no longer runs away!" });
        }
        "protected" => {
            d.protect_from_ignores = toggle_mode(a.first(), d.protect_from_ignores);
            out.good(if d.protect_from_ignores { "NPC is now protected from ignore targets!" } else { "NPC no longer protected from ignore targets!" });
        }
        "fightback" => {
            d.fightback = toggle_mode(a.first(), d.fightback);
            out.good(if d.fightback { "NPC now fights back!" } else { "NPC no longer fights back!" });
        }
        "accuracy" => {
            changed = number(&mut out, a.first(), &mut d.accuracy, "Current accuracy offset", "Accuracy offset set!", "Invalid accuracy offset number", |v| {
                (v >= 0.0 && v <= 10.0).then_some(()).ok_or("Number out of range (must be >= 0 and <= 10).".into())
            });
        }
        "reach" => {
            changed = number(&mut out, a.first(), &mut d.reach, "Current reach", "Reach set!", "Invalid reach number", |v| {
                (v >= 0.0).then_some(()).ok_or("Number out of range (must be >= 0).".into())
            });
        }
        "projectilerange" => {
            changed = number(&mut out, a.first(), &mut d.projectile_range, "Current projectile range", "Projectile range set!", "Invalid projectile range number", |v| {
                (v >= 0.0).then_some(()).ok_or("Number out of range (must be >= 0).".into())
            });
        }
        "range" => {
            changed = number(&mut out, a.first(), &mut d.range, "Current range", "Range set!", "Invalid range number", |v| {
                (v > 0.0 && v < 200.0).then_some(()).ok_or("Number out of range (must be > 0 and < 200).".into())
            });
        }
        "speed" => {
            changed = number(&mut out, a.first(), &mut d.speed, "Current speed", "Speed set!", "Invalid speed number", |v| {
                (v < 1000.0 && v >= 0.0).then_some(()).ok_or("Number out of range (must be >= 0 and < 1000).".into())
            });
        }
        "chaserange" => {
            changed = number(&mut out, a.first(), &mut d.chase_range, "Current chase range", "Chase range set!", "Invalid range number", |_| Ok(()));
        }
        "greetrange" => {
            changed = number(&mut out, a.first(), &mut d.greet_range, "Current greet range", "Range set!", "Invalid range number", |v| {
                (v < 100.0).then_some(()).ok_or("Number out of range (must be < 100).".into())
            });
        }
        "avoidrange" => {
            changed = number(&mut out, a.first(), &mut d.avoid_range, "Current avoid range", "Avoidance range set!", "Invalid range number", |v| {
                (v >= 4.0 && v < 100.0).then_some(()).ok_or("Number out of range (must be >= 4 and < 100).".into())
            });
        }
        "protectfromrange" => {
            changed = number(&mut out, a.first(), &mut d.protect_from_range, "Current protect-from-range", "Protect-from-range set!", "Invalid range number", |v| {
                (v >= 0.0 && v < 500.0).then_some(()).ok_or("Number out of range (must be >= 0 and < 500).".into())
            });
        }
        "damage" => {
            if !has {
                let calc = calculated_damage(d, body.as_ref());
                out.good(format!("Current damage: {EMPH}{}{BASIC} Calculated: {EMPH}{}", java_double(d.damage), java_double(calc)));
                changed = false;
            } else {
                changed = number(&mut out, a.first(), &mut d.damage, "", "Damage set!", "Invalid damage number", |v| {
                    (v < MAX_HEALTH).then_some(()).ok_or(format!("Number out of range (must be < {}).", java_double(MAX_HEALTH)))
                });
            }
        }
        "armor" => {
            if !has {
                let calc = if body.is_some() {
                    let worn = body.as_ref().map(|b| b.armor.clone()).unwrap_or_default();
                    format!("{BASIC} Calculated: {EMPH}{}", java_double(crate::combat::armor(d, &worn)))
                } else {
                    String::new()
                };
                out.good(format!("Current armor: {EMPH}{}{calc}", java_double(d.armor)));
                changed = false;
            } else {
                match jd(a[0]) {
                    Ok(v) if v <= 1.0 => {
                        d.armor = v;
                        let info = if v == 0.0 {
                            "no armor".to_owned()
                        } else if v < 0.0 {
                            "automatic armor calculation".to_owned()
                        } else if v == 1.0 {
                            "100%, invincible armor".to_owned()
                        } else {
                            format!("{}%", java_double(v * 100.0))
                        };
                        out.good(format!("Armor set to {EMPH}{}{BASIC}! ({info})", java_double(v)));
                    }
                    Ok(_) => {
                        changed = false;
                        out.bad("Invalid armor number: Number out of range (must be <= 1).");
                    }
                    Err(e) => {
                        changed = false;
                        out.bad(format!("Invalid armor number: {e}"));
                    }
                }
            }
        }
        "health" => {
            if !has {
                out.good(format!("Current max health: {EMPH}{}", java_double(d.health)));
                changed = false;
            } else {
                match jd(a[0]) {
                    Ok(v) if v >= HEALTH_MIN && v <= MAX_HEALTH => {
                        d.health = v;
                        out.effects.push(Effect::Health(npc, v));
                        out.good("Health set!");
                    }
                    Ok(_) => {
                        changed = false;
                        out.bad(format!(
                            "Invalid health number: Number out of range (must be >= {} and <= {}).",
                            java_double(HEALTH_MIN),
                            java_double(MAX_HEALTH)
                        ));
                    }
                    Err(e) => {
                        changed = false;
                        out.bad(format!("Invalid health number: {e}"));
                    }
                }
            }
        }
        "healrate" => {
            if !has {
                out.good(format!("Current heal rate: {EMPH}{}", java_double(d.heal_rate as f64 / 20.0)));
                changed = false;
            } else {
                match jd(a[0]) {
                    Ok(secs) => {
                        let t = (secs * 20.0) as i32 as i64;
                        if (t >= tick_rate && t <= HEAL_RATE_MAX) || t == 0 {
                            d.heal_rate = t;
                            out.good("Heal rate set!");
                        } else {
                            changed = false;
                            out.bad(format!(
                                "Invalid rate number: Number out of range (must be >= {} and <= {}, or 0).",
                                java_double(tick_rate as f64 / 20.0),
                                java_double(HEAL_RATE_MAX as f64 / 20.0)
                            ));
                        }
                    }
                    Err(e) => {
                        changed = false;
                        out.bad(format!("Invalid rate number: {e}"));
                    }
                }
            }
        }
        "respawntime" => {
            if !has {
                out.good(format!("Current respawn time: {EMPH}{}", java_double(d.respawn_time as f64 / 20.0)));
                changed = false;
            } else {
                match jd(a[0]) {
                    Ok(secs) => {
                        d.respawn_time = (secs * 20.0) as i64;
                        out.good("Respawn time set!");
                    }
                    Err(e) => {
                        changed = false;
                        out.bad(format!("Invalid time number: {e}"));
                    }
                }
            }
        }
        "targettime" => {
            if !has {
                out.good(format!("Current target time: {EMPH}{}", java_double(d.enemy_target_time as f64 / 20.0)));
                changed = false;
            } else {
                match jd(a[0]) {
                    Ok(secs) if secs >= 0.0 => {
                        d.enemy_target_time = (secs * 20.0) as i64;
                        out.good("Target time set!");
                    }
                    Ok(_) => {
                        changed = false;
                        out.bad("Invalid time number: Number out of range (must be >= 0).");
                    }
                    Err(e) => {
                        changed = false;
                        out.bad(format!("Invalid time number: {e}"));
                    }
                }
            }
        }
        "greetrate" => {
            if !has {
                out.good(format!("Current rate: {EMPH}{}", java_double(d.greet_rate as f64 / 20.0)));
                changed = false;
            } else {
                match jd(a[0]) {
                    Ok(secs) if secs >= 0.0 => {
                        d.greet_rate = (secs * 20.0) as i32 as i64;
                        out.good("Rate set!");
                    }
                    Ok(_) => {
                        changed = false;
                        out.bad("Invalid rate number: Number out of range (must be >= 0).");
                    }
                    Err(e) => {
                        changed = false;
                        out.bad(format!("Invalid rate number: {e}"));
                    }
                }
            }
        }
        "attackrate" => {
            if !has {
                out.good(format!("Current attack rate (close): {EMPH}{}", java_double(d.attack_rate as f64 / 20.0)));
                out.good(format!("Current attack rate (ranged): {EMPH}{}", java_double(d.attack_rate_ranged as f64 / 20.0)));
                changed = false;
            } else {
                match jd(a[0]) {
                    Ok(secs) => {
                        let t = (secs * 20.0) as i32 as i64;
                        if t >= tick_rate && t <= ATTACK_RATE_MAX {
                            if a.get(1).is_some_and(|r| r.to_lowercase().contains("ranged")) {
                                d.attack_rate_ranged = t;
                                out.good("Ranged attack rate set!");
                            } else {
                                d.attack_rate = t;
                                out.good("Attack rate set!");
                            }
                        } else {
                            changed = false;
                            out.bad(format!(
                                "Invalid rate number: Number out of range (must be >= {} and <= {}).",
                                java_double(tick_rate as f64 / 20.0),
                                java_double(ATTACK_RATE_MAX as f64 / 20.0)
                            ));
                        }
                    }
                    Err(e) => {
                        changed = false;
                        out.bad(format!("Invalid rate number: {e}"));
                    }
                }
            }
        }
        "deathxp" => {
            if !has {
                out.good(format!("Current death XP: {EMPH}{}", d.death_xp));
                changed = false;
            } else {
                match ji(a[0]) {
                    Ok(xp) if xp >= 0 => {
                        d.death_xp = xp;
                        out.good("Death XP set!");
                    }
                    Ok(_) => {
                        changed = false;
                        out.bad("Invalid XP number: Number out of range (must be >= 0).");
                    }
                    Err(e) => {
                        changed = false;
                        out.bad(format!("Invalid XP number: {e}"));
                    }
                }
            }
        }
        "squad" => {
            let name = a.join(" ").to_lowercase();
            d.squad = if name == "null" || name.is_empty() { None } else { Some(name) };
            out.good("Set!");
        }
        "greeting" => {
            d.greeting_text = a.join(" ");
            out.good("Set!");
        }
        "warning" => {
            d.warning_text = a.join(" ");
            out.good("Set!");
        }
        "guard" => {
            if has {
                let arg = a[0];
                if arg.to_lowercase().starts_with("npc:") {
                    match ji(&arg[4..]) {
                        Ok(id) => {
                            if !i32::try_from(id).is_ok_and(|id| look.npc_exists(id)) {
                                changed = false;
                                out.bad("NPC ID input invalid (that number isn't any NPC's ID)!");
                                return out;
                            }
                            d.set_guarding(None);
                            d.guarded_npc = id as i32;
                            out.good("NPC now guarding the specified NPC!");
                        }
                        Err(_) => {
                            out.bad("NPC ID input invalid (need a number)!");
                            return out;
                        }
                    }
                    out.effects.push(Effect::Dirty(npc));
                    return out;
                }
                d.guarded_npc = -1;
                d.set_guarding(look.player_uuid(arg));
            } else {
                d.guarded_npc = -1;
                d.set_guarding(None);
            }
            if d.guarding().is_none() {
                out.good("NPC now guarding its area!");
            } else {
                out.good("NPC now guarding that player!");
            }
        }
        "guarddistance" => {
            if !has {
                out.good(format!("Current guard minimum distance: {EMPH}{}", java_double(d.guard_distance_minimum)));
                out.good(format!("Current guard selection range: {EMPH}{}", java_double(d.guard_selection_range)));
                changed = false;
            } else {
                let res = (|| -> Result<(f64, f64), String> {
                    let dist = jd(a[0])?;
                    let mut sel = dist * 0.5;
                    if let Some(s) = a.get(1) {
                        sel = jd(s)?;
                        if sel < 1.0 || sel > 25.0 {
                            return Err("Selection_Range number out of range (must be >= 1 and <= 25).".into());
                        }
                    }
                    if dist >= 1.0 && dist <= 100.0 {
                        Ok((dist, sel))
                    } else {
                        Err("Minimum_Distance number out of range (must be >= 1 and <= 100).".into())
                    }
                })();
                match res {
                    Ok((dist, sel)) => {
                        d.guard_distance_minimum = dist;
                        d.guard_selection_range = sel;
                        out.good("guard distance set!");
                    }
                    Err(e) => {
                        changed = false;
                        out.bad(format!("Invalid distance number: {e}"));
                    }
                }
            }
        }
        "weapondamage" => {
            let weapon = a[0].to_lowercase();
            if !look.is_material(&weapon) {
                out.bad("Invalid weapon material (name misspelled?)");
                return out;
            }
            if a.len() < 2 {
                changed = false;
                let cur = d.weapon_damage.get(&weapon).map_or("Unset".to_owned(), |v| java_double(*v));
                out.good(format!("Current weapon damage for '{weapon}': {EMPH}{cur}"));
            } else {
                match jd(a[1]) {
                    Ok(v) if v < MAX_HEALTH => {
                        if v < 0.0 {
                            d.weapon_damage.remove(&weapon);
                            out.good("Weapon damage removed!");
                        } else {
                            d.weapon_damage.insert(weapon, v);
                            out.good("Weapon damage set!");
                        }
                    }
                    Ok(_) => {
                        changed = false;
                        out.bad(format!("Invalid weapon damage number: Number out of range (must be < {}).", java_double(MAX_HEALTH)));
                    }
                    Err(e) => {
                        changed = false;
                        out.bad(format!("Invalid weapon damage number: {e}"));
                    }
                }
            }
        }
        "weaponredirect" => {
            let one = a[0].to_lowercase();
            if !look.is_material(&one) {
                out.bad("Invalid weapon-one material (name misspelled?)");
                return out;
            }
            if a.len() < 2 {
                changed = false;
                let cur = d.weapon_redirects.get(&one).cloned().unwrap_or_else(|| "Unset".into());
                out.good(format!("Current weapon redirect for '{one}': {EMPH}{cur}"));
            } else {
                let two = a[1].to_lowercase();
                if !look.is_material(&two) {
                    out.bad("Invalid weapon-two material (name misspelled?)");
                    return out;
                }
                d.weapon_redirects.insert(one, two);
                out.good("Weapon redirect set!");
            }
        }
        "dropchance" => {
            if a.len() < 2 {
                changed = false;
                if d.drops.is_empty() {
                    out.bad("No drops currently set on this NPC. Use /sentinel drops");
                    changed = true;
                    d.drop_chances.clear();
                } else {
                    for (n, item) in d.drops.iter().enumerate() {
                        let ty = item.str("type").unwrap_or("?").to_uppercase();
                        let chance = d.drop_chances.get(n).map_or(100.0, |c| c * 100.0);
                        out.good(format!("[{}]: {ty}: {}%", n + 1, java_double(chance)));
                    }
                }
            } else {
                let parsed = ji(a[0]).and_then(|i| jd(a[1]).map(|c| (i - 1, c)));
                let Ok((id, chance)) = parsed else {
                    out.bad("Input invalid. Arguments must be an integer then a double - /sentinel dropchance ID CHANCE");
                    return out;
                };
                if id < 0 || id >= d.drops.len() as i64 {
                    out.bad("Input invalid. ID must be a valid listed ID - to see valid IDs, type /sentinel dropchance");
                    return out;
                }
                if !(0.0..=100.0).contains(&chance) {
                    out.bad("Input invalid. Chance must be from 0 to 100 (percent).");
                    return out;
                }
                let id = id as usize;
                d.drop_chances.truncate(d.drops.len());
                while d.drop_chances.len() < d.drops.len() {
                    d.drop_chances.push(1.0);
                }
                d.drop_chances[id] = chance * 0.01;
                let ty = d.drops[id].str("type").unwrap_or("?").to_uppercase();
                out.good(format!("Drop chance for {ty} set to {}%.", java_double(chance)));
            }
        }
        "drops" => {
            changed = false;
            if sender.player.is_none() {
                out.bad("Players only!");
            } else {
                out.bad("The drops menu is not available yet; edit `drops` in the NPC's saves.");
            }
        }
        "spawnpoint" | "avoidreturnpoint" => {
            let spawn = spec.name == "spawnpoint";
            let Some(b) = &body else {
                changed = false;
                out.bad("NPC must be spawned for this command!");
                return out;
            };
            let block = [b.pos[0].floor(), b.pos[1].floor(), b.pos[2].floor()];
            let world = look.world_name(&b.dimension);
            let field = if spawn { &mut d.spawn_point } else { &mut d.avoid_return_point };
            let same = field.as_ref().is_some_and(|p| p.world == world && [p.x.floor(), p.y.floor(), p.z.floor()] == block);
            if same {
                *field = None;
                out.good(if spawn { "Spawn point removed!" } else { "Avoid-return point removed!" });
            } else {
                *field = Some(Location {
                    world,
                    world_id: None,
                    x: block[0] + 0.5,
                    y: block[1],
                    z: block[2] + 0.5,
                    yaw: if spawn { b.yaw } else { 0.0 },
                    pitch: 0.0,
                });
                if spawn {
                    out.good(format!(
                        "Spawn point updated to the NPC's current location: {}, {}, {}",
                        block[0] as i64, block[1] as i64, block[2] as i64
                    ));
                } else {
                    out.good("Avoid-return point updated!");
                }
            }
        }
        // WorldGuard is not part of Horizon: the original's answer when it is not loaded.
        "wgregion" => {
            changed = false;
            out.bad("WorldGuard not loaded! This command does nothing!");
        }
        "kill" => {
            changed = false;
            if body.is_none() {
                out.bad("NPC is already dead!");
            } else {
                out.effects.push(Effect::Kill(npc));
                out.good("Killed!");
            }
        }
        "respawn" => {
            changed = false;
            out.effects.push(Effect::Respawn(npc));
            out.good("Respawned!");
        }
        _ => {
            changed = false;
        }
    }
    if changed {
        out.effects.push(Effect::Dirty(npc));
    }
    out
}

/// `SentinelPlugin.getOwner(npc)`: the owner's name, else `Server/Unknown`; `server` for an NPC
/// no player owns.
fn owner_text(look: &dyn Lookup, npc: NpcId) -> String {
    match look.owner(npc) {
        Some(u) => look.player_name(u).unwrap_or_else(|| "Server/Unknown".into()),
        None => "server".into(),
    }
}

/// `getDamage(true)` for the weapon the body holds, without writing the weapon's value into the
/// NPC (the original does write it, as a side effect of a lookup).
fn calculated_damage(d: &SentinelData, body: Option<&EntityView>) -> f64 {
    let mut copy = d.clone();
    crate::combat::damage(&mut copy, true, body.and_then(|b| b.main_hand.as_ref()))
}

/// A command that shows, or sets, one number: no argument shows `show: value`, a number that
/// `check` accepts is stored. The error text is `bad: <reason>`. Returns whether it changed.
fn number(
    out: &mut Outcome,
    arg: Option<&&str>,
    field: &mut f64,
    show: &str,
    done: &str,
    bad: &str,
    check: impl Fn(f64) -> Result<(), String>,
) -> bool {
    let Some(arg) = arg else {
        out.good(format!("{show}: {EMPH}{}", java_double(*field)));
        return false;
    };
    match jd(arg).and_then(|v| check(v).map(|_| v)) {
        Ok(v) => {
            *field = v;
            out.good(done);
            true
        }
        Err(e) => {
            out.bad(format!("{bad}: {e}"));
            false
        }
    }
}

fn help(out: &mut Outcome) {
    out.good("Sentinel commands:");
    for s in SUBCOMMANDS {
        out.lines.push(format!("{EMPH}/sentinel {} {BASIC}- {}", s.usage, s.desc));
    }
}

/// `SentinelTargetCommands.testLabel`.
fn test_label(out: &mut Outcome, label: &TargetLabel) -> bool {
    if !label.is_valid_target() {
        out.bad("Invalid target! See the readme for a list of valid targets.");
        return false;
    }
    if !label.is_valid_regex() {
        out.bad("Bad regular expression!");
        return false;
    }
    if !label.is_valid_prefix() {
        out.bad(format!("The target prefix '{}' is unknown!", label.prefix.as_deref().unwrap_or("")));
        return false;
    }
    if !label.is_valid_multi() {
        out.bad(format!("The multi-target '{}' is invalid (targets within don't exist?)!", label.value));
        return false;
    }
    true
}

/// `SentinelTargetCommands.outputEntireTargetsList`.
fn list_targets(out: &mut Outcome, list: &TargetList, prefix: &str) {
    let mut any = false;
    for (label, items) in [
        ("by Type", &list.targets),
        ("by Player Name", &list.by_player_name),
        ("by NPC Name", &list.by_npc_name),
        ("by Entity Name", &list.by_entity_name),
        ("by Held Item", &list.by_held_item),
        ("by Offhand Item", &list.by_offhand_item),
        ("by Equipped Item", &list.by_equipped_item),
        ("by Inventory-carried Item", &list.by_inventory_item),
        ("by Permissions Group", &list.by_group),
        ("by Event", &list.by_event),
        ("by Status", &list.by_status),
        ("by Other", &list.by_other),
    ] {
        if !items.is_empty() {
            out.good(format!("{prefix} {label}: {EMPH}{}", items.join(", ")));
            any = true;
        }
    }
    // Entries of a combined list are joined as Sentinel joins them: `<basic> , <emphasis>`.
    let comma = format!("{BASIC} , {EMPH}");
    let pipe = format!("{BASIC} | {EMPH}");
    for (i, all) in list.by_all_in_one.iter().enumerate() {
        out.good(format!("{prefix} by All-In-One ({EMPH}{i}{BASIC}): {EMPH}{}", all.combo_string(&pipe)));
        any = true;
    }
    for (i, multi) in list.by_multiple.iter().enumerate() {
        out.good(format!("{prefix} by Multiple ({EMPH}{i}{BASIC}): {EMPH}{}", multi.combo_string(&comma)));
        any = true;
    }
    if !any {
        out.good(format!("{prefix}: Nothing."));
    }
}

/// Java's `Map.toString` for the weapon maps (`{a=1.0, b=2.0}`), empty when empty.
fn map_text<V>(m: &std::collections::BTreeMap<String, V>, show: impl Fn(&V) -> String) -> String {
    if m.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = m.iter().map(|(k, v)| format!("{k}={}", show(v))).collect();
    format!("{{{}}}", parts.join(", "))
}

/// `SentinelInfoCommands.info`: every line the original adds, paged like Citizens' `Paginator`.
fn info(out: &mut Outcome, full_name: &str, d: &SentinelData, body: Option<&EntityView>, look: &dyn Lookup, npc: NpcId, page_arg: Option<&str>) {
    let guard = if let Some(u) = d.guarding() {
        look.player_name(u)
    } else if d.guarded_npc >= 0 && look.npc_exists(d.guarded_npc) {
        Some(format!("NPC {}: {}", d.guarded_npc, look.npc_name(d.guarded_npc).unwrap_or_default()))
    } else {
        None
    };
    let owner = match look.owner(npc) {
        Some(u) => look.player_name(u).unwrap_or_default(),
        None => "server".into(),
    };
    let calc_damage = calculated_damage(d, body);
    let armor = match body {
        Some(b) => format!("{}{BASIC} Calculated: {EMPH}{}", java_double(d.armor), java_double(crate::combat::armor(d, &b.armor))),
        None => java_double(d.armor),
    };
    let health = match body {
        Some(b) => format!("{}/{}", java_double(b.health), java_double(d.health)),
        None => java_double(d.health),
    };
    let secs = |t: i64| java_double(t as f64 / 20.0);
    let spawn = d
        .spawn_point
        .as_ref()
        .map(|p| format!("{},{},{}", java_double(p.x.floor()), java_double(p.y.floor()), java_double(p.z.floor())))
        .unwrap_or_default();
    let lines: Vec<(&str, String)> = vec![
        ("Owned by", owner),
        ("Guarding", guard.unwrap_or_default()),
        ("Damage", format!("{}{BASIC} Calculated: {EMPH}{}", java_double(d.damage), java_double(calc_damage))),
        ("Armor", armor),
        ("Health", health),
        ("Range", java_double(d.range)),
        ("Avoidance Range", java_double(d.avoid_range)),
        ("Attack Rate", secs(d.attack_rate)),
        ("Ranged Attack Rate", secs(d.attack_rate_ranged)),
        ("Heal Rate", secs(d.heal_rate)),
        ("Respawn Time", secs(d.respawn_time)),
        ("Reaction Slowdown", secs(d.reaction_slowdown)),
        ("Move Speed", format!("{BASIC}Walking: {EMPH}1.0{BASIC}, Attacking: {EMPH}{}", java_double(d.speed))),
        ("Accuracy", java_double(d.accuracy)),
        ("Reach", java_double(d.reach)),
        ("Projectile Range", java_double(d.projectile_range)),
        ("Greeting", d.greeting_text.clone()),
        ("Warning", d.warning_text.clone()),
        ("Greeting Range", java_double(d.greet_range)),
        ("Greeting Rate", d.greet_rate.to_string()),
        ("Guard Distance Minimum", java_double(d.guard_distance_minimum)),
        ("Guard Selection Range", java_double(d.guard_selection_range)),
        ("Invincibility Enabled", d.invincible.to_string()),
        ("Protected Enabled", d.protect_from_ignores.to_string()),
        ("Protect From Range", java_double(d.protect_from_range)),
        ("Fightback Enabled", d.fightback.to_string()),
        ("Ranged Chasing Enabled", d.ranged_chase.to_string()),
        ("Close-Quarters Chasing Enabled", d.close_chase.to_string()),
        ("Maximum chase range", java_double(d.chase_range)),
        ("Safe-Shot Enabled", d.safe_shot.to_string()),
        ("Enemy-Drops Enabled", d.enemy_drops.to_string()),
        ("Enemy Target Time", secs(d.enemy_target_time)),
        ("Autoswitch Enabled", d.autoswitch.to_string()),
        ("Needs-Ammo Enabled", d.needs_ammo.to_string()),
        ("Realistic Targeting Enabled", d.realistic.to_string()),
        ("Knockback allowed", d.allow_knockback.to_string()),
        ("Run-Away Enabled", d.runaway.to_string()),
        ("Squad", d.squad.clone().unwrap_or_default()),
        ("Spawnpoint", spawn),
        ("Per-weapon damage values", map_text(&d.weapon_damage, |v| java_double(*v))),
        ("Weapon redirections", map_text(&d.weapon_redirects, |v| v.clone())),
        ("Drops", d.drops.len().to_string()),
        ("Death XP", d.death_xp.to_string()),
    ];
    // `addLineIfNeeded`: an empty text is left out.
    let lines: Vec<String> = lines
        .into_iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("{GOOD}{k}: {EMPH}{v}"))
        .collect();
    let mut page = 1usize;
    if let Some(p) = page_arg {
        match ji(p) {
            Ok(n) => page = n.max(0) as usize,
            Err(_) => out.bad("First argument must be a valid page number."),
        }
    }
    let pages = lines.len().div_ceil(INFO_PER_PAGE).max(1);
    if page < 1 || page > pages {
        out.red("Page not found.");
        return;
    }
    out.lines.push(format!("{GOOD}{full_name}"));
    out.lines.extend(lines.into_iter().skip((page - 1) * INFO_PER_PAGE).take(INFO_PER_PAGE));
    out.lines.push(format!("{BASIC}Page {page}/{pages} - /sentinel info <page>"));
}

/// `SentinelCommand.onTabComplete`: the completions of the target argument of `addtarget`,
/// `addignore` and `addavoid` (prefixes, type names, `helditem:` ... with `name:` and `lore:`)
/// and of `removetarget`, `removeignore` and `removeavoid` (what the NPC's list holds), for an
/// NPC that is a Sentinel. `args` are the words after `/sentinel`, the last the one being typed.
/// None: not this command's business (the subcommand names come from the command tree).
/// GAP: the item prefixes complete `lore:` and `name:` only; Bukkit adds every material name.
pub fn tab_complete(reg: &Registry, selected: Option<NpcId>, args: &[String]) -> Option<Vec<String>> {
    if args.len() != 2 {
        return None;
    }
    let s = reg.sentinels.get(&selected?)?;
    let low = args[1].to_lowercase();
    let filter = |items: Vec<String>, arg: &str| -> Vec<String> {
        let low = arg.to_lowercase();
        items.into_iter().filter(|i| i.starts_with(&low)).collect()
    };
    match args[0].to_lowercase().as_str() {
        "addtarget" | "addignore" | "addavoid" => {
            if let Some(colon) = low.find(':') {
                if colon + 1 < low.len() {
                    let prefix = &low[..colon];
                    if ["helditem", "offhand", "equipped", "in_inventory"].contains(&prefix) {
                        let items = vec![format!("{prefix}:lore:"), format!("{prefix}:name:")];
                        return Some(filter(items, &low));
                    }
                }
            }
            let mut all: Vec<String> = ["player:", "npc:", "entityname:", "group:", "helditem:", "offhand:", "equipped:", "in_inventory:"]
                .iter()
                .map(|p| (*p).to_owned())
                .collect();
            all.extend(
                [
                    "status:angry", "status:passive", "event:", "event:pvp", "event:pve", "event:pvnpc", "event:pvsentinel",
                    "event:eve", "event:pv:", "event:ev:", "event:npcvnpc", "event:guarded_fight", "event:message:",
                ]
                .iter()
                .map(|p| (*p).to_owned()),
            );
            all.extend(crate::target::all_accepted_names());
            Some(filter(all, &low))
        }
        "removetarget" => Some(filter(s.data.all_targets.removable_strings(), &args[1])),
        "removeignore" => Some(filter(s.data.all_ignores.removable_strings(), &args[1])),
        "removeavoid" => Some(filter(s.data.all_avoids.removable_strings(), &args[1])),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::Node;

    struct Look;
    impl Lookup for Look {
        fn body(&self, _: NpcId) -> Option<EntityView> {
            Some(EntityView { pos: [10.3, 64.0, -3.7], dimension: "minecraft:overworld".into(), ..Default::default() })
        }
        fn player_uuid(&self, name: &str) -> Option<u128> {
            (name == "Alice").then_some(5)
        }
        fn player_name(&self, u: u128) -> Option<String> {
            (u == 5).then(|| "Alice".into())
        }
        fn owner(&self, _: NpcId) -> Option<u128> {
            Some(5)
        }
        fn world_name(&self, _: &str) -> String {
            "world".into()
        }
    }

    fn op() -> Sender {
        Sender { player: Some(9), op_level: 4, selected: Some(60) }
    }

    fn reg() -> Registry {
        let mut r = Registry::default();
        r.load(60, &Node::map());
        r
    }

    #[test]
    fn addignore_npc_id() {
        let mut r = reg();
        let o = run(&mut r, &op(), "addignore npc:3", &Look);
        assert!(o.lines[0].contains("Ignoring new target!"));
        assert!(o.effects.contains(&Effect::Dirty(60)));
        assert_eq!(r.sentinels[&60].data.all_ignores.by_npc_name, vec!["3"]);
        let o = run(&mut r, &op(), "addignore npc:3", &Look);
        assert!(o.lines[0].contains("Already ignoring"));
    }

    #[test]
    fn respawntime_in_seconds() {
        let mut r = reg();
        run(&mut r, &op(), "respawntime 0", &Look);
        assert_eq!(r.sentinels[&60].data.respawn_time, 0);
        run(&mut r, &op(), "respawntime 5", &Look);
        assert_eq!(r.sentinels[&60].data.respawn_time, 100);
        let o = run(&mut r, &op(), "respawntime", &Look);
        assert!(o.lines[0].ends_with("Current respawn time: \u{a7}b5.0"));
    }

    #[test]
    fn deathxp_refuses_bad_input() {
        let mut r = reg();
        let o = run(&mut r, &op(), "deathxp abc", &Look);
        assert!(o.lines[0].contains("Invalid XP number: For input string: \"abc\""));
        let o = run(&mut r, &op(), "deathxp -5", &Look);
        assert!(o.lines[0].contains("Number out of range (must be >= 0)."));
        assert!(!o.effects.iter().any(|e| matches!(e, Effect::Dirty(_))));
        run(&mut r, &op(), "deathxp 7", &Look);
        assert_eq!(r.sentinels[&60].data.death_xp, 7);
    }

    #[test]
    fn id_flag_and_ownership() {
        let mut r = reg();
        let stranger = Sender { player: Some(77), op_level: 0, selected: None };
        let o = run(&mut r, &stranger, "health 30 --id 60", &Look);
        assert!(o.lines[0].contains("do not have permission"));
        let op_not_owner = Sender { player: Some(77), op_level: 2, selected: None };
        // Operators hold citizens.admin, so they may edit other people's NPCs.
        let o = run(&mut r, &op_not_owner, "health 30 --id 60", &Look);
        assert!(o.effects.contains(&Effect::Health(60, 30.0)));
        let owner = Sender { player: Some(5), op_level: 2, selected: None };
        let o = run(&mut r, &owner, "health 25 --id 60", &Look);
        assert!(o.effects.contains(&Effect::Health(60, 25.0)));
        let o = run(&mut r, &owner, "health 25 --id x", &Look);
        assert!(o.lines[0].contains("ID input is invalid (not an integer)!"));
    }

    #[test]
    fn needs_a_selected_sentinel() {
        let mut r = reg();
        let none = Sender { player: Some(9), op_level: 4, selected: None };
        let o = run(&mut r, &none, "health 30", &Look);
        assert!(o.lines[0].contains("Must have a Sentinel NPC selected!"));
        let other = Sender { player: Some(9), op_level: 4, selected: Some(61) };
        let o = run(&mut r, &other, "health 30", &Look);
        assert!(o.lines[0].contains("Selected NPC is not a Sentinel!"));
    }

    #[test]
    fn spawnpoint_toggles() {
        let mut r = reg();
        run(&mut r, &op(), "spawnpoint", &Look);
        let p = r.sentinels[&60].data.spawn_point.clone().unwrap();
        assert_eq!((p.x, p.y, p.z), (10.5, 64.0, -3.5));
        run(&mut r, &op(), "spawnpoint", &Look);
        assert!(r.sentinels[&60].data.spawn_point.is_none());
    }

    #[test]
    fn invalid_targets_are_refused() {
        let mut r = reg();
        let o = run(&mut r, &op(), "addtarget dragons_and_unicorns", &Look);
        assert!(o.lines[0].contains("Invalid target! See the readme for a list of valid targets."));
        let o = run(&mut r, &op(), "addtarget player:(", &Look);
        assert!(o.lines[0].contains("Bad regular expression!"));
        let o = run(&mut r, &op(), "addtarget bogus:x", &Look);
        assert!(o.lines[0].contains("The target prefix 'bogus' is unknown!"));
    }

    #[test]
    fn unknown_and_short_commands() {
        let mut r = reg();
        let o = run(&mut r, &op(), "addtaget zombie", &Look);
        assert!(o.lines[0].contains("Unknown command. Did you mean:"));
        assert!(o.lines[1].ends_with(" /sentinel addtarget"));
        let o = run(&mut r, &op(), "weapondamage", &Look);
        assert_eq!(o.lines[0], "\u{a7}cToo few arguments.");
        assert_eq!(o.lines[1], "\u{a7}cUsage: /sentinel weapondamage MATERIAL DAMAGE");
        let o = run(&mut r, &op(), "kill now", &Look);
        assert_eq!(o.lines[0], "\u{a7}cToo many arguments.");
    }

    #[test]
    fn numbers_are_checked_like_java() {
        let mut r = reg();
        let o = run(&mut r, &op(), "range 0", &Look);
        assert!(o.lines[0].contains("Invalid range number: Number out of range (must be > 0 and < 200)."));
        let o = run(&mut r, &op(), "range 12.5", &Look);
        assert!(o.lines[0].contains("Range set!"));
        assert_eq!(r.sentinels[&60].data.range, 12.5);
        let o = run(&mut r, &op(), "attackrate 0.1", &Look);
        assert!(o.lines[0].contains("must be >= 0.5 and <= 100.0"));
        let o = run(&mut r, &op(), "healrate 0", &Look);
        assert!(o.lines[0].contains("Heal rate set!"));
        let o = run(&mut r, &op(), "armor 2", &Look);
        assert!(o.lines[0].contains("must be <= 1"));
        let o = run(&mut r, &op(), "armor 0.25", &Look);
        assert!(o.lines[0].contains("(25.0%)"));
        assert_eq!(jd("3d"), Ok(3.0));
        assert!(jd("inf").is_err());
    }

    #[test]
    fn toggles_take_true_false_or_flip() {
        let mut r = reg();
        let o = run(&mut r, &op(), "invincible", &Look);
        assert!(o.lines[0].contains("NPC now invincible!"));
        assert!(o.effects.contains(&Effect::Protected(60, true)));
        let o = run(&mut r, &op(), "invincible TRUE", &Look);
        assert!(o.lines[0].contains("NPC now invincible!"));
        let o = run(&mut r, &op(), "invincible false", &Look);
        assert!(o.effects.contains(&Effect::Protected(60, false)));
    }

    #[test]
    fn greeting_is_stored_as_typed() {
        let mut r = reg();
        let o = run(&mut r, &op(), "greeting &aHello  there", &Look);
        assert!(o.lines[0].ends_with("Set!"));
        assert_eq!(r.sentinels[&60].data.greeting_text, "&aHello there");
    }

    #[test]
    fn targets_listing() {
        let mut r = reg();
        run(&mut r, &op(), "addtarget zombie", &Look);
        let o = run(&mut r, &op(), "targets", &Look);
        assert!(o.lines.iter().any(|l| l.contains("Targeted by Type: ") && l.contains("ZOMBIE")));
        let o = run(&mut r, &op(), "avoids", &Look);
        assert!(o.lines.last().unwrap().contains("Avoided: Nothing."));
    }

    #[test]
    fn target_completions() {
        let mut r = reg();
        let w = |a: &str, b: &str| vec![a.to_owned(), b.to_owned()];
        let c = tab_complete(&r, Some(60), &w("addtarget", "zomb")).unwrap();
        assert!(c.contains(&"zombie".to_owned()) && c.contains(&"zombies".to_owned()));
        let c = tab_complete(&r, Some(60), &w("addignore", "pl")).unwrap();
        assert!(c.contains(&"player:".to_owned()) && c.contains(&"players".to_owned()));
        run(&mut r, &op(), "addignore npc:3", &Look);
        let c = tab_complete(&r, Some(60), &w("removeignore", "")).unwrap();
        assert!(c.contains(&"npc:3".to_owned()));
        assert!(tab_complete(&r, Some(61), &w("removeignore", "")).is_none());
        assert!(tab_complete(&r, Some(60), &["range".to_owned(), "".to_owned()]).is_none());
    }
}
