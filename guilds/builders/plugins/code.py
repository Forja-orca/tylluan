"""
TylluanNexus Code Guild — Local code analysis without external services.

This guild provides:
    - Parse code files and extract structures
    - Count lines of code by language
    - Find TODO/FIXME/BUG markers
    - Basic complexity estimation
    - Security vulnerability patterns
    
No external services required - all local analysis.
"""

import os
import re as _re
import re
import ast
import fnmatch
from pathlib import Path
from typing import Optional
from collections import defaultdict

from mcp.server.fastmcp import FastMCP

from guilds.builders.plugins import utils

mcp = FastMCP("tylluan-code")


def _extract_path_from_intent(intent: str, field_val: str) -> str:
    """Si field_val está vacío o es '.', extrae una ruta del texto del intent."""
    if field_val and field_val not in ('.', ''):
        return field_val
    # Search for path patterns in the intent
    patterns = [
        r'(?:file|archivo|fichero|path)\s+["\']?([^\s"\']+\.\w+)',  # "file X.rs"
        r'["\']([^"\']+\.\w+)["\']',                                  # 'X.rs'
        r'((?:crates|src|guilds|dashboard|scripts)/[^\s,]+)',         # crates/X/Y
        r'(\S+\.(?:rs|py|ts|tsx|js|toml|md|json))',                  # cualquier .ext
    ]
    for pat in patterns:
        m = _re.search(pat, intent, _re.IGNORECASE)
        if m:
            return m.group(1)
    return field_val  # devuelve el original si no encuentra nada


def extract_file_path(intent: str) -> str:
    """Extract file paths from natural language intents."""
    if not intent:
        return ""
    
    # Check for explicit 'file X' or 'archivo X'
    match = re.search(r'(?:file|archivo|path)\s+([^\s\'",:]+)', intent, re.IGNORECASE)
    if match:
        return match.group(1).strip()
        
    # Check for typical source code files
    match = re.search(r'([a-zA-Z0-9_/\-]+\.(?:rs|py|tsx|ts|js|jsx|html|css|json|md|toml|yml|yaml|c|cpp|h))', intent)
    if match:
        return match.group(1).strip()
        
    # Check for src/ or crates/
    match = re.search(r'((?:src|crates|guilds|dashboard)/[^\s\'",:]+)', intent)
    if match:
        return match.group(1).strip()
        
    return ""


@mcp.tool()
async def code_parse(
    file_path: str = ".",
    path: str = "",
    detail_level: str = "basic",
    intent: str = "",
) -> str:
    """Analyze, read, parse and explain a source code file. Use for: analyze file, read file, explain code, what does this file do, file structure, count lines, parse source, inspect functions, show imports, check syntax.

    Args:
        file_path: Path to code file
        path: Alias for file_path (kernel sends 'path' key)
        detail_level: "basic" (functions/classes) or "full" (all nodes)
        intent: Natural language intent for fallback path extraction

    Returns:
        Extracted code structures
    """
    # Accept 'path' as alias when kernel sends path= instead of file_path=
    if path and (not file_path or file_path == "."):
        file_path = path
    file_path = _extract_path_from_intent(intent, file_path)
        
    if not file_path or not os.path.isfile(file_path):
        return f"❌ File not found: {file_path or 'none provided'}"
    
    content = utils.safe_read_file(file_path)
    if content is None:
        return "⚠️ File too large or unreadable"
    
    lang = utils.detect_language(file_path)
    
    if lang == "Python":
        return parse_python(content, detail_level)
    elif lang in ["JavaScript", "TypeScript", "React"]:
        return parse_js_ts(content, detail_level)
    else:
        return f"ℹ️ Language '{lang}' - basic file info:\n\n" + get_basic_info(content, file_path)


def get_basic_info(content: str, file_path: str) -> str:
    """Get basic file information."""
    lines = content.split("\n")
    return f"""📄 {os.path.basename(file_path)}
   Lines: {len(lines)}
   Size: {len(content)} bytes
   Language: {utils.detect_language(file_path)}
   
   First 10 lines:
   {"".join(lines[:10])}"""


