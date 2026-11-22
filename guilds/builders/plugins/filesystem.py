"""
Filesystem guild: operations on files and directories given a path or route.
file_list lists a directory tree (crates/, docs/, src/, data/, E:/... any ruta);
file_read reads a file's lines; find_files finds files by name or extension;
file_search greps text inside files; file_write saves a file (needs approval).
Listar el contenido de una carpeta, leer un fichero, buscar ficheros por nombre o ruta.
"""

import os
import re
from pathlib import Path

from mcp.server.fastmcp import FastMCP
from guilds.core import utils
from guilds.core._security import SKIP_DIRS, SKIP_FILES, SKIP_EXTENSIONS
from guilds.builders.plugins.utils import validate_path
from guilds.core._root import raiz_o_deniega

mcp = FastMCP("tylluan-filesystem")

MAX_READ_SIZE = 500_000  # 500KB max read
MAX_SEARCH_RESULTS = 50

# Default root — kernel CWD, overridable via env var
def _raiz() -> str | None:
    """Raiz permitida resuelta EN CADA LLAMADA (no al importar).

    Si el fallo cerrado esta activo y no hay raiz declarada, devuelve None y las tools
    responden BLOCKED: asi el guild sigue vivo en vez de quedarse caido (guild_down).
    """
    raiz, _error = raiz_o_deniega()
    return raiz


def _denegado() -> str:
    """Mensaje de BLOCKED cuando no hay raiz utilizable."""
    _raiz_ok, error = raiz_o_deniega()
    return f"🚫 {error}" if error else "🚫 BLOCKED: no hay raiz permitida configurada."

# ─── FS-FIND-FILES-IGNORA-RUTA-DEL-INTENT (encargo TL T4147) ────────────────
# A natural-language intent can name the very directory that has to be listed
# ("lista los ficheros de E:\tylluan\docs"). `find_files` used to look only at
# the `directory` parameter (default "."), so the path in the intent was
# dropped and the guild listed the kernel's CWD instead — in production that is
# the Tylluan root, and the listing exposed `data/`, `bin/` and the NAMES of
# `.tylluan-token` and `identity.key` (Hermes-Linux report, T4147). These three
# helpers carry the extraction; see `_directory_from_text`.
_PATH_TOKEN_RE = re.compile(
    r"(?:(?<![A-Za-z0-9])[A-Za-z]:[\\/][^\s\"'`]*"  # C:\ruta, D:/ruta (nunca `https:`)
    r"|~[\\/][^\s\"'`]*"  # ~/ruta
    r"|[\\/]{2}[^\s\"'`\\/]+(?:[\\/][^\s\"`]*)*"  # \\host\share, //host/share
    r"|/[A-Za-z0-9._][^\s\"'`]*)"  # /ruta/absoluta
)
# Trailing prose punctuation must not become part of the path.
_PATH_TRAILING = ".,;:!?)]}'\"`"


def _directory_from_text(source: str) -> str | None:
    """Return the absolute path named in `source`, or None.

    Purely lexical: it never touches the disk. A UNC path is returned as-is on
    purpose so that `validate_path` rejects it with a clear message (B-32/H2)
    instead of paying a name resolution for it.
    """
    if not source:
        return None
    for m in _PATH_TOKEN_RE.finditer(source):
        prev = source[m.start() - 1] if m.start() else ""
        if prev in ":/\\":  # `http://host/x`, or the tail of a longer token
            continue
        candidate = m.group(0).rstrip(_PATH_TRAILING)
        if len(candidate) >= 2:
            return candidate
    return None


