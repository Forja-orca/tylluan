"""
TylluanNexus Filesystem Guild — Local file I/O operations.

Provides tools for reading, writing, searching, and listing files
on the local filesystem with safety bounds.
"""

import os
import re
from pathlib import Path

from mcp.server.fastmcp import FastMCP
from guilds.core import utils
from guilds.core._security import SKIP_DIRS, SKIP_FILES, SKIP_EXTENSIONS

mcp = FastMCP("tylluan-filesystem")

MAX_READ_SIZE = 500_000  # 500KB max read
MAX_SEARCH_RESULTS = 50


@mcp.tool()
async def file_read(
    path: str,
    start_line: int | None = None,
    end_line: int | None = None,
    intent: str = "",
) -> str:
    """Read a text file from the local filesystem.

    Args:
        path: Absolute or relative path to the file.
        start_line: Optional first line to read (1-indexed).
        end_line: Optional last line to read (1-indexed, inclusive).
        intent: Natural language intent for fallback line range extraction.

    Returns:
        File contents, optionally filtered by line range.
    """
    # Extract line range from natural language intent if not provided directly
    if (start_line is None or end_line is None) and intent:
        import re as _re
        # "lines 10 to 50" / "lines 10-50" / "line 10 to 50"
        m = _re.search(r'lines?\s+(\d+)\s+(?:to|-|through|hasta)\s+(\d+)', intent, _re.IGNORECASE)
        if m:
            if start_line is None:
                start_line = int(m.group(1))
            if end_line is None:
                end_line = int(m.group(2))
        else:
            # "first N lines" / "primeras N líneas"
            m = _re.search(r'(?:first|primeras?|top)\s+(\d+)\s+lines?', intent, _re.IGNORECASE)
            if m and start_line is None:
                start_line = 1
                end_line = int(m.group(1))
            else:
                # "line N" (single line)
                m = _re.search(r'\blines?\s+(\d+)\b(?!\s+(?:to|-|through))', intent, _re.IGNORECASE)
                if m and start_line is None:
                    start_line = int(m.group(1))
                    end_line = int(m.group(1))

    file_path = Path(path).resolve()

    if not file_path.exists():
        return f"❌ File not found: {file_path}"
    if not file_path.is_file():
        return f"❌ Not a file: {file_path}"

    size = file_path.stat().st_size
    if size > MAX_READ_SIZE:
        return f"⚠️ File too large ({size:,} bytes). Max: {MAX_READ_SIZE:,} bytes. Use line range to read a portion."

    content = file_path.read_text(encoding="utf-8", errors="replace")
    lines = content.splitlines(keepends=True)

    # Auto-truncate large files when no range specified to prevent context overflow
    MAX_AUTO_LINES = 300
    if start_line is None and end_line is None and len(lines) > MAX_AUTO_LINES:
        truncated = lines[:MAX_AUTO_LINES]
        return (
            f"📄 {file_path.name} ({len(lines)} lines total — showing first {MAX_AUTO_LINES})\n"
            f"⚠️ File too large to show fully. Use start_line/end_line or specify in your request.\n"
            + "".join(truncated)
        )

    if start_line is not None or end_line is not None:
        s = max((start_line or 1) - 1, 0)
        e = min(end_line or len(lines), len(lines))
        selected = lines[s:e]
        header = f"📄 {file_path.name} (lines {s + 1}-{e} of {len(lines)})\n"
        return header + "".join(selected)

    return f"📄 {file_path.name} ({len(lines)} lines, {size:,} bytes)\n{content}"


@mcp.tool()
async def file_write(
    path: str = "",
    content: str = "",
    create_dirs: bool = True,
    intent: str = "",
    query: str = "",
) -> str:
    """Write content to a file, creating parent directories if needed. [approval="always"]

    Args:
        path: Absolute or relative path to the file.
        content: The text content to write.
        create_dirs: Whether to create parent directories (default: True).
        intent: Natural language intent for fallback path/content extraction.
        query: Alternative query string for content extraction.

    Returns:
        Confirmation with bytes written.
    """
    # Extract path if not explicitly provided
    if not path:
        import re as _re
        m = _re.search(r'(?:file|archivo|fichero)\s+["\']?([^\s"\']+\.\w+)', intent or query or "", _re.IGNORECASE)
        if not m:
            m = _re.search(r'(?:in|to|at)\s+(\S+)', intent or query or "")
        path = m.group(1) if m else ""
    
    # Extract content if not explicitly provided
    if not content:
        import re as _re
        m = _re.search(
            r'(?:with text|content|texto|containing|:)\s*[:\-]?\s*(.+)$',
            intent or query or "", _re.IGNORECASE | _re.DOTALL
        )
        content = m.group(1).strip().strip('"\'') if m else ""

    if not path or not content:
        return "❌ Missing path or content. Example: 'write file hello.txt with text: hello world'"
    
    file_path = Path(path).resolve()

    # Safety guard: prevent accidental overwrites of existing non-empty files
    if file_path.exists() and file_path.stat().st_size > 0:
        safe_words = ["overwrite", "replace", "update", "write to", "save", "sobrescribir", "reemplazar", "actualizar"]
        if not any(w in intent.lower() or w in query.lower() for w in safe_words):
            return f"⚠️ File {path} already exists ({file_path.stat().st_size} bytes). Include 'overwrite' in your request to confirm, or set create_dirs=True to proceed."

    if create_dirs:
        file_path.parent.mkdir(parents=True, exist_ok=True)

    try:
        file_path.write_text(content, encoding="utf-8")
        return f"✅ Written {len(content):,} bytes to {file_path}"
    except Exception as e:
        return f"❌ Failed to write to {file_path}: {e}"


