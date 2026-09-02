#!/usr/bin/env python3
"""
Count .unwrap() calls in production Rust code.

Simplified two-pass approach:
1. For each file, scan for test boundaries (mod tests, #[cfg(test)] items)
2. Mark test regions via brace-delimited blocks
3. Count .unwrap() in non-test regions only

Excludes dedicated test files (tests.rs, integration_tests.rs, files under tests/).

Usage: python3 tools/count_production_unwraps.py [--summary] [--json]
"""

import sys
import json
import re
from pathlib import Path

EXCLUDE_FILENAMES = {"tests.rs", "integration_tests.rs", "test_helpers.rs", "common.rs"}


def is_excluded_file(filepath):
    parts = Path(filepath).parts
    if any(p == "tests" for p in parts):
        return True
    if Path(filepath).name in EXCLUDE_FILENAMES:
        return True
    return False


def find_closing_brace(lines, start_line, start_col):
    """Find matching closing brace starting from an opening brace."""
    depth = 0
    in_str = False
    in_char = False
    in_block = False
    in_line = False

    for i in range(start_line, len(lines)):
        line = lines[i]
        col = start_col if i == start_line else 0
        in_line = False

        while col < len(line):
            c = line[col]
            nc = line[col + 1] if col + 1 < len(line) else ""

            if in_block:
                if c == "*" and nc == "/":
                    in_block = False
                    col += 2
                    continue
                col += 1
                continue
            if in_line:
                break
            if in_str:
                if c == "\\":
                    col += 2
                    continue
                if c == '"':
                    in_str = False
                col += 1
                continue
            if in_char:
                if c == "\\":
                    col += 2
                    continue
                if c == "'":
                    in_char = False
                col += 1
                continue

            if c == "/" and nc == "/":
                in_line = True
                col += 2
                continue
            if c == "/" and nc == "*":
                in_block = True
                col += 2
                continue
            if c == '"':
                in_str = True
                col += 1
                continue
            if c == "'" and (col == 0 or not line[col - 1].isalnum()):
                in_char = True
                col += 1
                continue
            if c == "{":
                depth += 1
            elif c == "}":
                depth -= 1
                if depth == 0:
                    return i
            col += 1

    return len(lines) - 1


def find_test_regions(lines):
    """
    Find line ranges that are test code.
    Returns list of (start, end) inclusive line indices.
    """
    regions = []
    i = 0
    n = len(lines)

    while i < n:
        stripped = lines[i].strip()

        # Pattern 1: #[cfg(test)] followed by mod tests { or mod tests;
        if stripped.startswith("#[cfg(test)]"):
            found = False
            for j in range(i + 1, min(i + 4, n)):
                js = lines[j].strip()
                if js == "":
                    continue
                # mod tests { ... }
                if re.match(r"mod\s+\w*\s*\{", js):
                    brace_col = lines[j].index("{")
                    end = find_closing_brace(lines, j, brace_col)
                    regions.append((i, end))
                    i = end + 1
                    found = True
                    break
                # mod tests; (external reference — skip attribute, file handled separately)
                if re.match(r"mod\s+\w*\s*;", js):
                    i = j + 1
                    found = True
                    break
                if not js.startswith("#") and not js.startswith("//"):
                    break
            if found:
                continue

            # Pattern 1b: #[cfg(test)] fn/struct/enum/impl (item-level)
            for j in range(i + 1, min(i + 4, n)):
                js = lines[j].strip()
                if js == "":
                    continue
                if re.match(r"(pub(\(.*?\))?\s+)?(async\s+)?(unsafe\s+)?fn\s+\w+", js) or \
                   re.match(r"(pub(\(.*?\))?\s+)?(struct|enum|trait)\s+\w+", js) or \
                   re.match(r"(pub(\(.*?\))?\s+)?impl[\s<]", js):
                    # Find the opening brace
                    brace_line = -1
                    if "{" in lines[j]:
                        brace_line = j
                    else:
                        for k in range(j + 1, min(j + 3, n)):
                            if "{" in lines[k]:
                                brace_line = k
                                break
                    if brace_line >= 0:
                        brace_col = lines[brace_line].index("{")
                        end = find_closing_brace(lines, brace_line, brace_col)
                        regions.append((i, end))
                        i = end + 1
                    else:
                        i = j + 1
                    found = True
                    break
                # Not an item — stop
                break
            if found:
                continue

            # Pattern 1c: inline #[cfg(test)] { ... } block
            for j in range(i + 1, min(i + 3, n)):
                js = lines[j].strip()
                if js == "":
                    continue
                if js == "{":
                    end = find_closing_brace(lines, j, lines[j].index("{"))
                    regions.append((i, end))
                    i = end + 1
                    found = True
                    break
                break
            if found:
                continue

            i += 1
            continue

        # Pattern 2: mod tests { without #[cfg(test)] above
        if re.match(r"\s*mod\s+\w*\s*\{", lines[i]):
            if i > 0 and "#[cfg(test)]" in lines[i - 1]:
                i += 1
                continue
            brace_col = lines[i].index("{")
            end = find_closing_brace(lines, i, brace_col)
            regions.append((i, end))
            i = end + 1
            continue

        # Pattern 3: standalone #[cfg(test)] on its own line (inline block)
        if stripped == "#[cfg(test)]":
            for j in range(i + 1, min(i + 3, n)):
                js = lines[j].strip()
                if js == "":
                    continue
                if js == "{":
                    end = find_closing_brace(lines, j, lines[j].index("{"))
                    regions.append((i, end))
                    i = end + 1
                    break
                # Check if it's a fn/struct
                if re.match(r"(pub(\(.*?\))?\s+)?(async\s+)?fn\s+\w+", js) or \
                   re.match(r"(pub(\(.*?\))?\s+)?(struct|enum)\s+\w+", js):
                    brace_line = -1
                    if "{" in lines[j]:
                        brace_line = j
                    else:
                        for k in range(j + 1, min(j + 3, n)):
                            if "{" in lines[k]:
                                brace_line = k
                                break
                    if brace_line >= 0:
                        end = find_closing_brace(lines, brace_line, lines[brace_line].index("{"))
                        regions.append((i, end))
                        i = end + 1
                    else:
                        i = j + 1
                    break
                break
            else:
                i += 1
            continue

        i += 1

    return regions


