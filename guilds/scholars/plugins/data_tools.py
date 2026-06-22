"""
TylluanNexus Scholar Guild — Data Tools.

Provides tools for manipulating and querying common data formats:
JSON, YAML, and CSV.

Status: operational (degraded: requires mcp fastmcp pyyaml)
Last verified: 2026-05-11
"""

import json
import os
import csv
import sys
from typing import Any, Optional

_MCP_AVAILABLE = False
try:
    from mcp.server.fastmcp import FastMCP
    from guilds.core import utils
    _MCP_AVAILABLE = True
except ImportError as e:
    _MCP_AVAILABLE = False
    print(f"⚠️ Data Tools Guild: MCP dependencies not available: {e}", file=sys.stderr)

_YAML_AVAILABLE = False
if _MCP_AVAILABLE:
    try:
        import yaml
        _YAML_AVAILABLE = True
    except ImportError:
        pass

if _MCP_AVAILABLE:
    mcp = FastMCP("tylluan-data")
    
    def _deep_merge(target: dict, source: dict) -> dict:
        """Recursively merge two dictionaries."""
        for key, value in source.items():
            if isinstance(value, dict) and key in target and isinstance(target[key], dict):
                _deep_merge(target[key], value)
            else:
                target[key] = value
        return target

    @mcp.tool()
    async def json_query(path: str, query: str) -> str:
        """Find a value in a JSON file using a simple dot-notation query.

        Args:
            path: Absolute path to the JSON file.
            query: Dot-separated path (e.g., 'users.0.name').
        """
        if not os.path.exists(path):
            return f"❌ Error: File not found: {path}"

        try:
            with open(path, "r", encoding="utf-8") as f:
                data = json.load(f)
            
            # Traverse the data
            current: Any = data
            for part in query.split("."):
                if part.isdigit():
                    current = current[int(part)]
                else:
                    current = current[part]

            return json.dumps(current, indent=2)
        except Exception as e:
            return f"❌ JSON Query Error: {e}"

    @mcp.tool()
    async def yaml_merge(paths: list[str], output_path: str) -> str:
        """Deep-merge multiple YAML files into one.

        Args:
            paths: List of absolute paths to YAML files to merge.
            output_path: Path where the merged YAML will be saved.
        """
        if not _YAML_AVAILABLE:
            return "❌ YAML unavailable. Install: pip install pyyaml"
        try:
            result: dict = {}
            for path in paths:
                if not os.path.exists(path):
                    return f"❌ Error: File not found: {path}"
                with open(path, "r", encoding="utf-8") as f:
                    data = yaml.safe_load(f) or {}
                    _deep_merge(result, data)

            with open(output_path, "w", encoding="utf-8") as f:
                yaml.safe_dump(result, f, default_flow_style=False)

            return f"✅ Successfully merged {len(paths)} YAMLs into {output_path}."
        except Exception as e:
            return f"❌ YAML Merge Error: {e}"

    @mcp.tool()
    async def csv_query(
        path: str,
        column: Optional[str] = None,
        filter_val: Optional[str] = None,
        limit: int = 20,
    ) -> str:
        """Search and filter a CSV file.

        Args:
            path: Absolute path to the CSV file.
            column: Column name to filter by.
            filter_val: Value to search for in that column.
            limit: Maximum number of rows to return (default: 20).
        """
        if not os.path.exists(path):
            return f"❌ Error: File not found: {path}"

        try:
            results = []
            with open(path, "r", encoding="utf-8") as f:
                reader = csv.DictReader(f)
                count = 0
                for row in reader:
                    if column and filter_val:
                        if row.get(column) == filter_val:
                            results.append(row)
                            count += 1
                    else:
                        results.append(row)
                        count += 1
                    
                    if count >= limit:
                        break

            if not results:
                return "🔍 No matching rows found."

            return json.dumps(results, indent=2)
        except Exception as e:
            return f"❌ CSV Error: {e}"

    if __name__ == "__main__":
        utils.safe_mcp_run(mcp)
else:
    # Stub when MCP not available
    class StubMCP:
        @staticmethod
        def tool(func):
            return func
    
    mcp = StubMCP()
    
    async def json_query(path: str, query: str) -> str:
        return "❌ Data Tools guild unavailable. Install: pip install mcp fastmcp"
    
    async def yaml_merge(paths: list[str], output_path: str) -> str:
        return "❌ Data Tools guild unavailable. Install: pip install mcp fastmcp pyyaml"
    
    async def csv_query(path: str, column: Optional[str] = None, filter_val: Optional[str] = None, limit: int = 20) -> str:
        return "❌ Data Tools guild unavailable. Install: pip install mcp fastmcp"
    
    if __name__ == "__main__":
        print("Data Tools Guild: MCP not available. Install: pip install mcp fastmcp")