@mcp.tool()
async def find_files(
    pattern: str = "",
    directory: str = ".",
    intent: str = "",
    query: str = "",
    max_results: int = MAX_SEARCH_RESULTS,
) -> str:
    """Find files by name pattern or extension. Use this to LOCATE files, not search their contents.

    Use for: find all .py files, find files with extension, locate files by name,
    files matching pattern, find *.toml files, find *.rs files, find typescript files,
    find rust source files, show all json files, buscar archivos por extensión,
    encontrar ficheros con extensión.

    For listing a directory tree use file_list. For searching INSIDE file contents use file_search.

    Args:
        pattern: Glob pattern like '*.py', '*.toml', or a filename
        directory: Directory to search in (default: current)
        intent: Natural language intent for pattern extraction
        query: Alternative query string
        max_results: Maximum files to return
    """
    # Words that look like they precede "files" in English but are NOT extensions
    _NON_EXT = frozenset({
        "list", "all", "the", "any", "some", "my", "your", "our", "new",
        "old", "large", "small", "recent", "local", "current", "other",
        "what", "which", "show", "find", "get", "read", "open",
    })
    if not pattern:
        source = intent or query or ""
        import re as _re
        m = _re.search(r'["\']([^"\']+)["\']', source)
        if m:
            pattern = m.group(1)
        else:
            m = _re.search(r'(\*\.[a-zA-Z0-9]+)', source)
            if m:
                pattern = m.group(1)
            else:
                m = _re.search(r'\.([a-zA-Z0-9]+)\s*(?:files?|archivos?)?', source, _re.IGNORECASE)
                if m:
                    pattern = f"*.{m.group(1)}"
                else:
                    ext_map = {"python": "py", "rust": "rs", "typescript": "ts",
                               "javascript": "js", "toml": "toml", "yaml": "yaml",
                               "markdown": "md", "go": "go", "java": "java"}
                    m = _re.search(r'\b([a-zA-Z]+)\s+files?', source, _re.IGNORECASE)
                    if m:
                        word = m.group(1).lower()
                        if word not in _NON_EXT:
                            pattern = f"*.{ext_map.get(word, word)}"
                        else:
                            pattern = "*"
                    else:
                        pattern = "*"

    try:
        root = Path(directory).resolve()
        if not root.is_dir():
            return f"❌ Directory not found: {directory}"

        files: list[str] = []
        for f in root.rglob(pattern):
            if not f.is_file():
                continue
            if any(part in SKIP_DIRS for part in f.relative_to(root).parts):
                continue
            try:
                files.append(str(f.relative_to(root)))
            except ValueError:
                files.append(str(f))
            if len(files) >= max_results:
                break

        if not files:
            return f"🔍 No files found matching '{pattern}' in {root}"
        return f"📁 Found {len(files)} files matching '{pattern}':\n\n" + "\n".join(files[:max_results])
    except Exception as e:
        return f"❌ Find failed: {e}"


# Security constants centralised in guilds/core/_security.py
_FS_SKIP_DIRS = SKIP_DIRS
_FS_SKIP_NAMES = SKIP_FILES
_FS_SKIP_EXTS = SKIP_EXTENSIONS


