"""TRINITY Coordinator Guild — Thinker/Worker/Verifier for multi-step tasks.

Execution model:
  - Tasks that don't reference prior context run in parallel (ThreadPoolExecutor).
  - Tasks that reference prior output ("it", "the result", synthesis verbs) run sequentially.
  - Consecutive independent tasks are batched and dispatched simultaneously.
"""
import json
import re
import urllib.request
import urllib.error
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from mcp.server.fastmcp import FastMCP

mcp = FastMCP("coordinator")

MAX_TASKS = 5
TASK_TIMEOUT = 150  # per-task timeout in seconds; Heavy guild = 180s total

# Context-reference words that force a task to wait for the previous result
_CTX_REFS_PATTERN = re.compile(
    r'\b(?:it|the\s+result|that|eso|el\s+resultado|them|those|ese\s+resultado)\b',
    re.IGNORECASE,
)


def _resolve_kernel_url() -> str:
    import os
    if "KERNEL_BASE" in os.environ:
        return os.environ["KERNEL_BASE"]
    port_file = Path(__file__).resolve().parent.parent.parent / "data" / "active_port.json"
    try:
        data = json.loads(port_file.read_text())
        port = data.get("port", 3030)
        return f"http://127.0.0.1:{port}"
    except Exception:
        return "http://127.0.0.1:3030"


KERNEL_URL = _resolve_kernel_url()


# ── Thinker ──────────────────────────────────────────────────────────────────

def _split_intent(intent: str) -> list[str]:
    """Split a multi-step intent into ordered sub-tasks (rule-based, no LLM)."""
    connectors = r"\s+(?:then|and then|after that|finally|y luego|luego|despues|finalmente)\s+"
    parts = re.split(connectors, intent, flags=re.IGNORECASE)
    if len(parts) > 1:
        return [p.strip() for p in parts if p.strip()][:MAX_TASKS]
    numbered = re.split(r"\s*\d+\.\s+", intent)
    numbered = [p.strip() for p in numbered if p.strip()]
    if len(numbered) > 1:
        return numbered[:MAX_TASKS]
    return [intent.strip()]


def _needs_prior_context(task: str) -> bool:
    """True if this task explicitly references the output of a previous step."""
    return bool(_CTX_REFS_PATTERN.search(task)) or _is_synthesis_intent(task)


def _plan(tasks: list[str]) -> list[dict]:
    """Return an execution plan: each entry is {'type': 'parallel'|'sequential', 'tasks': [...]}."""
    plan: list[dict] = []
    parallel_batch: list[tuple[int, str]] = []

    def flush_batch():
        if parallel_batch:
            plan.append({"type": "parallel", "tasks": list(parallel_batch)})
            parallel_batch.clear()

    for i, task in enumerate(tasks):
        if _needs_prior_context(task):
            flush_batch()
            plan.append({"type": "sequential", "tasks": [(i, task)]})
        else:
            parallel_batch.append((i, task))

    flush_batch()
    return plan


# ── Worker ────────────────────────────────────────────────────────────────────

import http.client
import threading
from urllib.parse import urlparse

_THREAD_LOCAL = threading.local()

import socket

def _get_http_connection(url: str):
    if not hasattr(_THREAD_LOCAL, "conn") or _THREAD_LOCAL.conn is None:
        parsed = urlparse(url)
        conn = http.client.HTTPConnection(parsed.netloc, timeout=TASK_TIMEOUT)
        try:
            conn.connect()
            conn.sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        except Exception:
            pass
        _THREAD_LOCAL.conn = conn
    return _THREAD_LOCAL.conn

def _dispatch(sub_intent: str, agent_id: str = "coordinator-worker") -> str:
    """Send a sub-task to the kernel via POST /api/v1/do using HTTP Keep-Alive (thread-safe)."""
    payload = json.dumps({"intent": sub_intent, "agent_id": agent_id}).encode()
    headers = {
        "Content-Type": "application/json",
        "Connection": "keep-alive"
    }
    try:
        conn = _get_http_connection(KERNEL_URL)
        conn.request("POST", "/api/v1/do", body=payload, headers=headers)
        resp = conn.getresponse()
        data = resp.read()
        body = json.loads(data.decode())
        return body.get("result") or body.get("output") or json.dumps(body)
    except Exception as e:
        # Connection could be closed/stale, reset and retry once
        _THREAD_LOCAL.conn = None
        try:
            conn = _get_http_connection(KERNEL_URL)
            conn.request("POST", "/api/v1/do", body=payload, headers=headers)
            resp = conn.getresponse()
            data = resp.read()
            body = json.loads(data.decode())
            return body.get("result") or body.get("output") or json.dumps(body)
        except Exception as retry_err:
            return f"❌ dispatch error: {retry_err}"


