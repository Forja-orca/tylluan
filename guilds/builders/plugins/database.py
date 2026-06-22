"""
TylluanNexus Database Guild — SQLite operations without external services.

This guild provides:
    - Query any SQLite database
    - List tables and schema
    - Export to CSV/JSON
    - Basic CRUD operations
    
No external services required - uses built-in sqlite3.

Status: operational (degraded: requires mcp fastmcp installed)
Last verified: 2026-05-11
"""

import os
import sqlite3
import json
import csv
import io
import sys
from pathlib import Path
from typing import Optional, List, Dict, Any
from contextlib import contextmanager

_MCP_AVAILABLE = False
try:
    from mcp.server.fastmcp import FastMCP
    from guilds.core import utils
    _MCP_AVAILABLE = True
except ImportError as e:
    _MCP_AVAILABLE = False
    print(f"⚠️ Database Guild: MCP dependencies not available: {e}", file=sys.stderr)

if _MCP_AVAILABLE:
    mcp = FastMCP("tylluan-database")
else:
    class StubMCP:
        @staticmethod
        def tool(func):
            return func
    mcp = StubMCP()

# Security: restrict to specific paths
ALLOWED_DIRS = [
    os.getcwd(),
    os.path.expanduser("~"),
    os.path.join(os.getcwd(), "data"),
    os.path.join(os.getcwd(), "db"),
]


def get_connection(db_path: str) -> sqlite3.Connection:
    """Create a safe database connection."""
    abs_path = os.path.abspath(db_path)
    
    # Security: check path is in allowed dirs
    allowed = False
    for allowed_dir in ALLOWED_DIRS:
        if abs_path.startswith(os.path.abspath(allowed_dir)):
            allowed = True
            break
    
    if not allowed:
        raise PermissionError(f"Access denied: {db_path} not in allowed paths")
    
    if not os.path.exists(abs_path):
        raise FileNotFoundError(f"Database not found: {db_path}")
    
    conn = sqlite3.connect(abs_path)
    conn.row_factory = sqlite3.Row
    return conn


@contextmanager
def safe_db(db_path: str):
    """Context manager for safe DB operations."""
    conn = get_connection(db_path)
    try:
        yield conn
    finally:
        conn.close()


@mcp.tool()
async def db_query(
    db_path: str,
    sql: str,
    params: Optional[list] = None,
    max_rows: int = 100,
) -> str:
    """Execute a SELECT query on a SQLite database.
    
    Args:
        db_path: Path to SQLite database file
        sql: SQL query to execute (SELECT only for safety)
        params: Optional query parameters
        max_rows: Maximum rows to return
    
    Returns:
        Formatted query results as JSON table
    """
    # Security: only allow SELECT statements
    sql_stripped = sql.strip().upper()
    if not sql_stripped.startswith("SELECT"):
        return "❌ Only SELECT queries are allowed for safety"
    
    if params is None:
        params = []
    
    try:
        with safe_db(db_path) as conn:
            cursor = conn.cursor()
            cursor.execute(sql, params)
            
            columns = [desc[0] for desc in cursor.description] if cursor.description else []
            rows = cursor.fetchmany(max_rows)
            
            if not columns:
                return "⚠️ Query returned no columns"
            
            # Format as readable table
            result = {
                "columns": columns,
                "rows": [dict(zip(columns, row)) for row in rows],
                "row_count": len(rows),
            }
            
            # Also return as readable text
            return format_table(columns, rows, len(rows))
            
    except PermissionError as e:
        return f"🚫 {str(e)}"
    except Exception as e:
        return f"❌ Query failed: {str(e)}"


def format_table(columns: list, rows: list, row_count: int) -> str:
    """Format query results as readable text table."""
    if not rows:
        return "📊 0 rows returned"
    
    # Calculate column widths
    widths = {col: len(col) for col in columns}
    for row in rows:
        for i, val in enumerate(row):
            key = columns[i]
            val_str = str(val) if val is not None else "NULL"
            widths[key] = max(widths[key], min(len(val_str), 50))
    
    # Build header
    header = " | ".join(col.ljust(widths[col]) for col in columns)
    separator = "-+-".join("-" * widths[col] for col in columns)
    
    # Build rows
    lines = [header, separator]
    for row in rows:
        line = " | ".join(
            (str(val) if val is not None else "NULL")[:50].ljust(widths[col])
            for col, val in zip(columns, row)
        )
        lines.append(line)
    
    return f"📊 {row_count} rows:\n\n" + "\n".join(lines)