@mcp.tool()
async def file_search(
    directory: str,
    pattern: str = "",
    intent: str = "",
    query: str = "",
    file_glob: str = "*",
    max_results: int = MAX_SEARCH_RESULTS,
) -> str:
    """Search for a text pattern INSIDE file contents (recursive grep).

    Use for: search for text, grep for pattern, find string in code, look for TODO,
    find occurrences of word, search inside files, content search, find in files.

    For finding files by NAME or EXTENSION, use find_files instead.

    Args:
        directory: Directory to search in.
        pattern: Text or regex pattern to find inside files.
        intent: Natural language intent for fallback pattern extraction.
        query: Alternative query string for pattern extraction.
        file_glob: Glob pattern for files to include (e.g., '*.py', '*.ts').
        max_results: Maximum number of matches to return.

    Returns:
        Matching lines with file paths and line numbers.
    """
    if not pattern:
        import re as _re
        m = _re.search(r'["\']([^"\']+)["\']', intent or query or "")
        if m:
            pattern = m.group(1)
        else:
            m = _re.search(r'(\*\.[a-zA-Z0-9]+|\*)', intent or query or "")
            if m:
                pattern = m.group(1)
                if file_glob == "*":
                    file_glob = pattern
                pattern = "."
            else:
                stopwords = {"search", "find", "for", "the", "in", "all", "files",
                             "content", "pattern", "grep", "look", "inside"}
                words = [w for w in (intent or query or "").split()
                         if w.lower() not in stopwords and len(w) > 2]
                pattern = words[0] if words else "."

    directory = os.path.abspath(directory) if directory else os.path.abspath(".")
    dir_path = Path(directory).resolve()
    if not dir_path.is_dir():
        return f"❌ Not a directory: {dir_path}"

    try:
        regex = re.compile(pattern, re.IGNORECASE)
    except re.error as e:
        return f"❌ Invalid regex: {e}"

    results: list[str] = []
    files_searched = 0

    for file_path in dir_path.rglob(file_glob):
        if not file_path.is_file():
            continue
        rel = file_path.relative_to(dir_path)
        # Skip sensitive directories
        if any(part in _FS_SKIP_DIRS for part in rel.parts):
            continue
        # Skip sensitive files
        if file_path.name in _FS_SKIP_NAMES:
            continue
        if file_path.suffix.lower() in _FS_SKIP_EXTS:
            continue
        if file_path.name.startswith(".") and file_path.name not in (".gitignore", ".dockerignore"):
            continue
        if file_path.stat().st_size > MAX_READ_SIZE:
            continue

        files_searched += 1
        try:
            content = file_path.read_text(encoding="utf-8", errors="replace")
            for i, line in enumerate(content.splitlines(), 1):
                if regex.search(line):
                    results.append(f"{rel}:{i}: {line.strip()}")
                    if len(results) >= max_results:
                        break
        except (PermissionError, OSError):
            continue

        if len(results) >= max_results:
            break

    if not results:
        return f"🔍 No matches for '{pattern}' in {files_searched} files."

    return f"🔍 {len(results)} matches in {files_searched} files:\n\n" + "\n".join(results)


@mcp.tool()
async def file_list(directory: str, depth: int = 2) -> str:
    """List files and directories with a tree-like format.

    Args:
        directory: Directory to list.
        depth: Maximum depth to recurse (default: 2).

    Returns:
        Tree-formatted directory listing.
    """
    dir_path = Path(directory).resolve()
    if not dir_path.is_dir():
        return f"❌ Not a directory: {dir_path}"

    lines: list[str] = [f"📁 {dir_path}/"]
    _tree(dir_path, lines, depth=depth, prefix="")

    return "\n".join(lines)


def _tree(path: Path, lines: list[str], depth: int, prefix: str) -> None:
    if depth <= 0:
        return

    try:
        entries = sorted(path.iterdir(), key=lambda e: (not e.is_dir(), e.name.lower()))
    except PermissionError:
        lines.append(f"{prefix}├── ⚠️ Permission denied")
        return

    # BUG-05: Filter noise before iterating to ensure correct connectors (├──/└──)
    filtered = [e for e in entries if not (
        (e.is_dir() and e.name in _FS_SKIP_DIRS) or
        (e.is_file() and e.name in _FS_SKIP_NAMES)
    )]

    for i, entry in enumerate(filtered):
        is_last = i == len(filtered) - 1
        connector = "└── " if is_last else "├── "
        icon = "📁" if entry.is_dir() else "📄"
        size = f" ({entry.stat().st_size:,}B)" if entry.is_file() else ""
        lines.append(f"{prefix}{connector}{icon} {entry.name}{size}")

        if entry.is_dir():
            extension = "    " if is_last else "│   "
            _tree(entry, lines, depth - 1, prefix + extension)

# SECURITY: file_search MUST skip .env, *.key, *.pem — verified 2026-05-04
# _FS_SKIP_NAMES and _FS_SKIP_EXTS are module-level constants (lines 224-226)

if __name__ == "__main__":
    utils.safe_mcp_run(mcp)
