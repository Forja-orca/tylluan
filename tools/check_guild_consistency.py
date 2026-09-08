#!/usr/bin/env python3
"""
G6 — Guild Identity Consistency Gate.

Verifies that all consumers of guild identity agree with the canonical
catalog (tools/guild_catalog.py). Run in CI on every PR that touches
guilds/, tylluan.toml, benchmarks, or router/catalog.rs.

Exit code 0 = all consistent, 1 = drift detected.

Design: docs/architecture/DESIGN_guild_identity_gate.md
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

# Ensure we can import the canonical catalog
sys.path.insert(0, str(Path(__file__).resolve().parent))
from guild_catalog import GUILDS, ROUTABLE_IDS, ALIASES

REPO_ROOT = Path(__file__).resolve().parent.parent


# ──────────────────────────────────────────────────────────────────────
# CHECK 1: Filesystem guilds match canonical catalog
# ──────────────────────────────────────────────────────────────────────

def check_filesystem() -> list[str]:
    """Every .py with FastMCP under guilds/ should be in the catalog."""
    errors = []
    exclude_files = {
        "__init__", "_security", "utils", "silva_utils", "memory_bridge",
        "analyze_friction_weights", "check_coloquio", "download_gemma4",
        "test_gemma_coordinator", "benchmark_routing",
        "antigravity_watcher", "claude_watcher", "forja_watcher",
        "lmstudio_watcher", "openai_compat_watcher",
    }

    for f in sorted(REPO_ROOT.glob("guilds/**/*.py")):
        if f.stem.startswith("__") or f.stem in exclude_files:
            continue
        content = f.read_text(errors="ignore")
        if "FastMCP" not in content:
            continue

        # This file has a FastMCP server — check it's in the catalog
        match = re.search(r'FastMCP\(["\']([^"\']+)', content)
        mcp_name = match.group(1) if match else "UNKNOWN"

        # The canonical ID comes from the catalog (via name_override logic)
        # Check if any guild entry points to this file
        module_path = str(f.relative_to(REPO_ROOT)).replace("\\", "/")
        # Try to find by module path (dots instead of slashes, no .py)
        module_dots = module_path.replace("/", ".").replace(".py", "")

        found = False
        for gid, g in GUILDS.items():
            if g.module == module_dots:
                found = True
                break

        if not found:
            errors.append(
                f"FILESYSTEM: {f.name} has FastMCP('{mcp_name}') but no "
                f"canonical catalog entry. Add it to tools/guild_catalog.py"
            )

    return errors


# ──────────────────────────────────────────────────────────────────────
# CHECK 2: catalog.rs KNOWN_GUILDS test list matches canonical
# ──────────────────────────────────────────────────────────────────────

def check_catalog_rs() -> list[str]:
    """The KNOWN_GUILDS list in catalog.rs tests should match routable IDs."""
    errors = []
    catalog_rs = REPO_ROOT / "crates/tylluan-kernel/src/router/catalog.rs"
    if not catalog_rs.exists():
        return ["CATALOG_RS: catalog.rs not found"]

    content = catalog_rs.read_text(errors="ignore")

    # Extract KNOWN_GUILDS
    match = re.search(
        r'const KNOWN_GUILDS: &\[&str\] = &\[(.*?)\];',
        content, re.DOTALL
    )
    if not match:
        return ["CATALOG_RS: Could not find KNOWN_GUILDS"]

    known = set(re.findall(r'"([^"]+)"', match.group(1)))

    # Extract EXCLUDED_GUILDS
    excl_match = re.search(
        r'const EXCLUDED_GUILDS: &\[&str\] = &\[(.*?)\];',
        content, re.DOTALL
    )
    excluded = set(re.findall(r'"([^"]+)"', excl_match.group(1))) if excl_match else set()

    # Known minus excluded should equal routable IDs
    effective_known = known - excluded
    routable_set = set(ROUTABLE_IDS)

    missing_in_rs = routable_set - effective_known
    extra_in_rs = effective_known - routable_set

    if missing_in_rs:
        errors.append(
            f"CATALOG_RS: KNOWN_GUILDS is missing routable guilds: "
            f"{sorted(missing_in_rs)}"
        )
    if extra_in_rs:
        errors.append(
            f"CATALOG_RS: KNOWN_GUILDS has guilds not in canonical catalog: "
            f"{sorted(extra_in_rs)}"
        )

    return errors


# ──────────────────────────────────────────────────────────────────────
# CHECK 3: TOML always_on names exist in canonical catalog
# ──────────────────────────────────────────────────────────────────────

def check_toml() -> list[str]:
    """TOML always_on and V2 plugin names should exist in canonical catalog."""
    errors = []
    toml_path = REPO_ROOT / "tylluan.toml"
    if not toml_path.exists():
        return ["TOML: tylluan.toml not found"]

    content = toml_path.read_text(errors="ignore")

    # Parse always_on list from [guilds.core]
    # Look for always_on = [...] block
    always_on_match = re.search(
        r'always_on\s*=\s*\[(.*?)\]', content, re.DOTALL
    )
    if always_on_match:
        always_on = set(re.findall(r'"([^"]+)"', always_on_match.group(1)))
        catalog_ids = set(GUILDS.keys())
        phantom = always_on - catalog_ids
        if phantom:
            errors.append(
                f"TOML: always_on references guilds not in canonical catalog: "
                f"{sorted(phantom)}"
            )
        # Check always_on guilds are routable
        non_routable = {g for g in always_on if GUILDS.get(g, type("", (), {"status": ""})()).status != "routable"}
        if non_routable:
            errors.append(
                f"TOML: always_on includes non-routable guilds: "
                f"{sorted(non_routable)}"
            )

    # Parse V2 plugin filenames
    v2_plugins = re.findall(r'"([a-z_]+\.py)"', content)
    for plugin in v2_plugins:
        stem = plugin.replace(".py", "")
        if stem not in GUILDS and stem not in ALIASES:
            errors.append(
                f"TOML: V2 plugin '{plugin}' has no canonical catalog entry"
            )

    return errors


# ──────────────────────────────────────────────────────────────────────
# CHECK 4: Guild IDs are unique (no duplicates)
# ──────────────────────────────────────────────────────────────────────

def check_uniqueness() -> list[str]:
    """Each GuildId should appear exactly once in the catalog."""
    errors = []
    ids = [g.id for g in GUILDS.values()]
    seen = set()
    for gid in ids:
        if gid in seen:
            errors.append(f"UNIQUE: Duplicate GuildId '{gid}' in catalog")
        seen.add(gid)

    # Check aliases don't collide with IDs
    for alias, target in ALIASES.items():
        if alias in GUILDS:
            errors.append(
                f"UNIQUE: Alias '{alias}' collides with GuildId '{alias}'"
            )

    return errors


# ──────────────────────────────────────────────────────────────────────
# CHECK 5: Module paths are syntactically valid
# ──────────────────────────────────────────────────────────────────────

def check_module_paths() -> list[str]:
    """Module paths should be valid Python import paths."""
    errors = []
    for gid, g in GUILDS.items():
        # Convert module dots to filesystem path
        fs_path = REPO_ROOT / g.module.replace(".", "/") / "__init__.py"
        if not fs_path.exists():
            # Try as a file directly (some guilds are single .py files)
            fs_path = Path(str(REPO_ROOT / g.module.replace(".", "/")) + ".py")
            if not fs_path.exists():
                errors.append(
                    f"MODULE: {gid} module '{g.module}' does not resolve "
                    f"to a file on disk"
                )
    return errors


# ──────────────────────────────────────────────────────────────────────
# MAIN
# ──────────────────────────────────────────────────────────────────────

def main() -> int:
    all_errors: list[str] = []
    checks = [
        ("Filesystem → Catalog", check_filesystem),
        ("catalog.rs → Catalog", check_catalog_rs),
        ("TOML → Catalog", check_toml),
        ("Uniqueness", check_uniqueness),
        ("Module paths", check_module_paths),
    ]

    for name, check_fn in checks:
        errors = check_fn()
        if errors:
            all_errors.extend(errors)
            for e in errors:
                print(f"  FAIL: {e}")
        else:
            print(f"  OK: {name}")

    print()
    if all_errors:
        print(f"G6 CONSISTENCY: FAILED — {len(all_errors)} drift(s) detected")
        print("Fix the drifts above, then re-run: python tools/check_guild_consistency.py")
        return 1
    else:
        print("G6 CONSISTENCY: PASSED — all sources agree with canonical catalog")
        return 0


if __name__ == "__main__":
    sys.exit(main())
