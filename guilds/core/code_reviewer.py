"""
TylluanNexus Code Reviewer Guild — Static analysis and refactoring suggestions.

This guild provides:
    - Security vulnerability detection (eval, injections, hardcoded secrets)
    - Code quality review (complexity, imports, style)
    - Refactoring suggestions for readability and performance
    - Docstring and type annotation coverage check

No external services required — all local, deterministic analysis.
"""

import ast
import re
import sys
import logging
from pathlib import Path
from typing import Optional

from mcp.server.fastmcp import FastMCP

from guilds.core import utils

mcp = FastMCP("tylluan-code_reviewer")

# ── Security patterns ────────────────────────────────────────────────────────
_SECURITY_PATTERNS = [
    (r"\beval\s*\(", "CRITICAL: eval() allows arbitrary code execution"),
    (r"\bexec\s*\(", "CRITICAL: exec() allows arbitrary code execution"),
    (r"__import__\s*\(", "HIGH: Dynamic import via __import__()"),
    (r"subprocess\.call\s*\(.*shell\s*=\s*True", "HIGH: shell=True in subprocess is injection-prone"),
    (r"os\.system\s*\(", "HIGH: os.system() is injection-prone; use subprocess"),
    (r"pickle\.loads?\s*\(", "HIGH: pickle deserialization can execute arbitrary code"),
    (r"yaml\.load\s*\([^,)]+\)", "MEDIUM: yaml.load() without Loader= is unsafe; use safe_load()"),
    (r"import \*", "LOW: Wildcard import pollutes namespace and hides dependencies"),
    (r"(?i)password\s*=\s*['\"][^'\"]{4,}", "MEDIUM: Possible hardcoded password"),
    (r"(?i)secret\s*=\s*['\"][^'\"]{4,}", "MEDIUM: Possible hardcoded secret"),
    (r"(?i)api_key\s*=\s*['\"][^'\"]{4,}", "MEDIUM: Possible hardcoded API key"),
]

_SEVERITY_ORDER = {"CRITICAL": 0, "HIGH": 1, "MEDIUM": 2, "LOW": 3}


def _severity(finding: str) -> int:
    for k, v in _SEVERITY_ORDER.items():
        if k in finding:
            return v
    return 99


@mcp.tool()
async def review_code(
    code: str = "",
    file_path: str = "",
    language: str = "python",
    intent: str = "",
) -> str:
    """Review source code for security vulnerabilities, quality issues, and style problems.
    Use for: review code, audit code, check security, find vulnerabilities, lint, code smell,
    analyze code quality, check best practices, code review, inspect snippet, scan code.

    Args:
        code: Source code snippet to review (mutually exclusive with file_path).
        file_path: Path to a file on disk to review (mutually exclusive with code).
        language: Programming language hint ('python', 'rust', 'typescript', etc.).
        intent: Natural language description of what to focus on.
    """
    try:
        # Resolve source
        if not code and file_path:
            p = Path(file_path)
            if not p.exists():
                return f"❌ File not found: {file_path}"
            code = p.read_text(encoding="utf-8", errors="replace")
            language = language or _infer_language(p.suffix)

        if not code.strip():
            return "❌ No code provided. Pass `code` or `file_path`."

        lines = code.splitlines()
        findings: list[str] = []

        # Security scan
        for i, line in enumerate(lines, 1):
            stripped = line.strip()
            if stripped.startswith("#"):
                continue
            for pattern, desc in _SECURITY_PATTERNS:
                if re.search(pattern, line):
                    findings.append(f"Line {i}: {desc}\n    → `{stripped[:80]}`")
                    break

        # Python-specific AST checks
        if language == "python":
            try:
                tree = ast.parse(code)
                findings += _ast_checks(tree)
            except SyntaxError as e:
                findings.append(f"SYNTAX ERROR: {e}")

        if not findings:
            return (
                f"✅ **Code Review ({language}) — No issues found**\n\n"
                f"Scanned {len(lines)} lines. Clean of known security patterns and quality issues."
            )

        findings.sort(key=_severity)
        summary = (
            f"🔍 **Code Review ({language}) — {len(findings)} issue(s) in {len(lines)} lines**\n\n"
            + "\n\n".join(f"• {f}" for f in findings[:25])
        )
        if len(findings) > 25:
            summary += f"\n\n…and {len(findings) - 25} more (truncated)."
        return summary

    except Exception as e:
        logging.error("review_code failed: %s", e)
        return f"❌ Review failed: {e}"


