"""TRINITY Coordinator Guild — Thinker/Worker/Verifier for multi-step tasks."""
import json
import urllib.request
import urllib.error
from mcp.server.fastmcp import FastMCP

mcp = FastMCP("coordinator")

KERNEL_URL = "http://127.0.0.1:3030"
MAX_TASKS = 3
TASK_TIMEOUT = 120


# ── Thinker ──────────────────────────────────────────────────────────────────

def _split_intent(intent: str) -> list[str]:
    """Split a multi-step intent into ordered sub-tasks (rule-based, no LLM)."""
    import re
    connectors = r"\s+(?:then|and then|after that|finally|y luego|luego|despues|finalmente)\s+"
    parts = re.split(connectors, intent, flags=re.IGNORECASE)
    if len(parts) > 1:
        return [p.strip() for p in parts if p.strip()][:MAX_TASKS]
    numbered = re.split(r"\s*\d+\.\s+", intent)
    numbered = [p.strip() for p in numbered if p.strip()]
    if len(numbered) > 1:
        return numbered[:MAX_TASKS]
    return [intent.strip()]


# ── Worker ────────────────────────────────────────────────────────────────────

def _dispatch(sub_intent: str, agent_id: str) -> str:
    """Send a sub-task to the kernel via POST /api/v1/do."""
    payload = json.dumps({"intent": sub_intent, "agent_id": agent_id}).encode()
    req = urllib.request.Request(
        f"{KERNEL_URL}/api/v1/do",
        data=payload,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=TASK_TIMEOUT) as resp:
            body = json.loads(resp.read())
            return body.get("result") or body.get("output") or json.dumps(body)
    except urllib.error.HTTPError as e:
        return f"\u274c HTTP {e.code}: {e.reason}"
    except Exception as e:
        return f"\u274c dispatch error: {e}"


# ── Verifier ──────────────────────────────────────────────────────────────────

def _is_failure(result: str) -> bool:
    """Return True if the result is a structural failure."""
    if not result or not result.strip():
        return True
    lowered = result.lower()
    return "\u274c" in result or lowered.startswith("error") or '"error"' in lowered


# ── Main tool ─────────────────────────────────────────────────────────────────

@mcp.tool()
def coordinate(intent: str, agent_id: str = "coordinator") -> str:
    """
    Orchestrate complex multi-step tasks using Thinker/Worker/Verifier.
    Use for: multi-step tasks, research then implement, do X then Y,
    first do A then do B, plan and execute,
    complex workflows, sequential tasks, chained operations,
    primero A luego B, haz X y luego Y, tarea por pasos.
    """
    tasks = _split_intent(intent)
    n = len(tasks)
    results = []
    prev_result = ""

    for i, task in enumerate(tasks):
        if prev_result and any(w in task.lower() for w in ("it", "the result", "that", "eso", "el resultado")):
            ctx_snippet = prev_result[:200].replace("\n", " ")
            task_with_ctx = f"{task} [context: {ctx_snippet}]"
        else:
            task_with_ctx = task

        result = _dispatch(task_with_ctx, agent_id)

        if _is_failure(result):
            result = _dispatch(f"retry: {task}", agent_id)
            if _is_failure(result):
                result = f"\u26a0\ufe0f [step {i+1} failed after retry]"

        results.append((task, result))
        prev_result = result

    lines = []
    for idx, (task, res) in enumerate(results, 1):
        lines.append(f"## Step {idx}/{n} \u2014 {task}\n{res}")
    lines.append(f"\n---\nCoordinator completed {n}/{n} steps.")
    return "\n\n".join(lines)
