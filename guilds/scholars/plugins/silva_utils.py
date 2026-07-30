"""
SilvaDB utilities for guilds/scholars/plugins — thin re-export of the
canonical implementation in guilds/core/silva_utils.py.

This used to be a full duplicate of that file, copy-pasted at some point
during the v1-port migration and never touched again. It missed two real
fixes applied to the canonical copy on 2026-07-29/30: routing add_node()
through kernel IPC instead of a raw sqlite3.connect() INSERT (so nodes get
a real BGE-M3 embedding), and resolving KERNEL_URL dynamically instead of
a hardcoded "http://127.0.0.1:3030" (ForjaMCPo3's port, not Tylluan's).
Two copies of the same logic drifting apart is exactly how that second bug
went unnoticed -- re-exporting instead of duplicating means there is only
ever one copy to fix.
"""

from guilds.core.silva_utils import (  # noqa: F401
    DRIFT_SENSITIVE_TYPES,
    KERNEL_URL,
    AUTH_TOKEN,
    compress_for_storage,
    get_silva_db_path,
    add_node,
    search_nodes,
    write_edge,
    ensure_node_exists,
)