def _strip_path_tokens(source: str) -> str:
    """Remove paths from the text so they cannot be mistaken for a pattern.

    Without this, `C:\\Users\\x\\.cache\\tylluan` reached the extension regex in
    `find_files` and produced the pattern `*.cache`.
    """
    return _PATH_TOKEN_RE.sub(" ", source)


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
    raiz = _raiz()
    if raiz is None:
        return _denegado()
    if not validate_path(raiz, str(file_path)):
        return f"🚫 BLOCKED: Path '{file_path}' is outside the allowed root '{raiz}'."
    # SEGURIDAD (Deep, 2026-11-21): `file_search` ya filtraba con los SKIP compartidos de
    # `guilds/core/_security.py`, pero `file_read` NO: su unica barrera era `validate_path`,
    # cuya raiz cae a `os.getcwd()` -- en produccion, la raiz del repo. Con eso, `.tylluan-token`
    # (33 B, clase OPERADOR), `.env`, `id_rsa` y `data/*.db` se devolvian: un `tylluan_do`
    # ("lee el fichero .tylluan-token") bastaba para exfiltrar el token maestro.
    # Se reutiliza la MISMA lista que usa `file_search`, no una copia.
    if file_path.name in _FS_SKIP_NAMES or file_path.suffix.lower() in _FS_SKIP_EXTS:
        return f"🚫 BLOCKED: '{file_path.name}' es un fichero sensible (token, clave o base de datos); no se lee."

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
    raiz = _raiz()
    if raiz is None:
        return _denegado()
    if not validate_path(raiz, str(file_path)):
        return f"🚫 BLOCKED: Path '{file_path}' is outside the allowed root '{raiz}'."

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
        directory: Directory to search in. If omitted, an absolute path named in
            `intent`/`query` is used; it is never silently replaced by the
            kernel's working directory.
        intent: Natural language intent for pattern and directory extraction
        query: Alternative query string
        max_results: Maximum files to return
    """
    text = intent or query or ""
    # A path taken from the request only counts when the caller did not pass an
    # explicit `directory` — an explicit parameter is the caller's own choice.
    explicit_dir = bool(directory and directory.strip() not in (".", ""))
    named_dir: str | None = None
    if not explicit_dir:
        named_dir = _directory_from_text(text)
        if named_dir:
            directory = named_dir

    # Words that look like they precede "files" in English but are NOT extensions
    _NON_EXT = frozenset({
        "list", "all", "the", "any", "some", "my", "your", "our", "new",
        "old", "large", "small", "recent", "local", "current", "other",
        "what", "which", "show", "find", "get", "read", "open",
    })
    if not pattern:
        # A path named in the request must not be read again as a glob or
        # extension: the old chain turned `C:\Users\x\.cache` into `*.cache` and
        # `https://host/docs` into `*.com` (see `_strip_path_tokens`).
        source = _strip_path_tokens(text)
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
        named = " (named in your request)" if named_dir else ""
        raiz = _raiz()
        if raiz is None:
            return _denegado()
        if not validate_path(raiz, str(root)):
            return (
                f"🚫 BLOCKED: Directory '{directory}'{named} is outside the allowed root "
                f"'{raiz}'. The search did NOT fall back to the kernel's working "
                f"directory — name a path inside the allowed root."
            )
        if not root.is_dir():
            return (
                f"❌ Directory not found: {directory}{named}. It was not replaced by the "
                f"kernel's working directory ('{raiz}')."
            )

        files: list[str] = []
        for f in root.rglob(pattern):
            if not f.is_file():
                continue
            if any(part in SKIP_DIRS for part in f.relative_to(root).parts):
                continue
            # H-HER-M-06 (Hermes, 2026-11-22): `find_files` solo filtraba DIRECTORIOS, asi que
            # devolvia los NOMBRES de `.tylluan-token` y `.env` (18 llamadas ok, 26-jul a 22-oct en
            # guild_audit.db). `file_search` y `file_list` ya filtraban nombres; esto las iguala.
            if f.name in SKIP_FILES or f.suffix.lower() in SKIP_EXTENSIONS:
                continue
            try:
                files.append(str(f.relative_to(root)))
            except ValueError:
                files.append(str(f))
            if len(files) >= max_results:
                break

        if not files:
            return f"🔍 No files found matching '{pattern}' in {root}"
        # The searched directory is named in the header: a listing of the kernel's
        # working directory must never be an unlabelled surprise.
        return (
            f"📁 Found {len(files)} files matching '{pattern}' in {root}:\n\n"
            + "\n".join(files[:max_results])
        )
    except Exception as e:
        return f"❌ Find failed: {e}"


# Security constants centralised in guilds/core/_security.py
_FS_SKIP_DIRS = SKIP_DIRS
_FS_SKIP_NAMES = SKIP_FILES
_FS_SKIP_EXTS = SKIP_EXTENSIONS


@mcp.tool()
async def file_search(
    directory: str = ".",
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
        directory: Directory to search in. If omitted, an absolute path named in
            `intent`/`query` is used; it is never silently replaced by the
            kernel's working directory.
        pattern: Text or regex pattern to find inside files.
        intent: Natural language intent for fallback pattern and directory extraction.
        query: Alternative query string for pattern extraction.
        file_glob: Glob pattern for files to include (e.g., '*.py', '*.ts').
        max_results: Maximum number of matches to return.

    Returns:
        Matching lines with file paths and line numbers.
    """
    # FS-FIND-FILES-IGNORA-RUTA-DEL-INTENT, parte `file_search` (propuesta (c) de
    # mi nota T4190, en marcha tras «sigue con tu cola» del TL en T4210; el hueco
    # lo señalo Antigravity en T4182). Dos defectos: `directory` era obligatorio
    # (un intent que solo nombra la ruta ni ejecutaba la tool) y con "." se busca
    # en el CWD del kernel en silencio. Misma regla que `find_files`/`file_list`.
    text = intent or query or ""
    explicit_dir = bool(directory and directory.strip() not in (".", ""))
    named_dir: str | None = None
    if not explicit_dir:
        named_dir = _directory_from_text(text)
        if named_dir:
            directory = named_dir

    if not pattern:
        # La ruta ya consumida como `directory` no debe volver a leerse como glob
        # ni como patron (ver `_strip_path_tokens`).
        source = _strip_path_tokens(text)
        import re as _re
        m = _re.search(r'["\']([^"\']+)["\']', source)
        if m:
            pattern = m.group(1)
        else:
            m = _re.search(r'(\*\.[a-zA-Z0-9]+|\*)', source)
            if m:
                pattern = m.group(1)
                if file_glob == "*":
                    file_glob = pattern
                pattern = "."
            else:
                stopwords = {"search", "find", "for", "the", "in", "all", "files",
                             "content", "pattern", "grep", "look", "inside"}
                words = [w for w in source.split()
                         if w.lower() not in stopwords and len(w) > 2]
                pattern = words[0] if words else "."

    directory = os.path.abspath(directory) if directory else os.path.abspath(".")
    dir_path = Path(directory).resolve()
    named = " (named in your request)" if named_dir else ""
    raiz = _raiz()
    if raiz is None:
        return _denegado()
    if not validate_path(raiz, str(dir_path)):
        return (
            f"🚫 BLOCKED: Directory '{directory}'{named} is outside the allowed root "
            f"'{raiz}'. The search did NOT fall back to the kernel's working "
            f"directory — name a path inside the allowed root."
        )
    if not dir_path.is_dir():
        return (
            f"❌ Not a directory: {directory}{named}. It was not replaced by the "
            f"kernel's working directory ('{raiz}')."
        )

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
        return f"🔍 No matches for '{pattern}' in {files_searched} files under {dir_path}."

    # El directorio buscado sale en la cabecera: una busqueda en el cwd del
    # kernel nunca debe ser una sorpresa para quien pregunta en lenguaje llano.
    return (
        f"🔍 {len(results)} matches in {files_searched} files under {dir_path}:\n\n"
        + "\n".join(results)
    )


