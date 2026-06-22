"""
TylluanNexus Audit Guild — Local audit logging and security monitoring.

This guild provides tools for logging tool calls, tracking security events,
and maintaining an audit trail. All local - no external dependencies.
"""

import os
import json
import sqlite3
from datetime import datetime
from pathlib import Path
from typing import Optional, List, Dict
from mcp.server.fastmcp import FastMCP
from guilds.builders.plugins import utils

mcp = FastMCP("tylluan-audit")

DB_PATH = "./data/audit.db"


def get_db() -> sqlite3.Connection:
    """Get or create audit database."""
    os.makedirs("./data", exist_ok=True)
    conn = sqlite3.connect(DB_PATH)
    conn.execute("""
        CREATE TABLE IF NOT EXISTS audit_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            session_id TEXT,
            tool_name TEXT NOT NULL,
            channel TEXT,
            user_agent TEXT,
            success INTEGER,
            error_message TEXT,
            arguments TEXT,
            result_preview TEXT
        )
    """)
    conn.execute("""
        CREATE INDEX IF NOT EXISTS idx_timestamp ON audit_log(timestamp)
    """)
    conn.execute("""
        CREATE INDEX IF NOT EXISTS idx_tool ON audit_log(tool_name)
    """)
    return conn


@mcp.tool()
async def log_tool_call(
    tool_name: str,
    session_id: str = "local",
    channel: str = "stdio",
    success: bool = True,
    error_message: Optional[str] = None,
    arguments: Optional[str] = None,
    result_preview: Optional[str] = None,
) -> str:
    """Log a tool call to the audit trail.
    
    Args:
        tool_name: Name of the tool being called.
        session_id: Session identifier.
        channel: Communication channel (stdio, http, sse).
        success: Whether the call succeeded.
        error_message: Error message if failed.
        arguments: Tool arguments (JSON string).
        result_preview: Preview of the result.
    
    Returns:
        Confirmation message.
    """
    try:
        conn = get_db()
        cursor = conn.cursor()
        
        timestamp = datetime.utcnow().isoformat()
        
        cursor.execute("""
            INSERT INTO audit_log 
            (timestamp, session_id, tool_name, channel, success, error_message, arguments, result_preview)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        """, (
            timestamp,
            session_id,
            tool_name,
            channel,
            1 if success else 0,
            error_message,
            arguments[:1000] if arguments else None,  # Truncate long args
            result_preview[:500] if result_preview else None,  # Truncate long results
        ))
        
        conn.commit()
        conn.close()
        
        return f"✅ Logged: {tool_name} ({'OK' if success else 'FAIL'})"
        
    except Exception as e:
        return f"❌ Audit log failed: {str(e)}"


@mcp.tool()
async def get_audit_log(
    limit: int = 50,
    tool_name: Optional[str] = None,
    session_id: Optional[str] = None,
) -> str:
    """Retrieve audit log entries.
    
    Args:
        limit: Maximum number of entries to return.
        tool_name: Filter by tool name.
        session_id: Filter by session.
    
    Returns:
        Formatted audit log.
    """
    try:
        conn = get_db()
        cursor = conn.cursor()
        
        query = "SELECT * FROM audit_log WHERE 1=1"
        params = []
        
        if tool_name:
            query += " AND tool_name = ?"
            params.append(tool_name)
        
        if session_id:
            query += " AND session_id = ?"
            params.append(session_id)
        
        query += " ORDER BY timestamp DESC LIMIT ?"
        params.append(limit)
        
        cursor.execute(query, params)
        rows = cursor.fetchall()
        conn.close()
        
        if not rows:
            return "📋 No audit entries found"
        
        lines = ["📋 Audit Log (last {})".format(limit), "=" * 50, ""]
        
        for row in rows:
            ts, sid, tool, channel, success = row[1], row[2], row[3], row[4], row[5]
            icon = "✅" if success else "❌"
            lines.append(f"{icon} {ts[:19]} | {tool} | {channel} | {sid[:12]}...")
        
        return "\n".join(lines)
        
    except Exception as e:
        return f"❌ Failed to retrieve audit log: {str(e)}"