@mcp.tool()
async def suggest_refactoring(
    code: str = "",
    file_path: str = "",
    goal: str = "readability",
    intent: str = "",
) -> str:
    """Suggest concrete refactoring steps for a code snippet or file.
    Use for: refactor, improve code, make it cleaner, simplify, optimize, restructure,
    reduce complexity, improve readability, make it faster, better architecture.

    Args:
        code: Source code snippet to refactor.
        file_path: Path to a file on disk.
        goal: Refactoring goal ('readability', 'performance', 'security', 'testability').
        intent: Free-form description of what to improve.
    """
    try:
        if not code and file_path:
            p = Path(file_path)
            if not p.exists():
                return f"❌ File not found: {file_path}"
            code = p.read_text(encoding="utf-8", errors="replace")

        if not code.strip():
            return "❌ No code provided."

        lines = code.splitlines()
        suggestions: list[str] = []

        goal_l = (goal or intent or "").lower()

        # Universal suggestions
        long_lines = [i + 1 for i, l in enumerate(lines) if len(l) > 120]
        if long_lines:
            sample = long_lines[:5]
            suggestions.append(
                f"**Line length**: {len(long_lines)} lines exceed 120 chars "
                f"(lines {sample}{'…' if len(long_lines) > 5 else ''}). "
                "Break into multiple lines or extract sub-expressions."
            )

        deep_nesting = _check_nesting(lines)
        if deep_nesting:
            suggestions.append(
                f"**Deep nesting**: {len(deep_nesting)} locations with 4+ indent levels "
                f"(lines {deep_nesting[:5]}). Extract to helper functions or use early returns."
            )

        # Goal-specific suggestions
        if "readab" in goal_l or not goal_l:
            suggestions += [
                "**Naming**: Replace single-letter variables (`i`, `x`, `d`) with descriptive names.",
                "**Magic numbers**: Extract numeric literals into named constants.",
                "**Comments**: Replace obvious comments with expressive function/variable names.",
            ]

        if "perf" in goal_l:
            suggestions += [
                "**Comprehensions**: Replace `for` + `append` loops with list/dict comprehensions.",
                "**Early exit**: Add guard clauses at function entry to avoid deep nesting.",
                "**Caching**: Identify repeated expensive calls; consider `functools.lru_cache`.",
            ]

        if "secur" in goal_l:
            suggestions += [
                "**Input validation**: Add explicit type/range checks at all public entry points.",
                "**Least privilege**: Narrow file permissions and subprocess access scope.",
                "**Secrets**: Move credentials to environment variables or a secrets manager.",
            ]

        if "test" in goal_l:
            suggestions += [
                "**Pure functions**: Extract side-effect-free logic into pure functions — easier to unit-test.",
                "**Dependency injection**: Pass dependencies as parameters instead of importing globals.",
                "**Small functions**: Functions >30 lines are hard to test; break them down.",
            ]

        if not suggestions:
            return f"✅ Code looks clean for goal '{goal}'. No specific suggestions."

        return (
            f"💡 **Refactoring Suggestions — goal: `{goal}`** ({len(lines)} lines)\n\n"
            + "\n\n".join(f"{i+1}. {s}" for i, s in enumerate(suggestions))
        )

    except Exception as e:
        logging.error("suggest_refactoring failed: %s", e)
        return f"❌ Refactoring analysis failed: {e}"


