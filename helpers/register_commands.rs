// `/sentinel <subcommand> [args]`: every player may send it; the core checks the permission nodes
// (`sentinel.basic`, `sentinel.<subcommand>`) and the NPC's owner or `citizens.admin`. Every
// subcommand path is a usage with its typed arguments, so the chat box completes the names, the
// fixed choices (`true`/`false`) and the argument kinds. The catch-all comes
// last: it keeps every other line reaching `perform_command`, which answers the errors.
// The target argument of the add/remove subcommands is completed by `tab_complete`.
fn on_register_commands(commands: &mut Vec<horizon_client::mod_api::NativeCommand>) {
    let mut usages: Vec<String> = vec!["sentinel".into(), "sentinel help".into()];
    // No argument.
    for sub in [
        "stats", "targets", "ignores", "avoids", "forgive", "spawnpoint", "avoidreturnpoint", "kill", "respawn",
        "drops", "debug", "reload",
    ] {
        usages.push(format!("sentinel {sub}"));
    }
    usages.push("sentinel info".into());
    usages.push("sentinel info <page:integer>".into());
    // The target to add or remove: asked of the server when typed (`word?`), where `tab_complete`
    // answers with the prefixes, type names and what the NPC's list holds.
    for sub in ["addtarget", "removetarget", "addignore", "removeignore", "addavoid", "removeavoid"] {
        usages.push(format!("sentinel {sub} <target:word?>"));
    }
    usages.push("sentinel forgive <player:player_name>".into());
    // A number (shows the current value when given none).
    for sub in [
        "range", "reach", "projectilerange", "accuracy", "chaserange", "avoidrange", "protectfromrange", "greetrange",
        "damage", "armor", "speed", "health",
    ] {
        usages.push(format!("sentinel {sub}"));
        usages.push(format!("sentinel {sub} <value:number>"));
    }
    // A time in seconds.
    for sub in ["healrate", "respawntime", "targettime", "greetrate"] {
        usages.push(format!("sentinel {sub}"));
        usages.push(format!("sentinel {sub} <seconds:number>"));
    }
    usages.push("sentinel attackrate".into());
    usages.push("sentinel attackrate <seconds:number>".into());
    usages.push("sentinel attackrate <seconds:number> ranged".into());
    usages.push("sentinel deathxp".into());
    usages.push("sentinel deathxp <xp:integer>".into());
    // On or off: `true` and `false` are literals, a bare one toggles.
    for sub in [
        "invincible", "protected", "fightback", "runaway", "knockback", "chaseclose", "chaseranged", "safeshot",
        "needammo", "autoswitch", "realistic", "enemydrops",
    ] {
        usages.push(format!("sentinel {sub}"));
        usages.push(format!("sentinel {sub} true"));
        usages.push(format!("sentinel {sub} false"));
    }
    usages.push("sentinel squad".into());
    usages.push("sentinel squad <name:text>".into());
    usages.push("sentinel guard".into());
    usages.push("sentinel guard <player:player_name>".into());
    usages.push("sentinel guarddistance".into());
    usages.push("sentinel guarddistance <minimum:number>".into());
    usages.push("sentinel guarddistance <minimum:number> <selection:number>".into());
    usages.push("sentinel weapondamage <material:word>".into());
    usages.push("sentinel weapondamage <material:word> <damage:number>".into());
    usages.push("sentinel weaponredirect <one:word> <two:word>".into());
    // Free text, last.
    usages.push("sentinel greeting <text:text>".into());
    usages.push("sentinel warning <text:text>".into());
    usages.push("sentinel dropchance".into());
    usages.push("sentinel dropchance <index:integer> <chance:number>".into());
    usages.push("sentinel wgregion".into());
    usages.push("sentinel wgregion <region:word>".into());
    usages.push("sentinel <args:text>".into());
    for usage in usages {
        commands.push(horizon_client::mod_api::NativeCommand::new(&usage));
    }
}
