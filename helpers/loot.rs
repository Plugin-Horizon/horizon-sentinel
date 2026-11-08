// The drops of a dead Sentinel (its own `drops` list) or of a Sentinel's enemy (none, with
// `enemy drops` off), replacing what the loot tables rolled.
fn on_loot_drops(
    context: &horizon_client::mod_api::NativeLootDrops<'_>,
    drops: &mut Vec<horizon_client::items::ItemStack>,
) {
    let Some((entity, _)) = context.entity else { return };
    let Some(outcome) = crate::game::sentinel_host::take_death(entity) else { return };
    if outcome.replace_drops {
        drops.clear();
        for d in outcome.drops {
            drops.push(crate::game::sentinel_host::drop_stack(&d));
        }
    }
}
