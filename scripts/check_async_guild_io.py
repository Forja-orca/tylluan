#!/usr/bin/env python3
"""check_async_guild_io.py - gate: no blocking network I/O reachable from
async guild code without asyncio.to_thread.

Contract bwc-8c0dc35a (arbitration T638, 2026-09-19): FastMCP guild servers
run every @mcp.tool() handler on ONE shared event loop. A sync network call
inside an async def stalls every concurrent tool call for its whole timeout -
llama_backend.py had local inference (urlopen timeout=120) doing exactly that
(T633/T637). The fix pattern is asyncio.to_thread, already used elsewhere in
the same file; this gate keeps the class closed mechanically.

Ratchet semantics (why a baseline exists): the gate's first run found 30
pre-existing violations of the same class in guilds the contract did not
cover (browser.py, comfy_ui.py, n8n_bridge.py). Blocking the whole team's
pushes for an inherited backlog would repeat the check_no_predation lesson,
so: violations listed in scripts/async_io_baseline.json are KNOWN (warned,
not fatal); any violation NOT in the baseline FAILS the gate. Baseline
entries that stop matching are stale (the violation got fixed) - reported
for removal, not fatal. Baseline keys are file :: async_func :: target so
they survive unrelated line drift; editing the baseline to silence a NEW
violation is visible in the diff and must cite a Coloquio decision.

What it checks (single-module analysis):
  1. A direct blocking call (urlopen / time.sleep / socket.create_connection /
     requests verbs) lexically inside an `async def` body, not awaited
     (coroutines are exempt) -> violation.
  2. A call from an `async def` to a module-level sync function whose
     transitive call graph reaches a blocking call -> violation.
  3. Exemptions: functions referenced as the first argument of
     asyncio.to_thread(...) - their body runs in a worker thread, including
     nested defs defined inside the async function.

Known limits, documented honestly (same transparency contract as the
non-predation gate):
  - No cross-module/import resolution: a sync helper in ANOTHER module that
    does network I/O is not tracked. Guild tools are self-contained enough
    that this covers the real bug class; extend if a cross-module case appears.
  - Class methods are not tracked (module-level functions only).
  - A sync helper called BOTH via to_thread and directly is flagged on the
    direct edge - that is exactly the T633 bug shape (_dpc_messages was
    protected on one path and naked on another).
  - subprocess blocking calls are out of scope (network I/O only, per the
    contract); extend BLOCKING detection if that class bites.

Usage:
  python scripts/check_async_guild_io.py                # self-test + scan + baseline check
  python scripts/check_async_guild_io.py --no-self-test # scan only (fast inner loop)
  python scripts/check_async_guild_io.py --write-baseline
      # print the current findings as baseline JSON (paste into
      # scripts/async_io_baseline.json only with a Coloquio decision to do so)
"""

import ast
import json
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SCAN_ROOT = REPO_ROOT / "guilds"
BASELINE_PATH = REPO_ROOT / "scripts" / "async_io_baseline.json"

REQUESTS_VERBS = {"get", "post", "put", "delete", "head", "options", "patch", "request"}
SAFE_WRAPPERS = {"to_thread", "run_in_executor", "run_in_threadpool"}


def dotted_name(call):
    """Dotted path of a call's callee, e.g. 'urllib.request.urlopen' or
    '_urllib.urlopen'. None when not a simple Name/Attribute chain."""
    parts = []
    node = call.func
    while isinstance(node, ast.Attribute):
        parts.append(node.attr)
        node = node.value
    if isinstance(node, ast.Name):
        parts.append(node.id)
        return ".".join(reversed(parts))
    return None


def is_blocking_call(dotted):
    if dotted is None:
        return False
    parts = dotted.split(".")
    last = parts[-1]
    if last == "urlopen":  # urllib.request.urlopen, alias.urlopen, bare urlopen
        return True
    if last == "create_connection":  # socket.create_connection
        return True
    if last == "sleep" and (dotted == "sleep" or parts[-2] == "time"):
        return True  # time.sleep / bare from-import sleep - blocks the loop.
        # asyncio.sleep is a coroutine and never matches (parts[-2] == "asyncio").
    if parts[0] == "requests" and last in REQUESTS_VERBS:
        return True
    return False


def is_safe_wrapper(dotted):
    return dotted is not None and dotted.split(".")[-1] in SAFE_WRAPPERS


def to_threaded_names(func_node):
    """Function names referenced as the first arg of asyncio.to_thread(...)
    anywhere inside this function."""
    names = set()
    for sub in ast.walk(func_node):
        if isinstance(sub, ast.Call) and is_safe_wrapper(dotted_name(sub)):
            if sub.args and isinstance(sub.args[0], ast.Name):
                names.add(sub.args[0].id)
    return names


def iter_calls_excluding_threaded(func_node):
    """Yield Call nodes in this function's body, skipping the bodies of nested
    defs that are referenced by asyncio.to_thread(...) (they run in threads)."""
    threaded = to_threaded_names(func_node)
    stack = [iter(ast.iter_child_nodes(func_node))]
    while stack:
        try:
            child = next(stack[-1])
        except StopIteration:
            stack.pop()
            continue
        if isinstance(child, ast.FunctionDef) and child.name in threaded:
            continue
        if isinstance(child, ast.Call):
            yield child
        stack.append(iter(ast.iter_child_nodes(child)))


