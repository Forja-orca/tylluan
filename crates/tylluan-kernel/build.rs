use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

fn hash_into(path: &Path, hasher: &mut DefaultHasher) {
    if path.is_dir() {
        let mut entries: Vec<_> = match std::fs::read_dir(path) {
            Ok(rd) => rd.flatten().collect(),
            Err(_) => return,
        };
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            entry.file_name().hash(hasher);
            hash_into(&entry.path(), hasher);
        }
    } else if path.is_file()
        && let Ok(bytes) = std::fs::read(path) {
            bytes.hash(hasher);
        }
}

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());

    // M23/CLAUDE-4: expose the git commit so /health can report which binary runs.
    let commit = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(&manifest_dir)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=TYLLUAN_GIT_COMMIT={}", commit);
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/heads/main");

    let dashboard_dir = manifest_dir.join("..").join("..").join("dashboard");

    if !dashboard_dir.exists() {
        println!("cargo:warning=dashboard/ not found — skipping UI build");
        return;
    }

    // Fingerprint: hash src/ + package.json
    let mut hasher = DefaultHasher::new();
    hash_into(&dashboard_dir.join("src"), &mut hasher);
    hash_into(&dashboard_dir.join("package.json"), &mut hasher);
    let current = format!("{:x}", hasher.finish());

    let stamp = dashboard_dir.join(".dist-fingerprint");
    let stored = std::fs::read_to_string(&stamp).unwrap_or_default();
    let dist_ok = dashboard_dir.join("dist").join("index.html").exists();

    if stored.trim() == current && dist_ok {
        return; // Nothing changed — skip npm build
    }

    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };

    // Install dependencies if node_modules is missing
    if !dashboard_dir.join("node_modules").exists() {
        let ok = Command::new(npm)
            .args(["install"])
            .current_dir(&dashboard_dir)
            .status()
            .unwrap_or_else(|e| panic!("npm not found: {e} — install Node.js"));
        if !ok.success() {
            panic!("npm install failed in dashboard/");
        }
    }

    let ok = Command::new(npm)
        .args(["run", "build"])
        .current_dir(&dashboard_dir)
        .status()
        .unwrap_or_else(|e| panic!("npm not found: {e} — install Node.js"));

    if !ok.success() {
        panic!("Dashboard build failed — run 'npm run build' in dashboard/ to debug");
    }

    std::fs::write(&stamp, &current).ok();
}