def parse_python(content: str, detail_level: str) -> str:
    """Parse Python AST."""
    try:
        tree = ast.parse(content)
    except SyntaxError as e:
        return f"❌ Python syntax error: {e}"
    
    result = [f"🐍 Python file parsed successfully"]
    
    # Count nodes
    functions = []
    classes = []
    import_set = set()
    
    for node in ast.walk(tree):
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            functions.append(node.name)
        elif isinstance(node, ast.ClassDef):
            classes.append(node.name)
        elif isinstance(node, (ast.Import, ast.ImportFrom)):
            if isinstance(node, ast.ImportFrom):
                module = node.module or ""
                for alias in node.names:
                    import_set.add(f"{module}.{alias.name}" if module else alias.name)
            else:
                for alias in node.names:
                    import_set.add(alias.name)
    
    imports = sorted(list(import_set))
    
    if functions:
        result.append(f"\n📦 Functions ({len(functions)}):")
        for fn in functions[:20]:
            # Use appropriate icon for async
            result.append(f"   • {fn}()")
        if len(functions) > 20:
            result.append(f"   ... and {len(functions) - 20} more")
    
    if classes:
        result.append(f"\n🏛️ Classes ({len(classes)}):")
        for cls in classes[:20]:
            result.append(f"   • {cls}")
    
    if imports:
        result.append(f"\n📥 Imports ({len(imports)}):")
        for imp in imports[:15]:
            result.append(f"   • {imp}")
    
    return "\n".join(result)


def parse_js_ts(content: str, detail_level: str) -> str:
    """Basic JavaScript/TypeScript parsing (regex-based for now)."""
    result = [f"📜 JavaScript/TypeScript file"]
    
    # Find functions
    fn_pattern = r"(?:function\s+(\w+)|const\s+(\w+)\s*=\s*(?:async\s+)?\(|(\w+)\s*\([^)]*\)\s*\{)"
    matches = re.findall(fn_pattern, content)
    functions = [m[0] or m[1] or m[2] for m in matches if any(m)]
    
    # Find classes
    class_pattern = r"class\s+(\w+)"
    classes = re.findall(class_pattern, content)
    
    # Find imports
    import_pattern = r"(?:import\s+(?:.*?\s+from\s+)?['\"]([^'\"]+)['\"])"
    imports = re.findall(import_pattern, content)
    
    if functions:
        result.append(f"\n📦 Functions ({len(functions)}):")
        for fn in functions[:15]:
            result.append(f"   • {fn}")
    
    if classes:
        result.append(f"\n🏛️ Classes ({len(classes)}):")
        for cls in classes[:15]:
            result.append(f"   • {cls}")
    
    if imports:
        result.append(f"\n📥 Imports ({len(imports)}):")
        for imp in imports[:10]:
            result.append(f"   • {imp}")
    
    return "\n".join(result)