def module_functions(tree):
    """Module-level function defs: name -> (node, is_async)."""
    out = {}
    for node in tree.body:
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            out[node.name] = (node, isinstance(node, ast.AsyncFunctionDef))
    return out


def transitive_io(funcs):
    """Names of module-level SYNC functions whose transitive call graph
    reaches a blocking call (thread-exempt nested defs excluded)."""
    direct = {}
    edges = {}
    for name, (node, is_async) in funcs.items():
        if is_async:
            continue
        direct[name] = False
        edges[name] = set()
        for sub in iter_calls_excluding_threaded(node):
            d = dotted_name(sub)
            if is_blocking_call(d):
                direct[name] = True
            base = d.split(".")[-1] if d else None
            if base in funcs and base != name:
                edges[name].add(base)
    io = dict(direct)
    changed = True
    while changed:
        changed = False
        for name in io:
            if io[name]:
                continue
            if any(io.get(callee) for callee in edges[name]):
                io[name] = True
                changed = True
    return io


def scan_async_func(node, io, relpath, findings):
    """Walk an async def body. Nested sync defs are descended UNLESS their
    name is referenced by a to_thread(...) call (they run in a thread).
    Awaited calls are coroutines - exempt, but their arguments are still
    scanned (a sync call inside the args still runs on the loop)."""
    threaded = to_threaded_names(node)

    def visit(n, inside_nested, awaited=False):
        for child in ast.iter_child_nodes(n):
            if isinstance(child, ast.Await):
                visit(child, inside_nested, awaited=True)
                continue
            if isinstance(child, ast.FunctionDef):
                if child.name in threaded:
                    continue  # body runs in a worker thread - exempt
                visit(child, inside_nested=child.name)
                continue
            if isinstance(child, ast.Call):
                if awaited:
                    visit(child, inside_nested)  # coroutine call - scan args only
                    continue
                d = dotted_name(child)
                base = d.split(".")[-1] if d else None
                if is_blocking_call(d):
                    findings.append({
                        "file": relpath,
                        "line": child.lineno,
                        "func": node.name,
                        "target": d,
                        "kind": "direct",
                        "nested": inside_nested,
                    })
                elif base in io and io[base]:
                    findings.append({
                        "file": relpath,
                        "line": child.lineno,
                        "func": node.name,
                        "target": base + "()",
                        "kind": "transitive",
                        "nested": inside_nested,
                    })
            visit(child, inside_nested, awaited=False)

    visit(node, inside_nested=None)


def finding_key(f):
    """Stable baseline key: file :: async_func :: target (line numbers drift
    with unrelated edits; the semantic triple does not)."""
    return f"{f['file']} :: {f['func']} :: {f['target']}"


def render_finding(f):
    where = f" (nested def '{f['nested']}')" if f["nested"] else ""
    if f["kind"] == "direct":
        return (f"{f['file']}:{f['line']}: blocking call '{f['target']}' inside "
                f"async '{f['func']}'{where} without asyncio.to_thread")
    return (f"{f['file']}:{f['line']}: async '{f['func']}' calls sync "
            f"'{f['target']}' which transitively performs blocking network I/O "
            f"{where}- wrap the call in asyncio.to_thread")


def scan_file(path, findings):
    try:
        relpath = path.relative_to(REPO_ROOT).as_posix()
    except ValueError:  # fixture outside the repo (self-test)
        relpath = path.name
    try:
        tree = ast.parse(path.read_bytes(), filename=str(path))
    except SyntaxError as exc:
        findings.append({
            "file": relpath, "line": 0, "func": "<module>",
            "target": f"SYNTAX ERROR: {exc}", "kind": "syntax", "nested": None,
        })
        return
    funcs = module_functions(tree)
    io = transitive_io(funcs)
    for name, (node, is_async) in funcs.items():
        if is_async:
            scan_async_func(node, io, relpath, findings)


def load_baseline():
    if not BASELINE_PATH.exists():
        return set()
    try:
        data = json.loads(BASELINE_PATH.read_text(encoding="utf-8"))
        return set(data.get("known", []))
    except (json.JSONDecodeError, OSError) as exc:
        print(f"[async-io-gate] WARNING: unreadable baseline ({exc}) - treating as empty")
        return set()


def scan_guilds():
    findings = []
    files = sorted(SCAN_ROOT.rglob("*.py"))
    for path in files:
        scan_file(path, findings)
    return files, findings


