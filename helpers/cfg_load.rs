fn on_config(config: &crate::game::native_configs::NativeConfigFile) {
    if config.mod_id == horizon_client::sentinel::core::MOD_ID
        && let Err(e) = horizon_client::sentinel::core::load_config(&config.text)
    {
        eprintln!("[sentinel] config not read, keeping the old one: {e}");
    }
}
