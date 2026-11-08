// `sentinel ...` typed on the server console (level 4, no selection: use `--id <npc>`).
fn on_console_command(game: &mut crate::game::Game<'_>, command: &str, ran: &mut bool) {
    let Some(args) = command.trim_start_matches('/').strip_prefix("sentinel").filter(|r| r.is_empty() || r.starts_with(' ')) else {
        return;
    };
    *ran = true;
    for line in game.sentinel_command(None, 4, args.trim()) {
        // Console colour codes are left in; the console strips them.
        println!("{line}");
    }
}
