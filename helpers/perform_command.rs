// A player's `/sentinel ...`.
fn on_perform_command(
    game: &mut crate::game::Game<'_>,
    player: &horizon_client::mod_api::NativeServerPlayer,
    command: &str,
    ran: &mut bool,
) {
    let Some(args) = command.strip_prefix("sentinel").filter(|r| r.is_empty() || r.starts_with(' ')) else {
        return;
    };
    *ran = true;
    let lines = game.sentinel_command(Some(player.uuid), player.permission, args.trim());
    for line in lines {
        game.native_feedback(horizon_client::text::Text::literal(&line));
    }
}
