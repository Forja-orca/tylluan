#[cfg(test)]
mod dump_catalog_test {
    #[test]
    fn dump_catalog() {
        let catalog = tylluan_kernel::router::catalog::builtin_catalog();
        println!("CATALOG_COUNT: {}", catalog.len());
        for g in &catalog {
            println!("GUILD: {} cat={:?} mod={}", g.name, g.category, g.module_path);
        }

        // Real assertions — catalog must be non-empty and structurally valid
        assert!(!catalog.is_empty(), "builtin_catalog() returned empty — at least the core guilds must be present");

        // Every guild must have a non-empty name
        for g in &catalog {
            assert!(!g.name.is_empty(), "guild has empty name");
            assert!(!g.module_path.is_empty(), "guild '{}' has empty module_path", g.name);
        }

        // No duplicate guild names
        let mut names: Vec<&str> = catalog.iter().map(|g| g.name.as_str()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), catalog.len(), "duplicate guild names detected");

        // D-159: ya no hay guilds excluidos (vision_moondream se borro). El
        // mecanismo sigue vigilado en catalog.rs (test_excluded_guilds_are_not_routable).
    }
}
