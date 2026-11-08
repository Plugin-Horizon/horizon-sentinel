// A player's chat message (`AsyncPlayerChatEvent`): `event:message:<text>` targets and avoids.
// Lives in horizon-client-lib (the chat hook is a free function there).
fn on_chat(player: u128, message: &str) {
    if let Some(view) = crate::sentinel::view_by_uuid(player) {
        crate::sentinel::core::registry().on_chat(&view, message);
    }
}