@mcp.tool()
async def file_list(
    directory: str = ".",
    depth: int = 2,
    intent: str = "",
    query: str = "",
) -> str:
    """List files and directories with a tree-like format.

    Args:
        directory: Directory to list. If omitted, an absolute path named in
            `intent`/`query` is used; it is never silently replaced by the
            kernel's working directory.
        depth: Maximum depth to recurse (default: 2).
        intent: Natural language intent; it may name the directory to list.
        query: Alternative query string.

    Returns:
        Tree-formatted directory listing.
    """
    # FS-FIND-FILES-IGNORA-RUTA-DEL-INTENT, parte `file_list` (encargo TL T4183).
    # Hasta aqui `directory` era obligatorio y no existia `intent`: un
    # `tylluan_do` que solo nombra la ruta en el texto reventaba con
    # «Field required» (Hermes-Linux) y, si el llamante mandaba ".", el arbol
    # salia del CWD del kernel. Misma regla que en `find_files`: un
    # `directory` explicito manda; si no, se usa la ruta nombrada en el texto;
    # y si esa ruta no vale, se dice, no se cae al cwd en silencio.
    text = intent or query or ""
    explicit_dir = bool(directory and directory.strip() not in (".", ""))
    named_dir: str | None = None
    if not explicit_dir:
        named_dir = _directory_from_text(text)
        if named_dir:
            directory = named_dir

    dir_path = Path(directory).resolve()
    named = " (named in your request)" if named_dir else ""
    raiz = _raiz()
    if raiz is None:
        return _denegado()
    if not validate_path(raiz, str(dir_path)):
        return (
            f"🚫 BLOCKED: Directory '{directory}'{named} is outside the allowed root "
            f"'{raiz}'. The listing did NOT fall back to the kernel's working "
            f"directory — name a path inside the allowed root."
        )
    if not dir_path.is_dir():
        return (
            f"❌ Not a directory: {directory}{named}. It was not replaced by the "
            f"kernel's working directory ('{raiz}')."
        )

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
