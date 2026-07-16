use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use walkdir::WalkDir;

/// Lightweight snapshot of the repository structure, built once at startup.
#[derive(Debug, Clone, Serialize)]
pub struct RepoMap {
    pub root: String,
    pub built_at_unix: u64,
    pub build_duration_ms: u64,
    pub total_files: u64,
    pub total_dirs: u64,
    pub total_lines: u64,
    pub languages: HashMap<String, LangStats>,
    pub top_level_dirs: Vec<DirEntry>,
    pub key_files: Vec<FileEntry>,
    pub identifiers: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LangStats {
    pub files: u64,
    pub lines: u64,
    pub pct: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DirEntry {
    pub name: String,
    pub file_count: u64,
    pub dir_count: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileEntry {
    pub path: String,
    pub kind: &'static str,
}

const SKIP_DIRS: &[&str] = &[".git", "node_modules", "target", "__pycache__", ".venv", ".mypy_cache", ".pytest_cache", ".ruff_cache", ".hypothesis"];
const KEY_CONFIG_FILES: &[(&str, &str)] = &[
    ("Cargo.toml", "manifest"),
    ("package.json", "manifest"),
    ("pyproject.toml", "manifest"),
    ("tylluan.toml", "config"),
    ("forja.toml", "config"),
    ("tsconfig.json", "config"),
    ("README.md", "docs"),
    ("LICENSE", "license"),
    (".gitignore", "git"),
    ("Dockerfile", "docker"),
    ("Makefile", "build"),
    ("Justfile", "build"),
    ("docker-compose.yml", "docker"),
];

impl RepoMap {
    pub fn build(root: &Path) -> Arc<Self> {
        let start = Instant::now();
        let root_str = root.to_string_lossy().to_string();

        let mut total_files = 0u64;
        let mut total_dirs = 0u64;
        let mut total_lines = 0u64;
        let mut lang_counts: HashMap<String, (u64, u64)> = HashMap::new();
        let mut dir_children: HashMap<String, (u64, u64)> = HashMap::new();
        let mut identifiers: HashMap<String, Vec<String>> = HashMap::new();

        let walker = WalkDir::new(root)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                !SKIP_DIRS.contains(&name.as_ref())
            });

        for entry in walker.filter_map(|e| e.ok()) {
            let rel = entry.path().strip_prefix(root).unwrap_or(entry.path());
            let rel_str = rel.to_string_lossy().to_string();

            if entry.file_type().is_dir() {
                total_dirs += 1;
                if let Some(parent) = rel.parent() {
                    if parent == Path::new("") {
                        dir_children.entry(rel_str.clone())
                            .or_insert((0, 0)).1 += 1;
                    } else if parent.components().count() == 1 {
                        let top = parent.to_string_lossy().to_string();
                        dir_children.entry(top)
                            .or_insert((0, 0)).1 += 1;
                    }
                }
            } else if entry.file_type().is_file() {
                total_files += 1;

                if let Some(parent) = rel.parent() {
                    if parent == Path::new("") || parent.components().count() == 0 {
                        // top-level file, no dir counting
                    } else if parent.components().count() == 1 {
                        let top = parent.to_string_lossy().to_string();
                        dir_children.entry(top)
                            .or_insert((0, 0)).0 += 1;
                    }
                }

                let ext = entry.path().extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_default();

                let lang = match ext.as_str() {
                    "rs" => "Rust",
                    "py" => "Python",
                    "ts" | "tsx" => "TypeScript",
                    "js" | "jsx" => "JavaScript",
                    "css" | "scss" | "sass" | "less" => "CSS",
                    "html" | "htm" => "HTML",
                    "json" => "JSON",
                    "toml" => "TOML",
                    "yaml" | "yml" => "YAML",
                    "md" => "Markdown",
                    "sql" => "SQL",
                    "sh" | "bash" => "Shell",
                    "bat" | "cmd" | "ps1" => "Script",
                    "proto" => "Protobuf",
                    "vue" => "Vue",
                    "svelte" => "Svelte",
                    _ => "Other",
                };

                let line_count = count_lines(entry.path());
                total_lines += line_count as u64;

                let lang_stat = lang_counts.entry(lang.to_string()).or_insert((0, 0));
                lang_stat.0 += 1;
                lang_stat.1 += line_count as u64;

                if ext == "rs" {
                    let idents = extract_rust_identifiers(entry.path());
                    if !idents.is_empty() {
                        identifiers.insert(rel_str, idents);
                    }
                }
            }
        }

        let total = total_lines as f64;
        let languages: HashMap<String, LangStats> = lang_counts.into_iter()
            .map(|(k, (files, lines))| {
                let pct = if total > 0.0 { (lines as f64 / total) * 100.0 } else { 0.0 };
                (k, LangStats { files, lines, pct })
            })
            .collect();

        let mut top_level: Vec<DirEntry> = dir_children.into_iter()
            .filter(|(name, _)| !SKIP_DIRS.contains(&name.as_str()))
            .map(|(name, (file_count, dir_count))| DirEntry { name, file_count, dir_count })
            .collect();
        top_level.sort_by_key(|d| std::cmp::Reverse(d.file_count));

        let key_files: Vec<FileEntry> = find_key_files(root);

        let built_at_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let build_duration_ms = start.elapsed().as_millis() as u64;

        Arc::new(RepoMap {
            root: root_str,
            built_at_unix,
            build_duration_ms,
            total_files,
            total_dirs,
            total_lines,
            languages,
            top_level_dirs: top_level,
            key_files,
            identifiers,
        })
    }
}

fn count_lines(path: &Path) -> usize {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.lines().count())
        .unwrap_or(0)
}

