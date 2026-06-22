import logging
import re
from pathlib import Path

from mcp.server.fastmcp import FastMCP

from guilds.core import utils

mcp = FastMCP("tylluan-ast-surgeon")


def _read_file(path: str) -> str | None:
    fp = Path(path).resolve()
    if not fp.exists():
        return None
    try:
        return fp.read_text(encoding="utf-8")
    except Exception:
        return None


def _write_file(path: str, content: str) -> bool:
    try:
        Path(path).write_text(content, encoding="utf-8")
        return True
    except Exception:
        return False


_TS_KEYWORDS = {
    "class", "function", "interface", "type", "enum", "const", "let", "var",
    "import", "export", "async", "await", "return", "if", "else", "for",
    "while", "do", "switch", "case", "default", "try", "catch", "finally",
    "throw", "new", "this", "super", "extends", "implements", "typeof",
    "keyof", "readonly", "static", "public", "private", "protected",
    "abstract", "declare", "namespace", "module", "from", "of", "in",
    "as", "is", "satisfies", "asserts", "any", "unknown", "never", "void",
    "undefined", "null", "boolean", "number", "string", "symbol", "bigint",
    "true", "false",
}


@mcp.tool()
async def ast_get_file_outline(file_path: str) -> str:
    """Returns a structural syntax outline of a TypeScript/JavaScript file."""
    content = _read_file(file_path)
    if content is None:
        return f"File not found: {file_path}"
    fp = Path(file_path)
    lines = content.split("\n")
    outline = [f"AST Outline of {fp.name} ---"]
    for i, line in enumerate(lines):
        stripped = line.strip()
        if re.match(r'^\s*(export\s+)?(abstract\s+)?class\s+\w+', stripped):
            m = re.search(r'(?:class)\s+(\w+)', stripped)
            if m:
                outline.append(f"  L{i+1}: Class: {m.group(1)}")
        elif re.match(r'^\s*(export\s+)?(async\s+)?function\s+\w+', stripped):
            m = re.search(r'(?:function)\s+(\w+)', stripped)
            if m:
                outline.append(f"  L{i+1}: Function: {m.group(1)}")
        elif re.match(r'^\s*(export\s+)?interface\s+\w+', stripped):
            m = re.search(r'interface\s+(\w+)', stripped)
            if m:
                outline.append(f"  L{i+1}: Interface: {m.group(1)}")
        elif re.match(r'^\s*(export\s+)?type\s+\w+\s*=', stripped):
            m = re.search(r'type\s+(\w+)', stripped)
            if m:
                outline.append(f"  L{i+1}: Type Alias: {m.group(1)}")
        elif re.match(r'^\s*(export\s+)?enum\s+\w+', stripped):
            m = re.search(r'enum\s+(\w+)', stripped)
            if m:
                outline.append(f"  L{i+1}: Enum: {m.group(1)}")
        elif re.match(r'^\s*export\s+(const|let|var)\s+\w+', stripped):
            m = re.search(r'(?:export)\s+(?:const|let|var)\s+(\w+)', stripped)
            if m:
                outline.append(f"  L{i+1}: Export: {m.group(1)}")
    return "\n".join(outline) if len(outline) > 1 else f"No structures found in {fp.name}"


@mcp.tool()
async def ast_rename_symbol(file_path: str, old_name: str, new_name: str) -> str:
    """Renames an identifier across the file using regex-based replacement."""
    content = _read_file(file_path)
    if content is None:
        return f"File not found: {file_path}"
    if not re.match(r'^[a-zA-Z_$][a-zA-Z0-9_$]*$', new_name):
        return f"Invalid identifier name: {new_name}"
    pattern = re.compile(r'\b' + re.escape(old_name) + r'\b')
    if not pattern.search(content):
        return f"Identifier '{old_name}' not found in file."
    new_content = pattern.sub(new_name, content)
    if _write_file(file_path, new_content):
        count = len(pattern.findall(content))
        return f"Renamed '{old_name}' -> '{new_name}' ({count} occurrences) in {Path(file_path).name}"
    return f"Failed to write file: {file_path}"


@mcp.tool()
async def ast_find_references(file_path: str, symbol_name: str) -> str:
    """Finds all references to an identifier in a file using regex."""
    content = _read_file(file_path)
    if content is None:
        return f"File not found: {file_path}"
    lines = content.split("\n")
    refs = []
    for i, line in enumerate(lines):
        stripped = line.strip()
        if stripped.startswith("//") or stripped.startswith("/*") or stripped.startswith("*"):
            continue
        for m in re.finditer(r'\b' + re.escape(symbol_name) + r'\b', stripped):
            col = m.start()
            snippet = stripped[max(0, col - 15):col + len(symbol_name) + 15]
            refs.append(f"  L{i+1}:{col}: ...{snippet}...")
    if refs:
        return f"References to '{symbol_name}' in {Path(file_path).name} ({len(refs)}):\n" + "\n".join(refs)
    return f"No references to '{symbol_name}' found."


if __name__ == "__main__":
    utils.safe_mcp_run(mcp)
