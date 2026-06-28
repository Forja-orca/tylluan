"""
TylluanNexus Code-Graph Guild v1 — Static analysis code topology extractor.

Analyzes Python and Rust code using tree-sitter, extracts structure,
and imports them as code_entities and relations (calls, imports, defines) in SilvaDB.
"""

import os
import sys
import json
import logging
import sqlite3
from pathlib import Path
from typing import Dict, Any, List, Optional, Tuple

from mcp.server.fastmcp import FastMCP
from guilds.core import utils
from guilds.core.silva_utils import add_node, add_edge_direct, get_silva_db_path, ensure_node_exists

mcp = FastMCP("tylluan-code-graph")
logger = logging.getLogger("tylluan-code-graph")

# Initialize tree-sitter
from tree_sitter import Parser, Language
import tree_sitter_python
import tree_sitter_rust

PY_LANG = Language(tree_sitter_python.language())
RUST_LANG = Language(tree_sitter_rust.language())

py_parser = Parser(PY_LANG)
rust_parser = Parser(RUST_LANG)

# ── AST Walkers ────────────────────────────────────────────────────────────────

def get_node_text(bytes_content: bytes, node) -> str:
    return bytes_content[node.start_byte:node.end_byte].decode("utf-8", errors="ignore")

def walk_python_ast(node, bytes_content: bytes, file_id: str, results: Dict[str, Any]):
    """Recursively walk Python AST to find classes, functions, imports and calls."""
    node_type = node.type
    
    if node_type == "class_definition":
        name_node = node.child_by_field_name("name")
        if name_node:
            class_name = get_node_text(bytes_content, name_node)
            class_id = f"class:{file_id}:{class_name}"
            results["classes"].append({
                "id": class_id,
                "name": class_name,
                "line": node.start_point[0] + 1,
                "content": get_node_text(bytes_content, node)[:500]
            })
            # Link file -> defines -> class
            results["relations"].append((file_id, "defines", class_id, {"source": "tree-sitter"}))
            
            # Recurse inside class body
            body_node = node.child_by_field_name("body")
            if body_node:
                for child in body_node.children:
                    if child.type == "function_definition":
                        fn_name_node = child.child_by_field_name("name")
                        if fn_name_node:
                            fn_name = get_node_text(bytes_content, fn_name_node)
                            fn_id = f"method:{class_id}:{fn_name}"
                            results["functions"].append({
                                "id": fn_id,
                                "name": f"{class_name}.{fn_name}",
                                "line": child.start_point[0] + 1,
                                "content": get_node_text(bytes_content, child)[:500]
                            })
                            # Link class -> defines -> function/method
                            results["relations"].append((class_id, "defines", fn_id, {"source": "tree-sitter"}))
        return

    elif node_type == "function_definition":
        name_node = node.child_by_field_name("name")
        if name_node:
            fn_name = get_node_text(bytes_content, name_node)
            fn_id = f"function:{file_id}:{fn_name}"
            results["functions"].append({
                "id": fn_id,
                "name": fn_name,
                "line": node.start_point[0] + 1,
                "content": get_node_text(bytes_content, node)[:500]
            })
            # Link file -> defines -> function
            results["relations"].append((file_id, "defines", fn_id, {"source": "tree-sitter"}))
        return

    elif node_type in ("import_statement", "import_from_statement"):
        import_text = get_node_text(bytes_content, node)
        results["imports"].append({
            "line": node.start_point[0] + 1,
            "text": import_text
        })
        # Try to parse dependencies
        if node_type == "import_from_statement":
            module_node = node.child_by_field_name("module")
            if module_node:
                module_name = get_node_text(bytes_content, module_node)
                target_id = f"module:{module_name}"
                results["relations"].append((file_id, "imports", target_id, {"source": "tree-sitter"}))
        else:
            # Simple import e.g. import os, sys
            for child in node.children:
                if child.type == "dotted_name":
                    name = get_node_text(bytes_content, child)
                    results["relations"].append((file_id, "imports", f"module:{name}", {"source": "tree-sitter"}))

    # Recurse into children
    for child in node.children:
        walk_python_ast(child, bytes_content, file_id, results)