fn extract_rust_identifiers(path: &Path) -> Vec<String> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    let mut results = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("pub fn ") {
            let name = trimmed.strip_prefix("pub fn ")
                .and_then(|s| s.split(&['(', ' ', '<'][..]).next())
                .unwrap_or("");
            if !name.is_empty() {
                results.push(format!("fn {name}"));
            }
        } else if trimmed.starts_with("pub struct ") {
            let name = trimmed.strip_prefix("pub struct ")
                .and_then(|s| s.split(&[' ', '<', '{'][..]).next())
                .unwrap_or("");
            if !name.is_empty() {
                results.push(format!("struct {name}"));
            }
        } else if trimmed.starts_with("pub trait ") {
            let name = trimmed.strip_prefix("pub trait ")
                .and_then(|s| s.split(&[' ', '<', '{'][..]).next())
                .unwrap_or("");
            if !name.is_empty() {
                results.push(format!("trait {name}"));
            }
        } else if trimmed.starts_with("pub enum ") {
            let name = trimmed.strip_prefix("pub enum ")
                .and_then(|s| s.split(&[' ', '<', '{'][..]).next())
                .unwrap_or("");
            if !name.is_empty() {
                results.push(format!("enum {name}"));
            }
        } else if trimmed.starts_with("pub(crate) fn ") {
            let name = trimmed.strip_prefix("pub(crate) fn ")
                .and_then(|s| s.split(&['(', ' ', '<'][..]).next())
                .unwrap_or("");
            if !name.is_empty() {
                results.push(format!("fn {name}"));
            }
        } else if trimmed.starts_with("pub const ") {
            let name = trimmed.strip_prefix("pub const ")
                .and_then(|s| s.split(&[' ', ':', '='][..]).next())
                .unwrap_or("");
            if !name.is_empty() {
                results.push(format!("const {name}"));
            }
        } else if trimmed.starts_with("pub mod ") {
            let name = trimmed.strip_prefix("pub mod ")
                .and_then(|s| s.split(&[' ', ';', '{'][..]).next())
                .unwrap_or("");
            if !name.is_empty() {
                results.push(format!("mod {name}"));
            }
        } else if trimmed.starts_with("pub async fn ") {
            let name = trimmed.strip_prefix("pub async fn ")
                .and_then(|s| s.split(&['(', ' ', '<'][..]).next())
                .unwrap_or("");
            if !name.is_empty() {
                results.push(format!("async fn {name}"));
            }
        } else if trimmed.starts_with("pub(crate) async fn ") {
            let name = trimmed.strip_prefix("pub(crate) async fn ")
                .and_then(|s| s.split(&['(', ' ', '<'][..]).next())
                .unwrap_or("");
            if !name.is_empty() {
                results.push(format!("async fn {name}"));
            }
        }
    }
    results
}

