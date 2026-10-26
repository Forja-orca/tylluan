//! Anti-regression tripwire test for double-encoded mojibake artifacts in crates/tylluan-kernel/src.
//!
//! Scans every .rs file in crates/tylluan-kernel/src (recursively) to ensure no corrupt
//! double-encoded UTF-8 artifacts (such as â€, Ã©, Ã³, ðŸ, âš, etc.) are present in comments,
//! log strings, doc comments, or literals.

use std::fs;
use std::path::{Path, PathBuf};

fn collect_rs_files(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_rs_files(&path, files);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }
}

#[test]
fn test_no_mojibake_in_kernel_src() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src_dir = manifest_dir.join("src");
    assert!(src_dir.exists(), "src/ directory must exist in {manifest_dir:?}");

    let mut rs_files = Vec::new();
    collect_rs_files(&src_dir, &mut rs_files);
    assert!(!rs_files.is_empty(), "Must find Rust source files in {src_dir:?}");

    // Corrupt double-encoded UTF-8 byte sequences:
    // "â€" -> [0xc3, 0xa2, 0xe2, 0x82, 0xac]
    // "Ã©" -> [0xc3, 0x83, 0xc2, 0xa9]
    // "Ã³" -> [0xc3, 0x83, 0xc2, 0xb3]
    // "ðŸ" -> [0xc3, 0xb0, 0xc5, 0xb8]
    // "âš" -> [0xc3, 0xa2, 0xc5, 0xa1]
    // "Ã¡" -> [0xc3, 0x83, 0xc2, 0xa1]
    // "Ã­" -> [0xc3, 0x83, 0xc2, 0xad]
    // "â†’" -> [0xc3, 0xa2, 0xe2, 0x80\xa0]
    let corrupt_patterns: &[(&str, &[u8])] = &[
        ("â€", &[0xc3, 0xa2, 0xe2, 0x82, 0xac]),
        ("Ã©", &[0xc3, 0x83, 0xc2, 0xa9]),
        ("Ã³", &[0xc3, 0x83, 0xc2, 0xb3]),
        ("ðŸ", &[0xc3, 0xb0, 0xc5, 0xb8]),
        ("âš", &[0xc3, 0xa2, 0xc5, 0xa1]),
        ("Ã¡", &[0xc3, 0x83, 0xc2, 0xa1]),
        ("Ã\\u{AD}", &[0xc3, 0x83, 0xc2, 0xad]),
        ("â†’", &[0xc3, 0xa2, 0xe2, 0x80, 0xa0]),
    ];

    let mut violations = Vec::new();

    for path in rs_files {
        let content = fs::read(&path)
            .unwrap_or_else(|e| panic!("Failed to read file {path:?}: {e}"));

        for &(pattern_name, pattern_bytes) in corrupt_patterns {
            if content.windows(pattern_bytes.len()).any(|w| w == pattern_bytes) {
                let rel = path.strip_prefix(&manifest_dir).unwrap_or(&path);
                violations.push(format!("{}: contains pattern '{pattern_name}'", rel.display()));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Mojibake artifacts found in crates/tylluan-kernel/src:\n{}",
        violations.join("\n")
    );
}
