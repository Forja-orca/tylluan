"""
TylluanNexus Code Analysis Guild — Advanced static analysis.
Provides tools for multi-language code analysis, codebase searching, and context extraction.
"""

import os
import sys
import re
import ast
import json
import subprocess
from typing import Optional, Dict, List, Any

_MCP_AVAILABLE = False
try:
    from mcp.server.fastmcp import FastMCP
    from guilds.core import utils
    _MCP_AVAILABLE = True
except ImportError:
    pass

if _MCP_AVAILABLE:
    mcp = FastMCP("tylluan-code-analysis")

# --- TOOL 1: analyze_file ---

def _analyze_rust(source: str, path: str) -> Dict[str, Any]:
    lines = source.splitlines()
    loc = 0
    comments = 0
    blanks = 0
    
    functions = []
    structs_enums = []
    impl_blocks = []
    
    # Regexes
    re_fn = re.compile(r'^\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)')
    re_se = re.compile(r'^\s*(?:pub\s+)?(?:struct|enum)\s+(\w+)')
    re_impl = re.compile(r'^\s*impl(?:<[^>]+>)?\s+(\w+)')
    
    risks = []
    unwraps = source.count(".unwrap()")
    dead_code_allows = source.count("#[allow(dead_code)]")
    unsafe_blocks = source.count("unsafe {")
    
    if unwraps > 0: risks.append(f"Found {unwraps} .unwrap() calls")
    if dead_code_allows > 0: risks.append(f"Found {dead_code_allows} dead_code allows")
    if unsafe_blocks > 0: risks.append(f"Found {unsafe_blocks} unsafe blocks")
    
    for i, line in enumerate(lines, 1):
        stripped = line.strip()
        if not stripped:
            blanks += 1
        elif stripped.startswith("//"):
            comments += 1
        else:
            loc += 1
            if m := re_fn.match(line): functions.append({"name": m.group(1), "line": i})
            if m := re_se.match(line): structs_enums.append({"name": m.group(1), "line": i})
            if m := re_impl.match(line): impl_blocks.append({"name": m.group(1), "line": i})
            
    return {
        "file": path,
        "language": "rust",
        "summary": f"Rust file with {len(functions)} functions and {len(structs_enums)} types.",
        "metrics": {"loc": loc, "functions": len(functions), "complexity": len(impl_blocks)},
        "details": {"structs_enums": structs_enums, "impl_blocks": impl_blocks},
        "risks": risks,
        "todos": [l.strip() for l in lines if "TODO" in l]
    }

def _analyze_python(source: str, path: str) -> Dict[str, Any]:
    try:
        tree = ast.parse(source)
    except SyntaxError as e:
        return {"error": f"Syntax error at line {e.lineno}", "file": path}

    functions = 0
    classes = 0
    complexity = 0
    risks = []
    
    has_main_check = "if __name__" in source
    
    for node in ast.walk(tree):
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            functions += 1
        elif isinstance(node, ast.ClassDef):
            classes += 1
        
        # Complexity heuristic
        if isinstance(node, (ast.If, ast.For, ast.While, ast.Try, ast.ExceptHandler, ast.With)):
            complexity += 1
            
        # Risks
        if isinstance(node, ast.ExceptHandler) and node.type is None:
            risks.append(f"Bare except at line {node.lineno}")
        
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and node.func.id == "print":
            if not has_main_check:
                risks.append(f"Print statement outside __main__ at line {node.lineno}")

    lines = source.splitlines()
    loc = sum(1 for l in lines if l.strip() and not l.strip().startswith("#"))
    
    return {
        "file": path,
        "language": "python",
        "summary": f"Python module with {functions} functions and {classes} classes. Complexity: {complexity}",
        "metrics": {"loc": loc, "functions": functions, "complexity": complexity},
        "risks": list(set(risks)),
        "todos": [l.strip() for l in lines if "TODO" in l]
    }

def _analyze_typescript(source: str, path: str) -> Dict[str, Any]:
    lines = source.splitlines()
    loc = sum(1 for l in lines if l.strip() and not l.strip().startswith("//"))
    
    exports = re.findall(r'export\s+(?:const|class|interface|type|function)\s+(\w+)', source)
    interfaces = re.findall(r'interface\s+(\w+)', source)
    hooks = re.findall(r'(use[A-Z]\w+)', source)
    # Simple heuristic for components: function starting with Uppercase returning JSX-like
    components = re.findall(r'function\s+([A-Z]\w+).*{', source)
    
    risks = []
    if ": any" in source: risks.append("Found usage of ': any'")
    if "console.log" in source: risks.append("Found console.log")
    
    return {
        "file": path,
        "language": "typescript",
        "summary": f"TS/TSX file with {len(exports)} exports and {len(components)} components.",
        "metrics": {"loc": loc, "functions": len(exports), "complexity": len(hooks)},
        "risks": risks,
        "todos": [l.strip() for l in lines if "TODO" in l]
    }