def _dispatch_with_retry(idx: int, task: str, agent_id: str) -> tuple[int, str, str]:
    """Dispatch a task, retry once on failure. Returns (original_index, task, result)."""
    result = _dispatch(task, agent_id)
    if _is_failure(result):
        # Do not retry on known permanent infrastructure errors to save network time
        if "Unknown guild" in result or "Failed to start guild" in result or "not found" in result.lower():
            return idx, task, result
        result = _dispatch(f"retry: {task}", agent_id)
        if _is_failure(result):
            result = f"⚠️ [step {idx + 1} failed after retry]"
    return idx, task, result


# ── Verifier ──────────────────────────────────────────────────────────────────

def _is_failure(result: str) -> bool:
    if not result or not result.strip():
        return True
    lowered = result.lower()
    return "❌" in result or lowered.startswith("error") or '"error"' in lowered


def _is_synthesis_intent(intent: str) -> bool:
    """True if the intent synthesises, aggregates, or summarises prior results."""
    lowered = intent.lower().strip()
    signals = [
        "synthesize", "synthesise", "synthesis",
        "summarize", "summarise", "summary", "sum up", "count",
        "explain", "describe", "analyze", "tell me",
        "generate", "produce", "create",
        "combine", "merge", "unify", "consolidate", "collect results", "gather results",
        "wrap up", "conclude", "finalize",
        "put it together", "put together",
        "list them", "list the", "list all",
        "show the", "show them", "show names", "print", "display",
        # Spanish
        "generar resumen", "resumir", "sintetizar",
        "combinar", "unificar", "consolidar",
        "concluir", "finalizar",
        "dame un resumen", "resume todo",
        "contar", "lista", "listar", "explicar", "describir", "analizar",
        "mostrar", "imprimir",
    ]
    return any(s in lowered for s in signals)


def _should_parallelize_batch(tasks_batch: list[tuple[int, str]]) -> bool:
    """Decide if a batch of independent tasks should run in parallel via ThreadPoolExecutor.
    Runs in parallel if there is more than 1 independent task in the batch.
    """
    return len(tasks_batch) > 1


# ── Main tool ─────────────────────────────────────────────────────────────────

@mcp.tool()
def coordinate(intent: str, agent_id: str = "coordinator") -> str:
    """
    Orchestrate complex multi-step tasks using Thinker/Worker/Verifier.
    Independent sub-tasks execute in parallel; context-dependent tasks execute sequentially.
    Use for: multi-step tasks, research then implement, do X then Y,
    first do A then do B, plan and execute,
    complex workflows, sequential tasks, chained operations,
    primero A luego B, haz X y luego Y, tarea por pasos.
    """
    tasks = _split_intent(intent)
    n = len(tasks)



    # results[i] = (task_str, result_str) indexed by original task position
    results: dict[int, tuple[str, str]] = {}
    prev_result = ""

    plan = _plan(tasks)

    for step in plan:
        if step["type"] == "parallel" and len(step["tasks"]) > 1:
            if _should_parallelize_batch(step["tasks"]):
                # Run independent heavy tasks concurrently
                with ThreadPoolExecutor(max_workers=min(len(step["tasks"]), 4)) as pool:
                    futures = {
                        pool.submit(_dispatch_with_retry, idx, task, agent_id): (idx, task)
                        for idx, task in step["tasks"]
                    }
                    for future in as_completed(futures):
                        idx, task, result = future.result()
                        results[idx] = (task, result)
                        prev_result = result  # last written wins
            else:
                # Run lightweight tasks sequentially in the main thread (reuses connection)
                for idx, task in step["tasks"]:
                    _, _, result = _dispatch_with_retry(idx, task, agent_id)
                    results[idx] = (task, result)
                    prev_result = result

        elif step["type"] == "parallel" and len(step["tasks"]) == 1:
            # Single independent task — no need for executor overhead
            idx, task = step["tasks"][0]
            _, task, result = _dispatch_with_retry(idx, task, agent_id)
            results[idx] = (task, result)
            prev_result = result

        else:
            # Sequential: single task that needs prior context
            idx, task = step["tasks"][0]
            if prev_result and _CTX_REFS_PATTERN.search(task):
                ctx_snippet = prev_result[:200].replace("\n", " ")
                task_with_ctx = f"{task} [context: {ctx_snippet}]"
            else:
                task_with_ctx = task

            if _is_synthesis_intent(task) and prev_result:
                result = f"[Synthesis]\n{prev_result[:500]}"
            else:
                _, _, result = _dispatch_with_retry(idx, task_with_ctx, agent_id)

            results[idx] = (task, result)
            prev_result = result

    lines = []
    for i in range(n):
        task, res = results[i]
        lines.append(f"## Step {i + 1}/{n} — {task}\n{res}")
    lines.append(f"\n---\nCoordinator completed {n}/{n} steps.")
    return "\n\n".join(lines)


if __name__ == "__main__":
    from guilds.core import utils
    utils.safe_mcp_run(mcp)
