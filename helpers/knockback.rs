// `allow knockback` off: a hit on the Sentinel pushes it nowhere.
fn on_knockback(entity: i32, canceled: &mut bool) {
    if let Some(victim) = horizon_client::sentinel::view(entity)
        && victim.npc.is_some()
        && !horizon_client::sentinel::core::registry().allows_knockback(&victim)
    {
        *canceled = true;
    }
}