if _MCP_AVAILABLE:
    @mcp.tool()
    async def analyze_file(path: str, analysis_type: str = "full") -> str:
        """Analyze a file (.rs, .py, .ts, .tsx) and return structural insights."""
        try:
            if not os.path.exists(path):
                return json.dumps({"error": f"File not found: {path}"})
            
            with open(path, 'r', encoding='utf-8', errors='replace') as f:
                source = f.read()
            
            ext = os.path.splitext(path)[1].lower()
            if ext == ".rs":
                res = _analyze_rust(source, path)
            elif ext == ".py":
                res = _analyze_python(source, path)
            elif ext in [".ts", ".tsx"]:
                res = _analyze_typescript(source, path)
            else:
                res = {"file": path, "language": "unknown", "summary": "Unsupported extension for deep analysis."}
            
            return json.dumps(res, indent=2)
        except Exception as e:
            return json.dumps({"error": str(e), "partial": True})

# --- TOOL 2: search_codebase ---

if _MCP_AVAILABLE:
    @mcp.tool()
    async def search_codebase(query: str, directory: str = ".", file_type: str = None) -> str:
        """Search for a pattern in the codebase using ripgrep with fallback."""
        try:
            # Try ripgrep
            rg_cmd = ["rg", "--json", "-C", "2", "-m", "10", query, directory]
            if file_type:
                rg_cmd.extend(["-g", f"*.{file_type}"])
            
            try:
                proc = subprocess.run(rg_cmd, capture_output=True, text=True, check=False)
                if proc.returncode in [0, 1] and proc.stdout:
                    return proc.stdout
            except FileNotFoundError:
                pass # rg not installed, use fallback
            
            # Fallback
            results = []
            ext_filter = f".{file_type}" if file_type else None
            regex = re.compile(query, re.IGNORECASE)
            
            count = 0
            for root, _, files in os.walk(directory):
                if count >= 10: break
                for file in files:
                    if ext_filter and not file.endswith(ext_filter): continue
                    fpath = os.path.join(root, file)
                    try:
                        with open(fpath, 'r', encoding='utf-8', errors='replace') as f:
                            lines = f.readlines()
                            for i, line in enumerate(lines):
                                if regex.search(line):
                                    start = max(0, i-2)
                                    end = min(len(lines), i+3)
                                    results.append({
                                        "file": fpath,
                                        "line": i+1,
                                        "context": "".join(lines[start:end])
                                    })
                                    count += 1
                                    if count >= 10: break
                    except: continue
            
            return json.dumps(results, indent=2)
        except Exception as e:
            return json.dumps({"error": str(e)})

# --- TOOL 3: git_context ---

if _MCP_AVAILABLE:
    @mcp.tool()
    async def git_context(directory: str = ".") -> str:
        """Get recent git activity and status."""
        cmds = [
            ["git", "log", "--oneline", "-10"],
            ["git", "diff", "--stat", "HEAD~1"],
            ["git", "status", "--short"],
        ]
        output = []
        for cmd in cmds:
            try:
                res = subprocess.run(cmd, capture_output=True, text=True, timeout=5, cwd=directory)
                output.append(f"--- {' '.join(cmd)} ---\n{res.stdout or res.stderr}")
            except Exception as e:
                output.append(f"--- {' '.join(cmd)} ERROR ---\n{str(e)}")
        return "\n".join(output)

# --- TOOL 4: find_dead_code ---

if _MCP_AVAILABLE:
    @mcp.tool()
    async def find_dead_code(directory: str = ".", language: str = "rust") -> str:
        """Heuristic-based dead code detection."""
        try:
            results = {"language": language, "findings": []}
            if language == "rust":
                # Find #[allow(dead_code)]
                try:
                    proc = subprocess.run(["rg", "-rn", "#\\[allow\\(dead_code\\)\\]", directory], capture_output=True, text=True)
                    for line in proc.stdout.splitlines():
                        results["findings"].append(f"Explicit dead_code allow: {line}")
                except: pass
                
                # Heuristic: pub fn that isn't mentioned in other files
                # (Very simple, might have false positives)
            elif language == "python":
                # Collect all function names
                all_funcs = {}
                for root, _, files in os.walk(directory):
                    for file in files:
                        if file.endswith(".py"):
                            fpath = os.path.join(root, file)
                            try:
                                with open(fpath, 'r') as f:
                                    tree = ast.parse(f.read())
                                    for node in ast.walk(tree):
                                        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
                                            if not node.name.startswith("_"):
                                                all_funcs[node.name] = fpath
                            except: continue
                
                # Check if each function is mentioned elsewhere
                for func, path in all_funcs.items():
                    mentioned = False
                    for root, _, files in os.walk(directory):
                        if mentioned: break
                        for file in files:
                            if file.endswith(".py"):
                                fpath = os.path.join(root, file)
                                if fpath == path: continue
                                try:
                                    with open(fpath, 'r') as f:
                                        if func in f.read():
                                            mentioned = True
                                            break
                                except: continue
                    if not mentioned:
                        results["findings"].append(f"Function likely unused: {func} in {path}")
            
            return json.dumps(results, indent=2)
        except Exception as e:
            return json.dumps({"error": str(e)})

if __name__ == "__main__":
    import sys
    if "--help" in sys.argv:
        print("code_analysis guild — analyze_file, search_codebase, git_context, find_dead_code")
        sys.exit(0)
    if _MCP_AVAILABLE:
        mcp.run()