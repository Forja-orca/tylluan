/// Get the effective timeout for a guild based on its catalog weight and config.
pub fn guild_effective_timeout(guild_name: &str, low_memory_mode: bool) -> u64 {
    let catalog = crate::router::catalog::builtin_catalog();
    let guild_weight = catalog.iter()
        .find(|g| g.name == guild_name)
        .map(|g| g.weight)
        .unwrap_or_default();
    crate::config::effective_timeout_ms(guild_weight, low_memory_mode)
}
