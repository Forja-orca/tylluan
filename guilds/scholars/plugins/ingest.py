"""
TylluanNexus Ingest Guild — File ingestion pipeline.

Processes files and creates nodes in SilvaDB.
Supports: text, code, config, data formats. PDF requires pdfplumber (optional).
"""

import os
import sys
import json
from pathlib import Path
from typing import Optional, Dict, Any, List
from mcp.server.fastmcp import FastMCP
from guilds.core import utils
from guilds.core.silva_utils import add_node

mcp = FastMCP("tylluan-ingest")

MAX_FILE_SIZE = 10 * 1024 * 1024  # 10 MB per file
ALLOWED_EXTENSIONS = {
    ".md", ".txt", ".py", ".js", ".ts", ".tsx", ".jsx",
    ".rs", ".go", ".java", ".c", ".cpp", ".h",
    ".json", ".yaml", ".yml", ".toml", ".xml",
    ".html", ".css", ".sh", ".bat", ".ps1",
    ".sql", ".csv", ".log", ".pdf",
}
IGNORE_DIRS = {".venv", "node_modules", "target", ".git", "__pycache__", ".pytest_cache"}


def _extract_text(file_path: Path) -> tuple[str, Dict[str, Any]]:
    """Read file content and return (text, metadata)."""
    meta: Dict[str, Any] = {
        "filename": file_path.name,
        "extension": file_path.suffix.lower(),
        "size_bytes": file_path.stat().st_size,
        "parent_dir": file_path.parent.name,
    }

    ext = file_path.suffix.lower()

    # PDF — requires pdfplumber
    if ext == ".pdf":
        try:
            import pdfplumber
            with pdfplumber.open(file_path) as pdf:
                text = "\n".join(
                    page.extract_text() or "" for page in pdf.pages
                )
            meta["pages"] = len(pdf.pages)
            return text, meta
        except ImportError:
            return "[PDF: install pdfplumber for PDF support]", meta
        except Exception as e:
            return f"[PDF read error: {e}]", meta

    # All text-based formats
    for enc in ("utf-8", "latin-1"):
        try:
            content = file_path.read_text(encoding=enc)
            if enc != "utf-8":
                meta["encoding"] = enc
            return content, meta
        except UnicodeDecodeError:
            continue
        except Exception as e:
            return f"[Read error: {e}]", meta

    return "[Binary file — could not decode]", meta


def _is_allowed(file_path: Path) -> bool:
    return file_path.suffix.lower() in ALLOWED_EXTENSIONS


def _should_ignore(file_path: Path) -> bool:
    return any(part in IGNORE_DIRS for part in file_path.parts)


@mcp.tool()
async def ingest_file(path: str, create_node: bool = True) -> str:
    """Ingest a single file into SilvaDB as a document node.

    Args:
        path: Absolute path to the file.
        create_node: Whether to store in SilvaDB (default True).

    Returns:
        JSON with status, metadata, and node_id if created.
    """
    file_path = Path(path).resolve()

    if not file_path.exists():
        return json.dumps({"status": "error", "error": f"File not found: {path}"})
    if not file_path.is_file():
        return json.dumps({"status": "error", "error": f"Not a file: {path}"})
    if not _is_allowed(file_path):
        return json.dumps({"status": "error", "error": f"Type not supported: {file_path.suffix}"})
    if file_path.stat().st_size > MAX_FILE_SIZE:
        return json.dumps({"status": "error", "error": f"File too large ({file_path.stat().st_size} bytes, max {MAX_FILE_SIZE})"})
    if _should_ignore(file_path):
        return json.dumps({"status": "skipped", "reason": "Path is in ignore list"})

    content, meta = _extract_text(file_path)
    meta["preview"] = content[:500]
    meta["line_count"] = content.count("\n") + 1

    node_id: Optional[str] = None
    if create_node:
        node_id = add_node(
            content=content,
            node_type="document",
            tags=[file_path.suffix[1:], file_path.parent.name],
            metadata=meta,
        )

    return json.dumps({
        "status": "success",
        "path": str(file_path),
        "content_length": len(content),
        "node_id": node_id,
        "metadata": meta,
    })


@mcp.tool()
async def ingest_directory(
    directory: str,
    recursive: bool = True,
    max_files: int = 100,
    create_nodes: bool = True,
) -> str:
    """Ingest all supported files from a directory into SilvaDB.

    Args:
        directory: Path to the directory.
        recursive: Process subdirectories (default True).
        max_files: Hard cap on files processed (default 100).
        create_nodes: Store each file as a SilvaDB node.

    Returns:
        JSON summary with counts and per-file results.
    """
    dir_path = Path(directory).resolve()
    if not dir_path.exists() or not dir_path.is_dir():
        return json.dumps({"status": "error", "error": f"Directory not found: {directory}"})

    glob = dir_path.rglob("*") if recursive else dir_path.iterdir()
    candidates = [
        p for p in glob
        if p.is_file() and _is_allowed(p) and not _should_ignore(p)
    ][:max_files]

    results = []
    for file_path in candidates:
        try:
            content, meta = _extract_text(file_path)
            meta["preview"] = content[:300]
            node_id: Optional[str] = None
            if create_nodes:
                node_id = add_node(
                    content=content,
                    node_type="document",
                    tags=[file_path.suffix[1:], file_path.parent.name],
                    metadata=meta,
                )
            results.append({
                "status": "ok",
                "path": str(file_path.relative_to(dir_path)),
                "size": meta["size_bytes"],
                "node_id": node_id,
            })
        except Exception as e:
            results.append({
                "status": "error",
                "path": str(file_path.relative_to(dir_path)),
                "error": str(e),
            })

    ok = sum(1 for r in results if r["status"] == "ok")
    errors = len(results) - ok

    return json.dumps({
        "status": "complete",
        "directory": str(dir_path),
        "found": len(candidates),
        "ok": ok,
        "errors": errors,
        "files": results,
    })


@mcp.tool()
async def list_allowed_types() -> str:
    """List supported file extensions and ingest limits."""
    return json.dumps({
        "allowed_extensions": sorted(ALLOWED_EXTENSIONS),
        "ignore_directories": sorted(IGNORE_DIRS),
        "max_file_size_mb": MAX_FILE_SIZE // (1024 * 1024),
    })


if __name__ == "__main__":
    utils.safe_mcp_run(mcp)
