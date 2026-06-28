"""
TylluanNexus Deep Analysis Guild — Advanced project-wide code understanding.

This guild provides:
    - Project-wide dependency mapping
    - Impact analysis (blast radius)
    - Semantic logic clones detection (via SilvaDB)
    - Dead code hunting
"""

import os
import re
import ast
from pathlib import Path
from typing import Dict, List, Set, Optional
from collections import defaultdict

from mcp.server.fastmcp import FastMCP
from guilds.core import utils

mcp = FastMCP("tylluan-deep-analysis")

# Cache for the project graph
# { file_path: { imports: set(), dependents: set() } }
_project_graph = defaultdict(lambda: {"imports": set(), "dependents": set()})

def _get_project_root() -> Path:
    """Return the absolute path to the project root.

    Uses __file__ to find the repo root (guilds/core/ → repo root).
    Portable across platforms.
    """
    return Path(__file__).parent.parent.parent.resolve()

@mcp.tool()
async def sovereign_structural_mapper(refresh: bool = False) -> str:
    """Scan the entire project and build a dependency map.
    
    Args:
        refresh: Whether to force a full re-scan.
    
    Returns:
        Summary of the project structure and hotspots.
    """
    global _project_graph
    if not refresh and _project_graph:
        return f"📊 Using cached graph: {len(_project_graph)} files indexed."

    # Invalidate cache if contaminated with site-packages from a previous bad scan
    if any("site-packages" in k for k in list(_project_graph.keys())[:5]):
        _project_graph.clear()

    _project_graph.clear()
    root = _get_project_root()

    # 1. Discover files (Robust walk)
    code_files = []
    extensions = [".py", ".js", ".ts", ".rs"]
    ignore_dirs = {
        ".venv", "venv", "env", ".env",          # virtual envs (all naming styles)
        "node_modules", "target", "dist", "build", ".next",
        ".git", "__pycache__", ".cache", ".mypy_cache",
        "site-packages", "Lib",                  # safety net for oddly-named venvs
    }
    ignore_dirs_lower = {d.lower() for d in ignore_dirs}

    for dirpath, dirnames, filenames in os.walk(root):
        # Skip ignored directories (case-insensitive, pyvenv.cfg detection)
        dirnames[:] = [
            d for d in dirnames
            if d.lower() not in ignore_dirs_lower
            and not (Path(dirpath) / d / "pyvenv.cfg").exists()
        ]

        for filename in filenames:
            if any(filename.endswith(ext) for ext in extensions):
                code_files.append(Path(dirpath) / filename)

    # 2. Parse dependencies
    for file_path in code_files:
        rel_path = str(file_path.relative_to(root))
        lang = utils.detect_language(file_path.name)
        
        try:
            content = file_path.read_text(encoding="utf-8", errors="ignore")
            if lang == "Python":
                imports = _parse_python_imports(content)
            else:
                imports = _parse_generic_imports(content)
            
            for imp in imports:
                # Try to resolve import to a local file
                target_path = _resolve_import(imp, file_path, root)
                if target_path:
                    _project_graph[rel_path]["imports"].add(target_path)
                    _project_graph[target_path]["dependents"].add(rel_path)
        except Exception:
            continue

    # 3. Analyze hotspots
    hotspots = sorted(_project_graph.items(), key=lambda x: len(x[1]["dependents"]), reverse=True)
    
    summary = [
        f"📊 Project Map Complete",
        f"Files indexed: {len(code_files)}",
        f"Relationships found: {sum(len(v['imports']) for v in _project_graph.values())}",
        f"\n🔥 Top Hotspots (Most depended upon):"
    ]
    
    for path, data in hotspots[:10]:
        summary.append(f"   • {path} ({len(data['dependents'])} dependents)")
        
    return "\n".join(summary)

def _parse_python_imports(content: str) -> Set[str]:
    """Extract imports from Python source using AST."""
    imports = set()
    try:
        tree = ast.parse(content)
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                for alias in node.names:
                    imports.add(alias.name)
            elif isinstance(node, ast.ImportFrom):
                imports.add(node.module or "")
    except SyntaxError:
        pass
    return imports

def _parse_generic_imports(content: str) -> Set[str]:
    """Extract imports using regex (for JS/TS/Rust)."""
    imports = set()
    # Basic regex for JS/TS: import ... from 'path'
    js_pattern = re.compile(r"from\s+['\"]([^'\"]+)['\"]")
    for match in js_pattern.finditer(content):
        imports.add(match.group(1))
    
    # Rust: use crate::...
    rs_pattern = re.compile(r"use\s+([^;:]+)")
    for match in rs_pattern.finditer(content):
        imports.add(match.group(1))
        
    return imports

def _resolve_import(imp_name: str, current_file: Path, root: Path) -> Optional[str]:
    """Attempt to resolve an import name to a project-relative path."""
    # This is a simplified resolver. A real one would need to handle
    # sys.path, package structures, etc.
    
    # Try as relative path
    potential_rel = (current_file.parent / imp_name).with_suffix(".py")
    if potential_rel.exists():
        return str(potential_rel.relative_to(root))
        
    # Try as absolute from root (e.g., "guilds.core.utils" -> "guilds/core/utils.py")
    parts = imp_name.split(".")
    potential_abs = root.joinpath(*parts).with_suffix(".py")
    if potential_abs.exists():
        return str(potential_abs.relative_to(root))
        
    # TODO: Add JS/TS resolution (node_modules, aliases)
    
    return None

@mcp.tool()
async def get_impact_radius(file_path: str) -> str:
    """Find all files that depend on the given file (recursively).
    
    Args:
        file_path: Project-relative path to the file (e.g., 'guilds/core/utils.py').
    """
    if not _project_graph:
        await sovereign_structural_mapper()
        
    root_str = str(_get_project_root())
    if file_path not in _project_graph and f"{root_str}/{file_path}" not in _project_graph:
        # Try to normalize
        file_path = file_path.replace("\\", "/")
    
    visited = set()
    to_visit = [file_path]
    
    while to_visit:
        curr = to_visit.pop()
        if curr not in visited:
            visited.add(curr)
            if curr in _project_graph:
                to_visit.extend(_project_graph[curr]["dependents"])
                
    visited.remove(file_path) # Don't count itself
    
    if not visited:
        return f"✅ Zero impact radius: No other files depend on '{file_path}'."
        
    result = [f"⚠️ Impact Radius for '{file_path}': {len(visited)} files affected."]
    for path in sorted(visited):
        result.append(f"   • {path}")
        
    return "\n".join(result)

if __name__ == "__main__":
    utils.safe_mcp_run(mcp)
