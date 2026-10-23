//! T-DSH-52 — ¿cuánto tarda de verdad `RepoMap::build` con D-39a?
//!
//! Test **ignorado por defecto** (`#[ignore = "mide el repo real"]`): recorre el
//! repositorio REAL (no arranca kernel, no abre puertos, no toca motores; solo
//! lee disco) y compara con los tres arranques del log (`kernel.log`):
//!
//! ```text
//! Repo map built: 24880 files, 3404 dirs, 17898448 lines (979059ms)
//! Repo map built: 24878 files, 3404 dirs, 17900941 lines (945990ms)
//! Repo map built: 24884 files, 3404 dirs, 17904534 lines (1531144ms)
//! ```
//!
//! Uso (una sola vez; hay agentes usando CPU):
//!
//! ```text
//! TYLLUAN_REPO_ROOT=E:\tylluan \
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test repo_map_real_measure -- --ignored --nocapture
//! ```
//!
//! Sin `TYLLUAN_REPO_ROOT` usa la raíz detectada subiendo desde
//! `CARGO_MANIFEST_DIR` hasta el primer ancestro con `.git` (en un checkout
//! normal es el repo real; en un worktree es el worktree, que NO tiene los
//! artefactos pesados del repo del operador).
//!
//! Reglas D-39a que se miden (`crates/tylluan-kernel/src/repo_map.rs`):
//! `SKIP_DIRS` (+ `.fastembed_cache`), `SKIP_TOP_LEVEL_DIRS` (`models`, `logs`,
//! `data` solo en la raíz), `SKIP_TOP_LEVEL_PREFIXES` (`test-run-`, `target-`,
//! `boot-`) y `MAX_LINE_COUNT_BYTES` = 2 MiB (no se leen ficheros mayores para
//! contar líneas; se cuentan como fichero con 0 líneas).

use std::path::PathBuf;
use std::time::Instant;

use tylluan_kernel::repo_map::RepoMap;

/// Valores de los tres arranques del log (segundos, ficheros, líneas).
const LOG_RUNS: [(f64, u64, u64); 3] = [
    (979.059, 24_880, 17_898_448),
    (945.990, 24_878, 17_900_941),
    (1_531.144, 24_884, 17_904_534),
];

fn repo_root() -> PathBuf {
    if let Ok(p) = std::env::var("TYLLUAN_REPO_ROOT") {
        return PathBuf::from(p);
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for ancestor in manifest.ancestors() {
        if ancestor.join(".git").exists() {
            return ancestor.to_path_buf();
        }
    }
    manifest
}

#[test]
#[ignore = "mide el repo real: recorrido de disco completo (no arranca kernel ni toca puertos)"]
fn repo_map_build_on_real_repository() {
    let root = repo_root();
    assert!(root.exists(), "la raíz medida no existe: {}", root.display());
    assert!(
        root.join("Cargo.toml").exists() || root.join("guilds").exists(),
        "la raíz medida no parece el repo de Tylluan: {}",
        root.display()
    );
    println!("\n[medición] raíz: {}", root.display());

    let t0 = Instant::now();
    let map = RepoMap::build(&root);
    let elapsed = t0.elapsed();
    let secs = elapsed.as_secs_f64();

    println!(
        "[medición] RepoMap::build: {:.3} s | {} ficheros | {} directorios | {} líneas | build_duration_ms={}",
        secs, map.total_files, map.total_dirs, map.total_lines, map.build_duration_ms
    );

    // Comparación con los 3 arranques del log.
    println!("\n[comparación con kernel.log]");
    println!(
        "{:<28} {:>12} {:>10} {:>14} {:>10}",
        "arranque (log)", "segundos", "ficheros", "líneas", "factor"
    );
    for (log_secs, log_files, log_lines) in LOG_RUNS {
        println!(
            "{:<28} {:>12.3} {:>10} {:>14} {:>9.0}x",
            "pre-D-39a",
            log_secs,
            log_files,
            log_lines,
            log_secs / secs.max(0.001)
        );
    }
    let best = LOG_RUNS.iter().map(|r| r.0).fold(f64::INFINITY, f64::min);
    println!(
        "\n[medición] ahora {:.3} s vs mejor arranque del log {:.3} s → {:.0}x más rápido",
        secs,
        best,
        best / secs.max(0.001)
    );

    // Top directorios por número de ficheros que quedan en el mapa.
    let mut dirs = map.top_level_dirs.clone();
    dirs.sort_by_key(|d| std::cmp::Reverse(d.file_count));
    println!("\n[medición] top-level dirs del mapa ({}):", dirs.len());
    for d in dirs.iter().take(15) {
        println!("  {:<28} {:>8} ficheros", d.name, d.file_count);
    }

    // Lenguajes: confirma que el mapa sigue siendo representativo.
    let mut langs: Vec<_> = map.languages.iter().collect();
    langs.sort_by_key(|(_, s)| std::cmp::Reverse(s.lines));
    println!("\n[medición] lenguajes (top 10 por líneas):");
    for (lang, s) in langs.iter().take(10) {
        println!(
            "  {:<12} {:>8} ficheros {:>12} líneas {:>7.2}%",
            lang, s.files, s.lines, s.pct
        );
    }

    // Si sigue pasando de 10 s, la tabla de arriba ya señala qué directorios pesan.
    if secs > 10.0 {
        println!(
            "\n[AVISO] sigue por encima de 10 s ({secs:.3} s): revisar la tabla de directorios/lenguajes"
        );
    } else {
        println!("\n[OK] por debajo de 10 s ({secs:.3} s)");
    }
    assert!(map.total_files > 0, "el mapa no encontró ficheros en {}", root.display());
    assert!(
        map.build_duration_ms as f64 <= secs * 1000.0 + 5_000.0,
        "build_duration_ms ({}) incoherente con el tiempo medido ({secs:.3} s)",
        map.build_duration_ms
    );
}