@mcp.tool()
async def get_security_events(
    hours: int = 24,
    min_level: str = "medium",
) -> str:
    """Get security-relevant events.
    
    Args:
        hours: Time window in hours.
        min_level: Minimum severity level (low, medium, high, critical).
    
    Returns:
        Security events summary.
    """
    try:
        conn = get_db()
        cursor = conn.cursor()
        
        # Get failed tool calls (potential security issues)
        cursor.execute("""
            SELECT tool_name, COUNT(*) as count, MAX(timestamp) as last
            FROM audit_log
            WHERE success = 0
            AND timestamp > datetime('now', '-{} hours')
            GROUP BY tool_name
            ORDER BY count DESC
        """.format(hours), ())
        
        failures = cursor.fetchall()
        
        # Get dangerous tool usage
        dangerous_tools = ["bash_execute", "file_write", "docker_run", "system_exec"]
        cursor.execute("""
            SELECT tool_name, COUNT(*) as count
            FROM audit_log
            WHERE success = 1
            AND timestamp > datetime('now', '-{} hours')
            AND tool_name IN ({})
            GROUP BY tool_name
        """.format(hours, ",".join("?" * len(dangerous_tools))), dangerous_tools)
        
        dangerous = cursor.fetchall()
        
        conn.close()
        
        lines = ["🛡️ Security Events (last {}h)".format(hours), "=" * 50, ""]
        
        if failures:
            lines.append("⚠️ Failed Tool Calls (potential issues):")
            for tool, count, last in failures:
                lines.append(f"  • {tool}: {count} failures (last: {last[:19]})")
            lines.append("")
        
        if dangerous:
            lines.append("🔐 Dangerous Tool Usage:")
            for tool, count in dangerous:
                lines.append(f"  • {tool}: {count} calls")
            lines.append("")
        
        if not failures and not dangerous:
            lines.append("✅ No security events in the last {} hours".format(hours))
        
        return "\n".join(lines)
        
    except Exception as e:
        return f"❌ Failed to get security events: {str(e)}"


@mcp.tool()
async def get_audit_stats() -> str:
    """Get audit statistics and system status.

    Use for: audit system, system audit, check guilds, which guilds are running,
    system health, running status, status check, what is running, show stats,
    audit summary, tool usage summary, guild inventory, inspect guilds inventory,
    show guild inventory, system inventory, inspect inventory, list guild status.

    Returns:
        Summary statistics including tool usage and success rates.
    """
    try:
        conn = get_db()
        cursor = conn.cursor()
        
        # Total entries
        cursor.execute("SELECT COUNT(*) FROM audit_log")
        total = cursor.fetchone()[0]
        
        # Today's entries
        cursor.execute("SELECT COUNT(*) FROM audit_log WHERE date(timestamp) = date('now')")
        today = cursor.fetchone()[0]
        
        # Success rate
        cursor.execute("SELECT COUNT(*) FROM audit_log WHERE success = 1")
        success = cursor.fetchone()[0]
        rate = (success / total * 100) if total > 0 else 0
        
        # Top tools
        cursor.execute("""
            SELECT tool_name, COUNT(*) as count
            FROM audit_log
            GROUP BY tool_name
            ORDER BY count DESC
            LIMIT 5
        """)
        top_tools = cursor.fetchall()
        
        conn.close()
        
        lines = ["📊 Audit Statistics", "=" * 40, ""]
        lines.append(f"Total entries: {total}")
        lines.append(f"Today's entries: {today}")
        lines.append(f"Success rate: {rate:.1f}%")
        lines.append("")
        lines.append("Top tools:")
        for tool, count in top_tools:
            lines.append(f"  • {tool}: {count}")
        
        return "\n".join(lines)
        
    except Exception as e:
        return f"❌ Failed to get stats: {str(e)}"


@mcp.tool()
async def clear_audit_log(before_days: Optional[int] = None, intent: str = "", confirm: bool = False) -> str:
    """Clear/delete/wipe old log entries. DESTRUCTIVE OPERATION.

    ONLY use when explicitly asked to 'clear', 'delete', 'wipe', or 'purge' the log.
    NEVER use for status checks, listing, or stats retrieval.

    Args:
        before_days: Delete entries older than N days. If None, clears all.
        intent: Natural language intent for safety check.
        confirm: Explicit confirmation flag.

    Returns:
        Confirmation message.
    """
    destructive_words = [
        "clear", "delete", "wipe", "remove", "purge", "erase", "reset",
        "borrar", "limpiar", "eliminar", "vaciar", "resetear"
    ]
    # Double lock: requires at least 2 destructive words to confirm via intent
    matches = [w for w in destructive_words if w in intent.lower()]
    if not confirm and len(matches) < 2:
        return "⚠️ Safety guard: to clear the audit log, explicitly include at least TWO destructive words (e.g., 'clear and wipe') in your request, or set confirm=True."

    try:
        conn = get_db()
        cursor = conn.cursor()
        
        if before_days:
            cursor.execute("""
                DELETE FROM audit_log 
                WHERE timestamp < datetime('now', '-{} days')
            """.format(before_days), ())
            msg = f"🗑️ Deleted entries older than {before_days} days"
        else:
            cursor.execute("DELETE FROM audit_log")
            msg = "🗑️ Cleared all audit log entries"
        
        deleted = cursor.rowcount
        conn.commit()
        conn.close()
        
        return f"{msg} ({deleted} entries removed)"
        
    except Exception as e:
        return f"❌ Failed to clear log: {str(e)}"


if __name__ == "__main__":
    utils.safe_mcp_run(mcp)