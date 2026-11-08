// A hit on any living entity (`LivingEntity.hurtServer`, after its own guards): Sentinel's
// protection, armor, fire limit and damage override, and its bookkeeping (fightback, run away,
// safe shot, stats). `living_incoming_damage_v3` names the causing entity (a mob's or player's
// melee, an arrow's owner), so no earlier blow needs matching. The hook has no host: the views
// of the last level tick stand in.
fn on_incoming_damage(victim: u32, attacker: Option<u32>, damage_type: &str, amount: &mut f32, canceled: &mut bool) {
    use horizon_client::sentinel::{self as s, core::combat::HitDecision};
    if *canceled {
        return;
    }
    let Some(victim) = s::view(victim as i32) else { return };
    let attacker = attacker.and_then(|a| s::view(a as i32));
    // Only hits that involve a Sentinel, or that a Sentinel's event targets may care about.
    let decision = s::with_bridge(|bridge| {
        s::core::registry().on_hurt(&victim, attacker.as_ref(), damage_type, *amount, bridge)
    });
    match decision {
        Some(HitDecision::Cancel) => *canceled = true,
        Some(HitDecision::Amount(a)) => {
            *amount = a;
            // Sentinel's armor rule replaces vanilla's: `living_damage_pre` sets this amount as
            // what the health loses, after vanilla's armor and protection took their part.
            horizon_client::sentinel::set_final_damage(victim.id, a);
        }
        _ => {}
    }
}
