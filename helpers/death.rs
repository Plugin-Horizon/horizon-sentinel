// A living entity dies: a Sentinel's own death (drops, xp, respawn or stay dead) or an enemy's
// (forgotten; drops cleared when a Sentinel with `enemy drops` off hurt it). The drop change is
// carried out by loot.rs when the dying entity's drops roll.
fn on_living_death(entity: u32) {
    use horizon_client::sentinel as s;
    let Some(dead) = s::view(entity as i32) else { return };
    let mut seed = (entity as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let outcome = s::with_bridge(|bridge| {
        s::core::registry().on_death(&dead, bridge, || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f64 / (1u64 << 53) as f64
        })
    });
    if let Some(outcome) = outcome
        && (outcome.replace_drops || outcome.xp.is_some())
    {
        crate::game::sentinel_host::note_death(entity, outcome);
    }
    s::forget(entity);
}
