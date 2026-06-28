"""
TylluanNexus Memory Bridge — Unified Memory IPC Client.

This module provides a bridge for Python guilds to access the kernel's
SilvaDB directly via HTTP IPC instead of using local SQLite.

This is the FIRST step toward unified memory: Python guilds now
share the same memory as the Rust kernel.
"""

import json
import os
import urllib.request
from typing import Optional

KERNEL_URL = os.environ.get("TYLLUAN_KERNEL_URL", "http://127.0.0.1:3030")
AUTH_TOKEN = os.environ.get("TYLLUAN_AUTH_TOKEN", "")
TIMEOUT_SECONDS = 30


class MemoryBridge:
    """Unified Memory Bridge to kernel's SilvaDB."""
    
    def __init__(self, kernel_url: str = KERNEL_URL, auth_token: str = AUTH_TOKEN):
        self.url = kernel_url
        self.token = auth_token
    
    def _request(self, endpoint: str, method: str = "POST", data: Optional[dict] = None) -> dict:
        """Make HTTP request to kernel."""
        url = f"{self.url}{endpoint}"
        
        headers = {"Content-Type": "application/json"}
        if self.token:
            headers["Authorization"] = f"Bearer {self.token}"
        
        body = json.dumps(data).encode() if data else None
        
        try:
            req = urllib.request.Request(url, data=body, headers=headers, method=method)
            with urllib.request.urlopen(req, timeout=TIMEOUT_SECONDS) as resp:
                return json.loads(resp.read().decode())
        except Exception as e:
            return {"error": str(e), "status": "failed"}
    
    def write(self, content: str, node_type: str = "entity", metadata: str = "{}") -> dict:
        """Write to SilvaDB via IPC."""
        return self._request("/api/v1/memory/write", "POST", {
            "content": content,
            "node_type": node_type,
            "metadata": metadata,
        })
    
    def search(self, query: str, limit: int = 5) -> dict:
        """Search SilvaDB via IPC."""
        return self._request("/api/v1/memory/search", "POST", {
            "query": query,
            "limit": limit,
        })
    
    def add_edge(self, source: str, target: str, relation: str, metadata: str = "{}") -> dict:
        """Add edge to knowledge graph."""
        return self._request("/api/v1/graph/add", "POST", {
            "source": source,
            "target": target,
            "relation": relation,
            "metadata": metadata,
        })
    
    def get_context(self, node_id: str, depth: int = 2) -> dict:
        """Get context around a node."""
        return self._request("/api/v1/silva/context", "POST", {
            "node_id": node_id,
            "depth": depth,
        })


def get_bridge() -> MemoryBridge:
    """Get singleton bridge instance."""
    return MemoryBridge()