@mcp.tool()
async def check_coverage(
    code: str = "",
    file_path: str = "",
    intent: str = "",
) -> str:
    """Check docstring and type annotation coverage for Python code.
    Use for: check documentation, missing docstrings, type hints, annotation coverage,
    undocumented functions, missing types, doc coverage.

    Args:
        code: Python source code to inspect.
        file_path: Path to a Python file on disk.
        intent: Free-form description of what to check.
    """
    try:
        if not code and file_path:
            p = Path(file_path)
            if not p.exists():
                return f"❌ File not found: {file_path}"
            code = p.read_text(encoding="utf-8", errors="replace")

        if not code.strip():
            return "❌ No code provided."

        try:
            tree = ast.parse(code)
        except SyntaxError as e:
            return f"❌ Syntax error — cannot parse: {e}"

        funcs = [n for n in ast.walk(tree) if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef))]
        classes = [n for n in ast.walk(tree) if isinstance(n, ast.ClassDef)]

        missing_docs: list[str] = []
        missing_types: list[str] = []

        for f in funcs:
            if not (f.body and isinstance(f.body[0], ast.Expr) and isinstance(f.body[0].value, ast.Constant)):
                missing_docs.append(f"`{f.name}` (line {f.lineno})")
            unannotated = [a.arg for a in f.args.args if a.annotation is None and a.arg != "self"]
            if f.returns is None and f.name != "__init__":
                unannotated.append("→ return")
            if unannotated:
                missing_types.append(f"`{f.name}`: {', '.join(unannotated)}")

        total_funcs = len(funcs)
        doc_pct = round(100 * (total_funcs - len(missing_docs)) / max(total_funcs, 1))
        type_pct = round(100 * (total_funcs - len(missing_types)) / max(total_funcs, 1))

        report = (
            f"📋 **Coverage Report** — {total_funcs} functions, {len(classes)} classes\n\n"
            f"• Docstrings: {doc_pct}% covered\n"
            f"• Type annotations: {type_pct}% covered\n"
        )
        if missing_docs:
            report += f"\n**Missing docstrings** ({len(missing_docs)}):\n" + "\n".join(f"  – {m}" for m in missing_docs[:15])
        if missing_types:
            report += f"\n\n**Missing type hints** ({len(missing_types)}):\n" + "\n".join(f"  – {m}" for m in missing_types[:15])

        return report

    except Exception as e:
        logging.error("check_coverage failed: %s", e)
        return f"❌ Coverage check failed: {e}"


# ── Helpers ──────────────────────────────────────────────────────────────────

def _infer_language(suffix: str) -> str:
    return {
        ".py": "python", ".rs": "rust", ".ts": "typescript", ".tsx": "typescript",
        ".js": "javascript", ".jsx": "javascript", ".go": "go", ".cpp": "cpp",
        ".c": "c", ".rb": "ruby", ".java": "java",
    }.get(suffix.lower(), "text")


def _ast_checks(tree: ast.AST) -> list[str]:
    issues: list[str] = []
    for node in ast.walk(tree):
        # Functions longer than 60 lines
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            body_lines = (node.end_lineno or node.lineno) - node.lineno
            if body_lines > 60:
                issues.append(
                    f"LOW: `{node.name}` is {body_lines} lines long (line {node.lineno}). "
                    "Consider splitting into smaller functions."
                )
        # Bare except
        if isinstance(node, ast.ExceptHandler) and node.type is None:
            issues.append(
                f"LOW: Bare `except:` at line {node.lineno} catches everything including KeyboardInterrupt. "
                "Use `except Exception:`."
            )
    return issues


def _check_nesting(lines: list[str]) -> list[int]:
    deep: list[int] = []
    for i, line in enumerate(lines, 1):
        indent = len(line) - len(line.lstrip())
        if indent >= 16:  # 4 levels × 4 spaces
            deep.append(i)
    return deep


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO, stream=sys.stderr)
    utils.safe_mcp_run(mcp)