@mcp.tool()
async def db_list_tables(db_path: str) -> str:
    """List all tables in a SQLite database.
    
    Args:
        db_path: Path to SQLite database file
    
    Returns:
        List of tables with row counts
    """
    try:
        with safe_db(db_path) as conn:
            cursor = conn.cursor()
            cursor.execute("""
                SELECT name FROM sqlite_master 
                WHERE type='table' AND name NOT LIKE 'sqlite_%'
                ORDER BY name
            """)
            tables = [row[0] for row in cursor.fetchall()]
            
            if not tables:
                return "📭 Database has no tables"
            
            # Get row counts
            result = ["📋 Tables:"]
            for table in tables:
                cursor.execute(f"SELECT COUNT(*) FROM \"{table}\"")
                count = cursor.fetchone()[0]
                result.append(f"  • {table} ({count} rows)")
            
            return "\n".join(result)
            
    except Exception as e:
        return f"❌ Failed to list tables: {str(e)}"


@mcp.tool()
async def db_schema(db_path: str, table: str) -> str:
    """Get schema for a specific table.
    
    Args:
        db_path: Path to SQLite database file
        table: Table name
    
    Returns:
        CREATE TABLE statement and column info
    """
    try:
        with safe_db(db_path) as conn:
            cursor = conn.cursor()
            
            # Get column info
            cursor.execute(f"PRAGMA table_info(\"{table}\")")
            columns = cursor.fetchall()
            
            if not columns:
                return f"❌ Table '{table}' not found"
            
            # Format columns
            result = [f"📐 Schema for '{table}':\n"]
            for col in columns:
                col_name = col[1]
                col_type = col[2]
                nullable = "NULL" if not col[3] else "NOT NULL"
                pk = " PK" if col[5] else ""
                result.append(f"  • {col_name}: {col_type} ({nullable}){pk}")
            
            return "\n".join(result)
            
    except Exception as e:
        return f"❌ Failed to get schema: {str(e)}"


@mcp.tool()
async def db_export_csv(
    db_path: str,
    table: str,
    output_path: Optional[str] = None,
) -> str:
    """Export a table to CSV format.
    
    Args:
        db_path: Path to SQLite database file
        table: Table name to export
        output_path: Optional output file path
    
    Returns:
        CSV content or file path
    """
    try:
        with safe_db(db_path) as conn:
            cursor = conn.cursor()
            cursor.execute(f"SELECT * FROM \"{table}\"")
            
            columns = [desc[0] for desc in cursor.description]
            rows = cursor.fetchall()
            
            output = io.StringIO()
            writer = csv.writer(output)
            writer.writerow(columns)
            writer.writerows(rows)
            
            csv_content = output.getvalue()
            
            if output_path:
                with open(output_path, "w", encoding="utf-8") as f:
                    f.write(csv_content)
                return f"✅ Exported {len(rows)} rows to {output_path}"
            else:
                return f"📄 CSV export ({len(rows)} rows):\n\n{csv_content[:5000]}"
                
    except Exception as e:
        return f"❌ Export failed: {str(e)}"


@mcp.tool()
async def db_execute_write(
    db_path: str,
    sql: str,
    params: Optional[list] = None,
) -> str:
    """Execute INSERT/UPDATE/DELETE on a SQLite database.
    
    Args:
        db_path: Path to SQLite database file
        sql: SQL statement (INSERT, UPDATE, DELETE)
        params: Optional query parameters
    
    Returns:
        Operation result with rows affected
    """
    # Security: block dangerous operations
    sql_upper = sql.strip().upper()
    allowed = any(sql_upper.startswith(op) for op in ["INSERT", "UPDATE", "DELETE", "CREATE", "DROP", "ALTER"])
    
    if not allowed:
        return "❌ Only INSERT, UPDATE, DELETE, CREATE, DROP, ALTER allowed"
    
    if params is None:
        params = []
    
    try:
        with safe_db(db_path) as conn:
            cursor = conn.cursor()
            cursor.execute(sql, params)
            conn.commit()
            
            affected = cursor.rowcount
            return f"✅ Operation completed. {affected} row(s) affected."
            
    except Exception as e:
        return f"❌ Operation failed: {str(e)}"


if __name__ == "__main__":
    if _MCP_AVAILABLE:
        utils.safe_mcp_run(mcp)
    else:
        print("Database Guild: MCP not available. Install: pip install mcp fastmcp")