def count_unwraps_in_line(line):
    """Count .unwrap() in a line, skipping strings and comments."""
    count = 0
    in_str = False
    in_char = False
    in_line = False
    in_block = False
    i = 0
    while i < len(line):
        c = line[i]
        nc = line[i + 1] if i + 1 < len(line) else ""

        if in_block:
            if c == "*" and nc == "/":
                in_block = False
                i += 2
                continue
            i += 1
            continue
        if in_line:
            break
        if in_str:
            if c == "\\":
                i += 2
                continue
            if c == '"':
                in_str = False
            i += 1
            continue
        if in_char:
            if c == "\\":
                i += 2
                continue
            if c == "'":
                in_char = False
            i += 1
            continue

        if c == "/" and nc == "/":
            in_line = True
            i += 2
            continue
        if c == "/" and nc == "*":
            in_block = True
            i += 2
            continue
        if c == '"':
            in_str = True
            i += 1
            continue
        if c == "'" and (i == 0 or not line[i - 1].isalnum()):
            in_char = True
            i += 1
            continue

        if line[i:i + 8] == ".unwrap(":
            count += 1
            i += 8
            continue
        i += 1
    return count


def analyze_file(filepath):
    with open(filepath, "r", encoding="utf-8", errors="replace") as f:
        lines = f.readlines()

    test_regions = find_test_regions(lines)

    total = 0
    production = 0
    test_count = 0
    production_lines = []

    for i, line in enumerate(lines):
        wc = count_unwraps_in_line(line)
        if wc > 0:
            total += wc
            in_test = any(s <= i <= e for s, e in test_regions)
            if in_test:
                test_count += wc
            else:
                production += wc
                production_lines.append((i + 1, line.rstrip()))

    return {
        "total": total,
        "production": production,
        "test": test_count,
        "production_lines": production_lines,
    }


def main():
    summary_mode = "--summary" in sys.argv
    json_mode = "--json" in sys.argv

    crate_dirs = sorted(Path("crates").glob("*/src"))
    all_files = []
    for crate_src in crate_dirs:
        for rs_file in crate_src.rglob("*.rs"):
            all_files.append(str(rs_file))

    results = {}
    total_production = 0
    total_test = 0
    total_all = 0
    files_with_production_unwraps = []

    for filepath in sorted(all_files):
        if is_excluded_file(filepath):
            continue
        info = analyze_file(filepath)
        if info["production"] > 0:
            results[filepath] = info
            total_production += info["production"]
            total_test += info["test"]
            total_all += info["total"]
            files_with_production_unwraps.append((filepath, info["production"]))

    files_with_production_unwraps.sort(key=lambda x: -x[1])

    if json_mode:
        output = {
            "total_production_unwraps": total_production,
            "total_test_unwraps": total_test,
            "total_all_unwraps": total_all,
            "files": {
                fp: {
                    "production": info["production"],
                    "test": info["test"],
                    "total": info["total"],
                    "production_lines": [
                        {"line": ln, "text": text}
                        for ln, text in info["production_lines"]
                    ],
                }
                for fp, info in results.items()
            },
        }
        print(json.dumps(output, indent=2))
        return

    print(f"=== Production .unwrap() count ===")
    print(f"Methodology: brace-counting for #[cfg(test)] + mod tests exclusion")
    print(f"Files scanned: {len(all_files)} total, {len(results)} with production unwraps")
    print()

    if summary_mode:
        print(f"{'File':<70} {'Prod':>5} {'Test':>5} {'Total':>5}")
        print("-" * 90)
        for fp, count in files_with_production_unwraps:
            info = results[fp]
            short = fp.replace("crates/", "c/")
            print(f"{short:<70} {info['production']:>5} {info['test']:>5} {info['total']:>5}")
        print("-" * 90)
        print(f"{'TOTAL':<70} {total_production:>5} {total_test:>5} {total_all:>5}")
    else:
        for fp, count in files_with_production_unwraps:
            info = results[fp]
            print(f"\n--- {fp} ({info['production']} production, {info['test']} test, {info['total']} total) ---")
            for ln, text in info["production_lines"]:
                print(f"  L{ln}: {text.strip()}")

        print(f"\n{'='*60}")
        print(f"TOTAL: {total_production} production .unwrap() calls")
        print(f"       {total_test} test-only .unwrap() calls")
        print(f"       {total_all} total .unwrap() calls")


if __name__ == "__main__":
    main()
