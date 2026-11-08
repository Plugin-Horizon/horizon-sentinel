// The server's tick: respawn timers of dead Sentinels (`respawn time` > 0).
fn on_server_tick(phase: &horizon_client::mod_api::ServerTickPhase) {
    if let horizon_client::mod_api::ServerTickPhase::ServerTickEnd { world_running: true } = phase {
        horizon_client::sentinel::with_bridge(|bridge| {
            horizon_client::sentinel::core::registry().tick(bridge, None);
        });
    }
}
