//! Canonical list of "lazy" (on-demand) guild registrations.
//!
//! # Why this exists
//!
//! `main.rs` used to hold this as a private `let lazy_guilds = vec![...]`
//! inline inside `main()`. `router::catalog::builtin_catalog()` is the
//! source of truth for "guilds tylluan_do is *allowed* to route to"
//! (enforced by `test_every_guild_file_is_in_catalog`), but nothing ever
//! checked that a catalog entry was also reachable at runtime — i.e. that
//! it was actually registered by main.rs, either here or via
//! `tylluan.toml`'s `[guilds.core] always_on` list.
//!
//! That gap shipped the same bug twice: a guild had a `.py` file, a
//! `GuildDescriptor` in catalog.rs, and passed every existing test — but
//! `tylluan_do` answered "Unknown guild: X" in production because nobody
//! added it to the `lazy_guilds` vec in main.rs (docker; then separately
//! night_reasoner/llama_backend).
//!
//! Pulling the list out into this public constant lets
//! `router::catalog::tests::test_lazy_or_always_on_guilds_are_registered`
//! assert, at compile/test time, that every catalog guild (minus the ones
//! explicitly excused — see that test) is present either here or in
//! tylluan.toml's `always_on` list. main.rs imports this constant
//! unchanged — registration behavior is not modified by this refactor.
///
/// Each entry is `(guild_name, python_module_path, is_cpu_bound)`.
pub const LAZY_GUILDS: &[(&str, &str, bool)] = &[
    ("database", "guilds.core.database", false),
    ("browser", "guilds.core.browser", false),
    ("pdf", "guilds.core.pdf", false),
    ("code_analysis", "guilds.core.code_analysis", false),
    ("data_tools", "guilds.core.data_tools", false),
    ("formatter", "guilds.core.formatter", false),
    ("vision", "guilds.core.vision", false),
    ("ingest", "guilds.core.ingest", false),
    ("deep_analysis", "guilds.core.deep_analysis", false),
    ("mcp_bridge", "guilds.core.mcp_bridge", false),
    ("code_reviewer", "guilds.core.code_reviewer", false),
    ("deep_web_research", "guilds.core.deep_web_research", false),
    ("coloquio_digest", "guilds.core.coloquio_digest", false),
    ("coordinator", "guilds.core.coordinator", false),
    ("night_reasoner", "guilds.core.night_reasoner", false),
    ("llama_backend", "guilds.core.llama_backend", false),
    ("seed_tools", "guilds.core.seed_tools", false),
    ("git", "guilds.builders.plugins.git", false),
    ("docker", "guilds.builders.plugins.docker", false),
    ("scheduler", "guilds.core.scheduler", true),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lazy_guilds_have_no_duplicate_names() {
        let mut names: Vec<&str> = LAZY_GUILDS.iter().map(|(name, _, _)| *name).collect();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), before, "LAZY_GUILDS contains a duplicate guild name");
    }
}
