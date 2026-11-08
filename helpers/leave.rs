// A body left the level (despawned, unloaded, removed): its goal state goes.
fn on_leave(entity: u32) {
    horizon_client::sentinel::forget(entity);
}
