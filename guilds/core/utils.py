"""
Shared utilities for TylluanNexus Guilds.

This module provides common functionality used across multiple guilds:
- Safe file operations with size limits
- Async subprocess execution with timeout
- Path validation and security checks
- Common data processing helpers
"""

import os
import sys
import shutil
import asyncio
import logging
import warnings
from pathlib import Path
from typing import Optional, Any
from contextlib import redirect_stdout

# ANTI-ORPHAN: Note - FD redirection moved to safe_mcp_run() to avoid
# corrupting stdout before MCP handshake. This module-level redirection
# was causing MCP protocol corruption.
# Original code moved inside safe_mcp_run() for proper lifecycle management.

# --- Sovereign Global Logging Configuration ---
# Redirect all logging and warnings to stderr to protect MCP stdout channel.
logging.basicConfig(
    level=logging.WARNING,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s',
    stream=sys.stderr,
    force=True  # Ensure we override any existing config
)

# Force warnings to stderr
def warn_with_log(message, category, filename, lineno, file=None, line=None):
    logging.warning(f"{filename}:{lineno}: {category.__name__}: {message}")

warnings.showwarning = warn_with_log


def safe_read_file(path: str, max_size: int = 1_000_000) -> Optional[str]:
    """Read file with size limit (1MB default).
    
    Args:
        path: File path to read
        max_size: Maximum file size in bytes
        
    Returns:
        File contents or None if too large/unreadable
    """
    try:
        abs_path = os.path.abspath(path)
        if not os.path.isfile(abs_path):
            return None
        if os.path.getsize(abs_path) > max_size:
            return None
        with open(abs_path, "r", encoding="utf-8", errors="ignore") as f:
            return f.read()
    except Exception:
        return None


async def run_command(
    cmd: list[str],
    cwd: Optional[str] = None,
    timeout_secs: int = 30,
    capture_stderr: bool = True,
) -> tuple[int, str, str]:
    """Run a command asynchronously with timeout.
    
    Args:
        cmd: Command and arguments as list
        cwd: Working directory
        timeout_secs: Timeout in seconds
        capture_stderr: Whether to capture stderr
        
    Returns:
        Tuple of (return_code, stdout, stderr)
    """
    if not cmd:
        return (-1, "", "Error: Empty command")
    
    try:
        process = await asyncio.create_subprocess_exec(
            *cmd,
            cwd=cwd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE if capture_stderr else None,
        )
        
        stdout, stderr = await asyncio.wait_for(
            process.communicate(),
            timeout=timeout_secs,
        )
        
        return (
            process.returncode,
            stdout.decode(errors="replace") if stdout else "",
            stderr.decode(errors="replace") if stderr else "",
        )
    except asyncio.TimeoutError:
        process.kill()
        await process.wait()
        raise asyncio.TimeoutError(f"Command timed out after {timeout_secs}s")


def validate_path(base_dir: str, target_path: str) -> bool:
    """Validate that target path is within base directory (prevent path traversal).
    
    Args:
        base_dir: Base directory that target must be within
        target_path: Path to validate
        
    Returns:
        True if path is safe, False otherwise
    """
    try:
        base = Path(os.path.abspath(base_dir)).resolve()
        target = Path(os.path.abspath(target_path)).resolve()
        return target.is_relative_to(base)
    except Exception:
        return False


def detect_language(filename: str) -> str:
    """Detect programming language from file extension.
    
    Args:
        filename: Name of file with extension
        
    Returns:
        Language name or "Unknown"
    """
    ext = os.path.splitext(filename)[1].lower()
    lang_map = {
        ".py": "Python",
        ".js": "JavaScript",
        ".ts": "TypeScript",
        ".jsx": "React",
        ".tsx": "React",
        ".rs": "Rust",
        ".go": "Go",
        ".java": "Java",
        ".c": "C",
        ".cpp": "C++",
        ".h": "C/C++",
        ".cs": "C#",
        ".rb": "Ruby",
        ".php": "PHP",
        ".swift": "Swift",
        ".kt": "Kotlin",
        ".sql": "SQL",
        ".sh": "Shell",
        ".ps1": "PowerShell",
        ".yaml": "YAML",
        ".yml": "YAML",
        ".json": "JSON",
        ".toml": "TOML",
        ".md": "Markdown",
    }
    return lang_map.get(ext, "Unknown")


def find_executable(name: str) -> Optional[str]:
    """Check if executable exists in PATH.
    
    Args:
        name: Executable name (with or without .exe)
        
    Returns:
        Full path to executable or None if not found
    """
    return shutil.which(name)


def truncate_output(text: str, max_chars: int = 50000) -> str:
    """Truncate output to prevent context overflow.
    
    Args:
        text: Text to truncate
        max_chars: Maximum characters to return
        
    Returns:
        Truncated text with indicator
    """
    if len(text) <= max_chars:
        return text
    return text[:max_chars] + f"\n\n⚠️ Output truncated ({len(text)} chars total)"


def format_table(columns: list, rows: list, max_col_width: int = 50) -> str:
    """Format data as readable text table.
    
    Args:
        columns: Column names
        rows: Data rows
        max_col_width: Maximum width per column
        
    Returns:
        Formatted table string
    """
    if not rows:
        return "0 rows"
    
    # Calculate widths
    widths = {col: min(len(str(col)), max_col_width) for col in columns}
    for row in rows:
        for i, val in enumerate(row):
            if i < len(columns):
                val_str = str(val)[:max_col_width]
                widths[columns[i]] = max(widths[columns[i]], len(val_str))
    
    # Build table
    header = " | ".join(col.ljust(widths[col]) for col in columns)
    sep = "-+-".join("-" * widths[col] for col in columns)
    
    lines = [header, sep]
    for row in rows:
        line = " | ".join(
            str(val)[:max_col_width].ljust(widths[columns[i]]) if i < len(columns) else ""
            for i, val in enumerate(row)
        )
        lines.append(line)
    
    return "\n".join(lines)


def safe_mcp_run(mcp: Any):
    """Execute FastMCP server with proper stdout isolation.
    
    FastMCP needs a clean stdout for JSON-RPC protocol.
    All other output (logging, warnings, library debug) goes to stderr.
    """
    logging.debug(f"🚀 Starting Sovereign Guild: {mcp.name}")
    try:
        # T27: Aggressive stdout protection.
        # Ensure we flush any stray output before starting the MCP loop.
        sys.stdout.flush()
        
        # We don't use redirect_stdout here because FastMCP needs to OWN stdout.
        # But we ensure logging is definitely on stderr.
        mcp.run()
    except Exception as e:
        logging.error(f"❌ Guild Critical Failure: {e}")
        sys.exit(1)