def walk_rust_ast(node, bytes_content: bytes, file_id: str, results: Dict[str, Any]):
    """Recursively walk Rust AST to find structs, enums, traits, functions, and impls."""
    node_type = node.type
    
    if node_type in ("struct_item", "enum_item", "trait_item"):
        name_node = node.child_by_field_name("name")
        if name_node:
            item_name = get_node_text(bytes_content, name_node)
            kind = node_type.replace("_item", "")
            item_id = f"{kind}:{file_id}:{item_name}"
            results["classes"].append({  # group under classes for simplicity
                "id": item_id,
                "name": item_name,
                "line": node.start_point[0] + 1,
                "content": get_node_text(bytes_content, node)[:500]
            })
            # Link file -> defines -> item
            results["relations"].append((file_id, "defines", item_id, {"source": "tree-sitter"}))
        return
        
    elif node_type == "function_item":
        name_node = node.child_by_field_name("name")
        if name_node:
            fn_name = get_node_text(bytes_content, name_node)
            fn_id = f"function:{file_id}:{fn_name}"
            results["functions"].append({
                "id": fn_id,
                "name": fn_name,
                "line": node.start_point[0] + 1,
                "content": get_node_text(bytes_content, node)[:500]
            })
            # Link file -> defines -> function
            results["relations"].append((file_id, "defines", fn_id, {"source": "tree-sitter"}))
        return

    elif node_type == "impl_item":
        # impl Trait for Struct OR impl Struct
        # Find trait and type name
        trait_node = node.child_by_field_name("trait")
        type_node = node.child_by_field_name("type")
        
        type_name = get_node_text(bytes_content, type_node) if type_node else "Unknown"
        trait_name = get_node_text(bytes_content, trait_node) if trait_node else ""
        
        struct_id = f"struct:{file_id}:{type_name}"
        
        body_node = node.child_by_field_name("body")
        if body_node:
            for child in body_node.children:
                if child.type == "function_item":
                    fn_name_node = child.child_by_field_name("name")
                    if fn_name_node:
                        fn_name = get_node_text(bytes_content, fn_name_node)
                        fn_id = f"method:{struct_id}:{fn_name}"
                        results["functions"].append({
                            "id": fn_id,
                            "name": f"{type_name}.{fn_name}",
                            "line": child.start_point[0] + 1,
                            "content": get_node_text(bytes_content, child)[:500]
                        })
                        results["relations"].append((struct_id, "defines", fn_id, {"source": "tree-sitter"}))
                        if trait_name:
                            # Link to trait
                            trait_id = f"trait:{file_id}:{trait_name}"
                            results["relations"].append((fn_id, "implements", trait_id, {"source": "tree-sitter"}))
        return

    elif node_type == "use_declaration":
        use_text = get_node_text(bytes_content, node)
        results["imports"].append({
            "line": node.start_point[0] + 1,
            "text": use_text
        })
        # Try a simple module linkage
        # e.g. use crate::memory::silva::SilvaDB;
        for child in node.children:
            if child.type == "scoped_identifier" or child.type == "identifier":
                name = get_node_text(bytes_content, child)
                results["relations"].append((file_id, "imports", f"module:{name}", {"source": "tree-sitter"}))

    for child in node.children:
        walk_rust_ast(child, bytes_content, file_id, results)


# ── File analyzer ──────────────────────────────────────────────────────────────

def analyze_file_internal(file_path: Path) -> Dict[str, Any]:
    file_path = file_path.resolve()
    ext = file_path.suffix.lower()
    
    if ext not in (".py", ".rs"):
        return {"status": "error", "reason": f"Unsupported extension: {ext}"}
        
    try:
        content_bytes = file_path.read_bytes()
    except Exception as e:
        return {"status": "error", "reason": f"Read error: {e}"}
        
    file_id = f"file:{file_path.name}"
    
    results = {
        "file_id": file_id,
        "classes": [],
        "functions": [],
        "imports": [],
        "relations": []
    }
    
    if ext == ".py":
        tree = py_parser.parse(content_bytes)
        walk_python_ast(tree.root_node, content_bytes, file_id, results)
    elif ext == ".rs":
        tree = rust_parser.parse(content_bytes)
        walk_rust_ast(tree.root_node, content_bytes, file_id, results)
        
    return results


