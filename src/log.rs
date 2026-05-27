pub fn init() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .format_level(true)
        .format_target(false)
        .target(env_logger::Target::Stdout)
        .init();
}
