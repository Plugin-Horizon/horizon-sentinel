// `/sentinel addtarget|addignore|addavoid <target>` and `removetarget|removeignore|removeavoid
// <target>` complete as Sentinel's `onTabComplete` does: the prefixes and type names to add, what
// the selected NPC's list holds to remove. The usages register the target argument as `word?`,
// so the client asks the server for it; the subcommand names come from the command tree.
fn on_tab_complete(
    player: u128,
    label: &str,
    arguments: &[String],
    range: [usize; 2],
    suggestions: &mut Vec<horizon_client::mod_api::Suggestion>,
) {
    use horizon_client::sentinel as s;
    if label != "sentinel" {
        return;
    }
    let selected = s::with_bridge(|bridge| bridge.selected(player)).flatten();
    let words = {
        let reg = s::core::registry();
        s::core::commands::tab_complete(&reg, selected, arguments)
    };
    for text in words.unwrap_or_default() {
        suggestions.push(horizon_client::mod_api::Suggestion {
            range: (range[0], range[1]),
            text,
            tooltip: None,
            int_value: None,
        });
    }
}
