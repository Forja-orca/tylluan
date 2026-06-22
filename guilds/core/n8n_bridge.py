"""
TylluanNexus n8n Bridge Guild — Workflow automation via n8n REST API.

Complements the external_mcp n8n connection (which proxies MCP tools) with
direct REST API control: list all workflows, trigger by ID or webhook URL,
poll execution status, and retrieve results.

Requires n8n running at N8N_BASE_URL (default: http://localhost:5678).
Auth: N8N_API_KEY env var (n8n Settings → API Keys).
"""

import asyncio
import json
import logging
import os
import re
import sys
import urllib.error
import urllib.parse
import urllib.request
from typing import Optional

from mcp.server.fastmcp import FastMCP
from guilds.core import utils

mcp = FastMCP("tylluan-n8n_bridge")

_BASE = os.environ.get("N8N_BASE_URL", "http://localhost:5678").rstrip("/")
_API_KEY = os.environ.get("N8N_API_KEY", "")
_WEBHOOK_BASE = os.environ.get("N8N_WEBHOOK_BASE", _BASE)


def _headers() -> dict:
    h = {"Content-Type": "application/json", "Accept": "application/json"}
    if _API_KEY:
        h["X-N8N-API-KEY"] = _API_KEY
    return h


def _get(path: str, timeout: int = 10) -> dict | list:
    url = f"{_BASE}/api/v1{path}"
    req = urllib.request.Request(url, headers=_headers(), method="GET")
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return json.loads(resp.read().decode())
    except urllib.error.HTTPError as e:
        body = e.read().decode(errors="replace")
        raise RuntimeError(f"n8n {e.code}: {body[:200]}") from e
    except Exception as e:
        raise RuntimeError(f"n8n unreachable at {_BASE}: {e}") from e


def _post(path: str, body: dict, timeout: int = 30) -> dict:
    url = f"{_BASE}/api/v1{path}"
    data = json.dumps(body).encode()
    req = urllib.request.Request(url, data=data, headers=_headers(), method="POST")
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return json.loads(resp.read().decode())
    except urllib.error.HTTPError as e:
        body_text = e.read().decode(errors="replace")
        raise RuntimeError(f"n8n {e.code}: {body_text[:200]}") from e
    except Exception as e:
        raise RuntimeError(f"n8n unreachable at {_BASE}: {e}") from e


def _post_webhook(webhook_path: str, payload: dict, timeout: int = 60) -> str:
    url = f"{_WEBHOOK_BASE}/{webhook_path.lstrip('/')}"
    data = json.dumps(payload).encode()
    req = urllib.request.Request(url, data=data, headers=_headers(), method="POST")
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            raw = resp.read().decode(errors="replace")
            return raw[:2000]
    except urllib.error.HTTPError as e:
        return f"HTTP {e.code}: {e.read().decode(errors='replace')[:300]}"
    except Exception as e:
        return f"Error: {e}"


# ── Tools ─────────────────────────────────────────────────────────────────────

@mcp.tool()
async def list_workflows(
    active_only: bool = False,
    search: str = "",
    intent: str = "",
) -> str:
    """List all n8n workflows with their IDs, names, and activation status.
    Use for: list workflows, show automations, what workflows do we have, n8n flows,
    show n8n, list automations, what can n8n do, available workflows.

    Args:
        active_only: If True, only return active (enabled) workflows.
        search: Filter workflows by name (case-insensitive).
        intent: Natural language description of what you're looking for.
    """
    try:
        data = _get("/workflows")
        workflows = data if isinstance(data, list) else data.get("data", [])

        if active_only or "active only" in (intent or "").lower():
            workflows = [w for w in workflows if w.get("active")]

        # Only filter by name when `search` is explicitly provided.
        # `intent` comes from the Tylluan router and contains routing phrases like
        # "list n8n workflows" — using it as a search filter silences all results.
        if search:
            q = search.lower()
            workflows = [w for w in workflows if q in w.get("name", "").lower()]

        if not workflows:
            return "📭 No workflows found" + (" matching your query." if search else ".")

        lines = [f"📋 **n8n Workflows** ({len(workflows)} found)\n"]
        for w in workflows[:30]:
            status = "🟢" if w.get("active") else "⚫"
            wid = w.get("id", "?")
            name = w.get("name", "Unnamed")
            updated = (w.get("updatedAt") or w.get("updated_at") or "")[:10]
            lines.append(f"{status} **{name}** (ID: `{wid}`) {updated}")
            # Show trigger type if available
            nodes = w.get("nodes", [])
            triggers = [n.get("type", "") for n in nodes if "Trigger" in n.get("type", "") or "Webhook" in n.get("type", "")]
            if triggers:
                lines.append(f"   ↳ Triggers: {', '.join(set(t.split('.')[-1] for t in triggers))}")

        if len(workflows) > 30:
            lines.append(f"\n…and {len(workflows) - 30} more.")
        return "\n".join(lines)

    except RuntimeError as e:
        return f"❌ {e}"


