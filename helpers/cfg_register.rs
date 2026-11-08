fn on_register(configs: &mut Vec<crate::game::native_configs::NativeConfig>) {
    configs.push(crate::game::native_configs::NativeConfig::new(
        horizon_client::sentinel::core::MOD_ID,
        crate::game::native_configs::NativeConfigKind::Common,
        horizon_client::sentinel::core::DEFAULT_TOML,
    ));
}
