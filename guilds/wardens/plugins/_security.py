"""
Sovereign Security Configuration for TylluanNexus Guilds.
Centralizes exclusion patterns and safety constants.
"""

# Directories to skip during recursive filesystem operations
SKIP_DIRS = {
    ".git", ".venv", "__pycache__", "node_modules", "target", "dist", 
    ".pytest_cache", ".mypy_cache", ".ruff_cache", "backups", "logs",
    ".gemini", ".anthropic"
}

# Sensitive files to never read or search
SKIP_FILES = {
    "tylluan.db", "silva.db", "mailbox.db", ".env", ".tylluan-token", 
    "tylluan-nexus.pid", "Cargo.lock", "package-lock.json", "id_rsa", "id_ed25519"
}

# Binary or large extensions to ignore by default
SKIP_EXTENSIONS = {
    ".exe", ".dll", ".so", ".dylib", ".bin", ".pyc", ".pyo", ".pyd", 
    ".db", ".sqlite", ".tar", ".gz", ".zip", ".7z", ".png", ".jpg", ".jpeg", 
    ".gif", ".svg", ".ico", ".pdf", ".docx", ".xlsx", ".pptx"
}

def rg_exclude_flags():
    """Returns ripgrep-compatible glob flags based on SKIP constants."""
    flags = []
    for d in SKIP_DIRS:
        flags.extend(["-g", f"!{d}/"])
    for f in SKIP_FILES:
        flags.extend(["-g", f"!{f}"])
    for e in SKIP_EXTENSIONS:
        flags.extend(["-g", f"!*{e}"])
    return flags