@mcp.tool()
async def execute_workflow(
    workflow_id: str = "",
    workflow_name: str = "",
    payload: str = "{}",
    intent: str = "",
) -> str:
    """Execute an n8n workflow by ID or name. Triggers it via the n8n API.
    Use for: run workflow, execute automation, trigger n8n, run flow, start workflow,
    ejecuta el flujo, dispara la automatización, run n8n workflow.

    Args:
        workflow_id: The n8n workflow ID (numeric string like "42").
        workflow_name: Workflow name to search and run (used if workflow_id is empty).
        payload: JSON string with input data to pass to the workflow.
        intent: Natural language description — used to infer workflow name if not provided.
    """
    try:
        # Resolve name → id
        if not workflow_id and (workflow_name or intent):
            data = _get("/workflows")
            all_wf = data if isinstance(data, list) else data.get("data", [])
            q = (workflow_name or intent).lower()
            matches = [w for w in all_wf if q in w.get("name", "").lower()]
            if not matches:
                names = [w.get("name", "") for w in all_wf[:10]]
                return f"❌ No workflow found matching '{q}'.\nAvailable: {', '.join(names)}"
            if len(matches) > 1:
                opts = [f"• **{w['name']}** (ID: {w['id']})" for w in matches[:5]]
                return f"⚠️ Multiple matches — be more specific:\n" + "\n".join(opts)
            workflow_id = str(matches[0]["id"])

        if not workflow_id:
            return "❌ Provide `workflow_id` or `workflow_name`."

        try:
            input_data = json.loads(payload) if payload.strip() else {}
        except json.JSONDecodeError:
            input_data = {"text": payload}

        result = _post(f"/workflows/{workflow_id}/run", {"runData": input_data})

        exec_id = result.get("data", {}).get("executionId") or result.get("executionId", "?")
        return (
            f"✅ **Workflow {workflow_id} triggered**\n"
            f"Execution ID: `{exec_id}`\n"
            f"Use `get_execution_status(execution_id='{exec_id}')` to poll the result."
        )

    except RuntimeError as e:
        return f"❌ {e}"


@mcp.tool()
async def kernel_pulse(intent: str = "") -> str:
    """Get a real-time snapshot of the Tylluan kernel: guilds, SilvaDB, recent memory nodes.
    Triggers the TylluanNexus Kernel Pulse n8n workflow and returns the structured report.
    Use for: kernel pulse, system pulse, tylluan status via n8n, kernel snapshot, pulse report,
    estado del kernel, pulso del sistema, n8n pulse, get kernel pulse.
    """
    result = _post_webhook("webhook/tylluan-pulse", {"source": "tylluan_do", "intent": intent}, timeout=180)
    try:
        data = json.loads(result)
        if isinstance(data, dict) and "pulse" in data:
            return data["pulse"]
    except Exception:
        pass
    return f"✅ **Kernel Pulse triggered**\n\n{result[:1000]}"


@mcp.tool()
async def trigger_webhook(
    webhook_path: str = "",
    payload: str = "{}",
    intent: str = "",
) -> str:
    """Trigger an n8n workflow via its Webhook URL path.
    Use for: call webhook, webhook trigger, POST to n8n webhook, send data to n8n,
    trigger via HTTP, call n8n endpoint.

    Args:
        webhook_path: The webhook path after /webhook/ (e.g. 'my-flow' for /webhook/my-flow).
        payload: JSON string with data to send in the request body.
        intent: Natural language description of the trigger.
    """
    if not webhook_path:
        return "❌ Provide `webhook_path` (e.g. 'tylluan-pulse')."
    try:
        try:
            data = json.loads(payload) if payload.strip() else {}
        except json.JSONDecodeError:
            data = {"text": payload}

        full_path = f"webhook/{webhook_path.lstrip('/')}"
        result = _post_webhook(full_path, data)
        return f"✅ **Webhook triggered**: `{webhook_path}`\n\nResponse:\n```\n{result}\n```"
    except Exception as e:
        return f"❌ Webhook failed: {e}"


@mcp.tool()
async def get_execution_status(
    execution_id: str,
    intent: str = "",
) -> str:
    """Get the status and result of an n8n workflow execution.
    Use for: check execution, execution status, did the workflow finish, get result,
    poll execution, execution result.

    Args:
        execution_id: The execution ID returned by execute_workflow.
        intent: Natural language description.
    """
    try:
        data = _get(f"/executions/{execution_id}")
        exec_data = data.get("data", data)

        status = exec_data.get("status", "unknown")
        finished = exec_data.get("finished", False)
        mode = exec_data.get("mode", "")
        started = (exec_data.get("startedAt") or "")[:19]
        stopped = (exec_data.get("stoppedAt") or "")[:19]

        emoji = {"success": "✅", "error": "❌", "running": "⏳", "waiting": "⏸️"}.get(status, "🔄")
        lines = [
            f"{emoji} **Execution `{execution_id}`** — {status.upper()}",
            f"Mode: {mode} | Started: {started}" + (f" | Stopped: {stopped}" if stopped else ""),
        ]

        if status == "error":
            err = exec_data.get("data", {})
            if isinstance(err, dict):
                err_msg = err.get("resultData", {}).get("error", {}).get("message", "")
                if err_msg:
                    lines.append(f"Error: {err_msg[:300]}")

        return "\n".join(lines)

    except RuntimeError as e:
        return f"❌ {e}"


@mcp.tool()
async def n8n_status(intent: str = "") -> str:
    """Check if n8n is running and return status + active workflow count.
    Use for: n8n status, is n8n running, n8n health, check n8n, n8n online.
    """
    try:
        # Check healthz first (no auth needed)
        try:
            req = urllib.request.Request(f"{_BASE}/healthz", headers=_headers(), method="GET")
            with urllib.request.urlopen(req, timeout=5) as resp:
                healthz = json.loads(resp.read().decode())
                health_ok = healthz.get("status") == "ok"
        except Exception as e:
            return f"❌ n8n unreachable: {e}"

        # Fetch workflows to check API Key and count workflows
        workflows = _get("/workflows")
        all_wf = workflows if isinstance(workflows, list) else workflows.get("data", [])
        active = sum(1 for w in all_wf if w.get("active"))
        return (
            f"✅ **n8n is running** — Status: OK\n"
            f"Workflows: {len(all_wf)} total, {active} active\n"
            f"Base URL: {_BASE}"
        )
    except RuntimeError as e:
        return f"❌ n8n API error: {e}"


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO, stream=sys.stderr)
    utils.safe_mcp_run(mcp)