def run_gate(quiet=False):
    files, findings = scan_guilds()
    known = load_baseline()
    current_keys = {finding_key(f) for f in findings}
    new = [f for f in findings if finding_key(f) not in known]
    stale = sorted(known - current_keys)

    if not quiet or findings:
        print(f"[async-io-gate] scanned {len(files)} files under guilds/")
    for f in findings:
        if finding_key(f) in known:
            print(f"[async-io-gate] KNOWN: {render_finding(f)}")
        else:
            print(f"[async-io-gate] VIOLATION: {render_finding(f)}")
    for key in stale:
        print(f"[async-io-gate] STALE BASELINE ENTRY (violation fixed; remove it): {key}")

    if new:
        print(
            f"[async-io-gate] FAIL: {len(new)} NEW violation(s) not in the baseline. "
            "Fix pattern: asyncio.to_thread (see guilds/core/llama_backend.py, "
            "contract bwc-8c0dc35a). Adding to the baseline requires a Coloquio decision."
        )
        return 1
    if not quiet and not findings:
        print("[async-io-gate] clean: no blocking network I/O reachable from async guild code")
    elif findings and not new:
        print(
            f"[async-io-gate] {len(findings)} known violation(s) (baseline) - "
            "gate passes; cleanup tracked separately."
        )
    return 0


# - self-test: the gate must catch the bug class it exists for -
FIXTURES_PASS = {
    # to_thread at the call site (the standard fix)
    "good_threaded": (
        "import asyncio\n"
        "def g():\n"
        "    import urllib.request\n"
        "    return urllib.request.urlopen('http://x', timeout=2)\n"
        "async def f():\n"
        "    return await asyncio.to_thread(g)\n"
    ),
    # nested def passed to to_thread (the bwc-8c0dc35a helper pattern)
    "good_nested_threaded": (
        "import asyncio, urllib.request\n"
        "async def f():\n"
        "    def h():\n"
        "        return urllib.request.urlopen('http://x', timeout=2)\n"
        "    return await asyncio.to_thread(h)\n"
    ),
    # awaited coroutines are NOT blocking: asyncio.sleep must stay exempt
    "good_awaited_coroutine": (
        "import asyncio\n"
        "async def f():\n"
        "    await asyncio.sleep(1)\n"
        "    d = {}\n"
        "    return d.get('k')  # .get() must not trip the requests-verb rule\n"
    ),
}
FIXTURES_FAIL = {
    # direct urlopen in async body (T633 shape, 120s inference)
    "bad_direct": (
        "import urllib.request\n"
        "async def f():\n"
        "    return urllib.request.urlopen('http://x', timeout=120)\n"
    ),
    # transitive: async -> sync helper -> urlopen (the _dpc_messages shape)
    "bad_transitive": (
        "import urllib.request\n"
        "def g():\n"
        "    return urllib.request.urlopen('http://x', timeout=3)\n"
        "async def f():\n"
        "    return g()\n"
    ),
    # nested def NOT passed to to_thread, called on the loop
    "bad_nested_direct": (
        "import urllib.request\n"
        "async def f():\n"
        "    def h():\n"
        "        return urllib.request.urlopen('http://x', timeout=2)\n"
        "    return h()\n"
    ),
    # time.sleep on the loop
    "bad_sleep": (
        "import time\n"
        "async def f():\n"
        "    time.sleep(30)\n"
    ),
    # sync helper called BOTH via to_thread and directly: the direct edge
    # is the violation (exactly how T633 looked after a partial fix)
    "bad_partial_fix": (
        "import asyncio, urllib.request\n"
        "def g():\n"
        "    return urllib.request.urlopen('http://x', timeout=3)\n"
        "async def f(ok):\n"
        "    if ok:\n"
        "        return await asyncio.to_thread(g)\n"
        "    return g()\n"
    ),
}


def self_test():
    import tempfile

    failures = []
    with tempfile.TemporaryDirectory() as td:
        td_path = Path(td)
        for name, src in FIXTURES_PASS.items():
            f = td_path / f"{name}.py"
            f.write_text(src)
            found = []
            scan_file(f, found)
            if found:
                failures.append(f"{name}: expected clean, got {[render_finding(x) for x in found]}")
        for name, src in FIXTURES_FAIL.items():
            f = td_path / f"{name}.py"
            f.write_text(src)
            found = []
            scan_file(f, found)
            if not found:
                failures.append(f"{name}: expected a violation, got none")
    if failures:
        print("[async-io-gate] SELF-TEST FAILED:")
        for line in failures:
            print(f"  - {line}")
        return False
    print(
        f"[async-io-gate] self-test OK: {len(FIXTURES_PASS)} clean fixtures pass, "
        f"{len(FIXTURES_FAIL)} violation fixtures detected"
    )
    return True


def main(argv):
    quiet = "--quiet" in argv
    if "--write-baseline" in argv:
        _, findings = scan_guilds()
        payload = {
            "_comment": (
                "Known async-blocking-I/O violations at gate landing (bwc-8c0dc35a, "
                "2026-09-19). Gate FAILS on findings not listed here; WARNS on stale "
                "entries (violation fixed - remove the entry). Adding entries requires "
                "a Coloquio decision. Key: file :: async_func :: target."
            ),
            "known": sorted(finding_key(f) for f in findings),
        }
        print(json.dumps(payload, indent=2))
        return 0
    if "--no-self-test" not in argv and not self_test():
        return 2
    return run_gate(quiet=quiet)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
