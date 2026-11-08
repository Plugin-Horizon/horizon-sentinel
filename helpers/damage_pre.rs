// After vanilla's armor, enchantment protection and absorption: a Sentinel's own share stands
// (Sentinel zeroes vanilla's modifiers and applies its own, `getArmor`).
fn on_damage_pre(entity: i32, amount: &mut f32) {
    if let Some(a) = horizon_client::sentinel::take_final_damage(entity) {
        *amount = a;
    }
}