@mcp.tool()
async def code_stats(
    path: str = ".",
    include_pattern: str = "*",
    intent: str = "",
) -> str:
    """Count lines of code by language in a directory. For single files, use code_parse instead.
    
    Args:
        path: Directory path to analyze (NOT a file path — use code_parse for files)
        include_pattern: Glob pattern for files (e.g., "*.py", "*.{js,ts}")
        intent: Natural language intent for fallback path extraction
    
    Returns:
        Lines of code statistics by language
    """
    # Extract include_pattern from intent if still default
    if include_pattern == "*" and intent:
        import re as _re
        m = _re.search(r'(\*\.[a-zA-Z0-9]+|\*\.?\{[^}]+\})', intent)
        if m:
            include_pattern = m.group(1)
        else:
            lang_exts = {
                'rust': '*.rs', 'python': '*.py', 'typescript': '*.ts',
                'javascript': '*.js', 'react': '*.tsx', 'go': '*.go',
                'java': '*.java', 'toml': '*.toml', 'json': '*.json',
                'markdown': '*.md', 'yaml': '*.yaml',
            }
            for lang, ext in lang_exts.items():
                if lang in intent.lower():
                    include_pattern = ext
                    break

    # Extract path from intent if still default or empty
    if (not path or path == ".") and intent:
        import re as _re
        m = _re.search(r'\b(crates|guilds|src|dashboard|scripts|docs|reports)(?:/\S*)?\b', intent)
        if m:
            extracted = m.group(0).rstrip('/')
            if os.path.isdir(extracted) or os.path.isdir(os.path.abspath(extracted)):
                path = extracted

    path = os.path.abspath(path) if path and path not in ('.', '') else path

    # If path resolves to a file, delegate to code_parse
    if path and os.path.isfile(path):
        return await code_parse(file_path=path, detail_level="normal", intent=intent)

    if not os.path.isdir(path):
        return f"❌ Directory not found: {path}"
    
    stats = defaultdict(lambda: {"files": 0, "lines": 0})
    ignore_dirs = {".git", "node_modules", "target", "__pycache__", ".venv", "venv", "dist", "build"}
    
    try:
        root = Path(path)
        for file_path in root.rglob("*"):
            # Skip ignored directories early
            if any(part in ignore_dirs for part in file_path.parts):
                continue
                
            if not file_path.is_file():
                continue
            
            # Filter by pattern
            if include_pattern != "*":
                if not fnmatch.fnmatch(file_path.name, include_pattern):
                    continue
            
            lang = utils.detect_language(str(file_path))
            if lang == "Unknown":
                continue
            
            try:
                with open(file_path, "r", encoding="utf-8", errors="ignore") as f:
                    lines = len(f.readlines())
                
                stats[lang]["files"] += 1
                stats[lang]["lines"] += lines
            except Exception:
                continue
        
        if not stats:
            return "⚠️ No code files found"
        
        # Prepare table data
        columns = ["Language", "Files", "Lines", "% Lines"]
        rows = []
        
        total_files = sum(s["files"] for s in stats.values())
        total_lines = sum(s["lines"] for s in stats.values())
        
        sorted_stats = sorted(stats.items(), key=lambda x: x[1]["lines"], reverse=True)
        for lang, data in sorted_stats:
            pct = (data["lines"] / total_lines * 100) if total_lines else 0
            rows.append([
                lang, 
                str(data["files"]), 
                str(data["lines"]), 
                f"{pct:.1f}%"
            ])
        
        summary = f"📊 Analysis complete: {total_files} files, {total_lines} lines.\n\n"
        table = utils.format_table(columns, rows)
        
        return summary + table
        
    except Exception as e:
        return f"❌ Analysis failed: {str(e)}"


@mcp.tool()
async def code_todos(
    path: str = ".",
    patterns: str = "TODO,FIXME,BUG,HACK,XXX",
    intent: str = "",
) -> str:
    """Find TODO/FIXME/BUG markers in code.
    
    Args:
        path: Directory or file to scan
        patterns: Comma-separated patterns to find
        intent: Natural language intent for fallback path extraction
    
    Returns:
        List of markers with locations
    """
    path = os.path.abspath(path) if path and path not in ('.', '') else path
    
    if path and os.path.isfile(path):
        return await code_parse(file_path=path, detail_level="normal", intent=intent)

    if path == "." or not path:
        extracted = extract_file_path(intent)
        if extracted and extracted != ".":
            path = os.path.abspath(extracted)
            
    if path and os.path.isfile(path):
        return await code_parse(file_path=path, detail_level="normal", intent=intent)
            
    if not os.path.exists(path):
        return f"❌ Path not found: {path}"
    
    search_patterns = [p.strip() for p in patterns.split(",")]
    results = []
    
    try:
        root = Path(path)
        is_file = root.is_file()
        
        if is_file:
            files = [root]
        else:
            files = [f for f in root.rglob("*") if f.is_file() and detect_language(f.name) != "Unknown"]
        
        for file_path in files[:200]:
            try:
                with open(file_path, "r", encoding="utf-8", errors="ignore") as f:
                    for i, line in enumerate(f, 1):
                        for pattern in search_patterns:
                            if pattern.lower() in line.lower():
                                rel_path = file_path.relative_to(root) if not is_file else file_path.name
                                results.append(f"📍 {rel_path}:{i} — {pattern}")
                                break
            except Exception:
                continue
        
        if not results:
            return f"🔍 No markers found for: {patterns}"
        
        return f"🏷️ Found {len(results)} markers:\n\n" + "\n".join(results[:50])
        
    except Exception as e:
        return f"❌ Scan failed: {str(e)}"