fn find_key_files(root: &Path) -> Vec<FileEntry> {
    let mut files = Vec::new();
    let mut check = |name: &str, kind: &'static str| {
        if root.join(name).exists() {
            files.push(FileEntry { path: name.to_string(), kind });
        }
    };
    for (name, kind) in KEY_CONFIG_FILES {
        check(name, kind);
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    #[test]
    fn test_repo_map_builds_in_temp_dir() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::create_dir_all(dir.path().join("tests")).unwrap();

        let mut f = fs::File::create(dir.path().join("src/main.rs")).unwrap();
        writeln!(f, "pub fn main() {{}}").unwrap();
        writeln!(f, "pub struct Foo {{}}").unwrap();

        let mut f2 = fs::File::create(dir.path().join("Cargo.toml")).unwrap();
        writeln!(f2, "[package]\nname = \"test\"").unwrap();

        let map = RepoMap::build(dir.path());
        assert!(map.total_files >= 2, "should find at least 2 files");
        assert!(map.total_dirs >= 2, "should find at least 2 dirs");
        assert!(map.total_lines >= 3, "should count lines");
        assert!(map.languages.contains_key("Rust"), "should detect Rust");
        assert!(map.languages.contains_key("TOML"), "should detect TOML");
        assert!(!map.identifiers.is_empty(), "should extract Rust identifiers");
        assert!(map.key_files.iter().any(|f| f.path == "Cargo.toml"), "should find Cargo.toml");
        // build_duration_ms is not asserted > 0 here: a 2-file temp dir can
        // legitimately build in under 1ms on fast hardware/CI runners, making
        // that assertion flaky. The field's presence/type is guaranteed by
        // the struct definition; its value on a real multi-thousand-file repo
        // is exercised by repo_map_endpoint_test.rs instead.
    }

    #[test]
    fn test_repo_map_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let map = RepoMap::build(dir.path());
        assert_eq!(map.total_files, 0);
        assert_eq!(map.total_lines, 0);
        assert!(map.languages.is_empty());
        assert!(map.identifiers.is_empty());
    }

    #[test]
    fn test_extract_rust_identifiers_various() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lib.rs");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(f, "pub fn hello() {{}}").unwrap();
        writeln!(f, "pub struct User {{}}").unwrap();
        writeln!(f, "pub trait Speak {{}}").unwrap();
        writeln!(f, "pub enum Color {{}}").unwrap();
        writeln!(f, "pub const MAX: u32 = 100;").unwrap();
        writeln!(f, "pub mod utils;").unwrap();
        writeln!(f, "pub async fn fetch() {{}}").unwrap();
        writeln!(f, "pub(crate) fn helper() {{}}").unwrap();
        writeln!(f, "pub(crate) async fn load() {{}}").unwrap();

        let idents = extract_rust_identifiers(&path);
        assert_eq!(idents.len(), 9);
        assert!(idents.contains(&"fn hello".to_string()));
        assert!(idents.contains(&"struct User".to_string()));
        assert!(idents.contains(&"trait Speak".to_string()));
        assert!(idents.contains(&"enum Color".to_string()));
        assert!(idents.contains(&"const MAX".to_string()));
        assert!(idents.contains(&"mod utils".to_string()));
        assert!(idents.contains(&"async fn fetch".to_string()));
        assert!(idents.contains(&"fn helper".to_string()));
        assert!(idents.contains(&"async fn load".to_string()));
    }
}