def save_to_silva(results: Dict[str, Any], filepath: Path) -> int:
    """Save extracted code_entities and relations to SilvaDB."""
    nodes_created = 0
    
    # Ensure source file node exists
    file_id = results["file_id"]
    file_meta = {
        "source_path": str(filepath),
        "filename": filepath.name,
        "extension": filepath.suffix.lower(),
        "lines": len(results["classes"]) + len(results["functions"])
    }
    
    # Write file node
    fid = add_node(
        content=f"Code file: {filepath.name}\nPath: {filepath}",
        node_type="code_entity",
        tags=["file", filepath.suffix.lstrip(".")],
        metadata=file_meta
    )
    if fid:
        nodes_created += 1

    # Write class/struct/enum nodes
    for c in results["classes"]:
        cid = add_node(
            content=f"Structure: {c['name']}\nFile: {filepath.name}:{c['line']}\n\n{c['content']}",
            node_type="code_entity",
            tags=["class" if filepath.suffix == ".py" else "structure"],
            metadata={
                "source_path": str(filepath),
                "line": c["line"],
                "name": c["name"]
            }
        )
        if cid:
            nodes_created += 1
            
    # Write function/method nodes
    for f in results["functions"]:
        fn_id = add_node(
            content=f"Function: {f['name']}\nFile: {filepath.name}:{f['line']}\n\n{f['content']}",
            node_type="code_entity",
            tags=["function"],
            metadata={
                "source_path": str(filepath),
                "line": f["line"],
                "name": f["name"]
            }
        )
        if fn_id:
            nodes_created += 1
            
    # Write relationships (edges)
    for src, rel, dest, meta in results["relations"]:
        # Ensure stub nodes exist for destinations like standard modules or unparsed imports
        ensure_node_exists(src, "code_entity")
        ensure_node_exists(dest, "code_entity")
        add_edge_direct(src, rel, dest, meta)
        
    return nodes_created


# ── MCP Tools ──────────────────────────────────────────────────────────────────

@mcp.tool()
async def analyze_file(path: str) -> str:
    """Analyze a single Python (.py) or Rust (.rs) file using tree-sitter,
    and save its class/struct and function definitions to SilvaDB as code_entities.

    Args:
        path: Absolute path to the code file.
    """
    file_path = Path(path).resolve()
    if not file_path.exists():
        return json.dumps({"status": "error", "error": f"File not found: {path}"})
    if not file_path.is_file():
        return json.dumps({"status": "error", "error": f"Not a file: {path}"})
        
    results = analyze_file_internal(file_path)
    if "status" in results and results["status"] == "error":
        return json.dumps(results)
        
    nodes_created = save_to_silva(results, file_path)
    return json.dumps({
        "status": "ok",
        "file": file_path.name,
        "classes_found": len(results["classes"]),
        "functions_found": len(results["functions"]),
        "relations_found": len(results["relations"]),
        "nodes_created": nodes_created
    })


@mcp.tool()
async def analyze_repo(root_path: str, max_files: int = 100) -> str:
    """Recursively search and analyze all Python and Rust files in a repository.

    Args:
        root_path: Path to the repository root.
        max_files: Max files to analyze to prevent resource exhaustion (default 100).
    """
    dir_path = Path(root_path).resolve()
    if not dir_path.exists() or not dir_path.is_dir():
        return json.dumps({"status": "error", "error": f"Not a directory: {root_path}"})
        
    candidates = []
    for root, dirs, files in os.walk(dir_path):
        # Skip ignore dirs
        dirs[:] = [d for d in dirs if d not in (".venv", "venv", "node_modules", "target", ".git", "build", "dist")]
        for file in files:
            p = Path(root) / file
            if p.suffix.lower() in (".py", ".rs"):
                candidates.append(p)
                if len(candidates) >= max_files:
                    break
        if len(candidates) >= max_files:
            break
            
    analyzed, total_nodes, errors = 0, 0, 0
    for file_path in candidates:
        try:
            results = analyze_file_internal(file_path)
            if "status" in results and results["status"] == "error":
                errors += 1
                continue
            nodes = save_to_silva(results, file_path)
            total_nodes += nodes
            analyzed += 1
        except Exception as e:
            logger.error(f"Error analyzing {file_path}: {e}")
            errors += 1
            
    return json.dumps({
        "status": "complete",
        "scanned_files": len(candidates),
        "analyzed_files": analyzed,
        "nodes_created": total_nodes,
        "errors": errors
    })


if __name__ == "__main__":
    utils.safe_mcp_run(mcp)