@mcp.tool()
async def code_security_scan(
    path: str = ".",
    scan_secrets: bool = True,
    intent: str = "",
) -> str:
    """Scan code for potential security issues.
    
    Args:
        path: Directory to scan
        scan_secrets: Check for hardcoded secrets/keys
        intent: Natural language intent for fallback path extraction
    
    Returns:
        List of potential security issues
    """
    path = os.path.abspath(path) if path and path not in ('.', '') else path

    if path and os.path.isfile(path):
        return await code_parse(file_path=path, detail_level="normal", intent=intent)

    if path == "." or not path:
        extracted = extract_file_path(intent)
        if extracted and extracted != ".":
            path = os.path.abspath(extracted)
            
    if path and os.path.isfile(path):
        return await code_parse(file_path=path, detail_level="normal", intent=intent)
            
    if not os.path.exists(path):
        return f"❌ Path not found: {path}"
    
    issues = []
    
    # Patterns that might indicate security issues
    secret_patterns = [
        (r"(?i)api[_-]?key\s*[=:]\s*['\"][a-zA-Z0-9]{20,}", "Hardcoded API key"),
        (r"(?i)password\s*[=:]\s*['\"][^'\"]{8,}", "Hardcoded password"),
        (r"(?i)secret\s*[=:]\s*['\"][a-zA-Z0-9]{20,}", "Hardcoded secret"),
        (r"(?i)token\s*[=:]\s*['\"][a-zA-Z0-9]{20,}", "Hardcoded token"),
    ]
    
    dangerous_patterns = [
        (r"eval\s*\(", "Use of eval()"),
        (r"exec\s*\(", "Use of exec()"),
        (r"os\.system\s*\(", "Use of os.system()"),
        (r"subprocess\s*\.\s*shell\s*=\s*True", "Shell=True in subprocess"),
    ]
    
    try:
        root = Path(path)
        is_file = root.is_file()
        
        if is_file:
            files = [root]
        else:
            files = [f for f in root.rglob("*") if f.is_file()]
        
        for file_path in files[:100]:
            try:
                with open(file_path, "r", encoding="utf-8", errors="ignore") as f:
                    for i, line in enumerate(f, 1):
                        # Check secrets
                        if scan_secrets:
                            for pattern, desc in secret_patterns:
                                if re.search(pattern, line):
                                    rel_path = file_path.relative_to(root) if not is_file else file_path.name
                                    issues.append(f"🔴 {rel_path}:{i} — {desc}")
                                    break
                        
                        # Check dangerous patterns
                        for pattern, desc in dangerous_patterns:
                            if re.search(pattern, line):
                                rel_path = file_path.relative_to(root) if not is_file else file_path.name
                                issues.append(f"🟠 {rel_path}:{i} — {desc}")
                                break
            except Exception:
                continue
        
        if not issues:
            return "✅ No security issues detected"
        
        return f"⚠️ Found {len(issues)} potential issues:\n\n" + "\n".join(issues[:30])
        
    except Exception as e:
        return f"❌ Scan failed: {str(e)}"


if __name__ == "__main__":
    utils.safe_mcp_run(mcp)