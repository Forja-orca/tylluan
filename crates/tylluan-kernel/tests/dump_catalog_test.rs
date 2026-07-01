#[cfg(test)]
mod dump_catalog_test {
    #[test]
    fn dump_catalog() {
        let catalog = tylluan_kernel::router::catalog::builtin_catalog();
        println!("CATALOG_COUNT: {}", catalog.len());
        for g in &catalog {
            println!("GUILD: {} cat={:?} mod={}", g.name, g.category, g.module_path);
        }
    }
}
