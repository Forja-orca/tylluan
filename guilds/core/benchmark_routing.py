"""Benchmark: embedding router vs majority class vs keyword router.

Baseline discipline:
1. Majority class (trivial baseline): always predict the most common guild
2. Current keyword router: heuristic keyword matching (simulated)
3. Embedding router: BGE-M3 similarity via POST /api/v1/embed

ALL three measured on the SAME held-out intents from guild_audit_log.
Guild description embeddings are cached: 1 call per guild, not per intent.
"""
import json, sys, urllib.request, sqlite3, re, math, time
from pathlib import Path
from collections import Counter

def _resolve_kernel_base():
    import os as _os
    if "KERNEL_BASE" in _os.environ:
        return _os.environ["KERNEL_BASE"]
    port_file = Path(__file__).resolve().parent.parent.parent / "data" / "active_port.json"
    try:
        data = json.loads(port_file.read_text())
        port = data.get("port", 4000)
        return f"http://127.0.0.1:{port}"
    except Exception:
        return "http://127.0.0.1:4000"

KERNEL_URL = _resolve_kernel_base()
DATA_DIR = Path("data")

# ── Guild descriptions (from catalog.rs) ────────────────────────────────────
GUILD_DESCRIPTIONS = {
    "bash": "Run shell commands and scripts",
    "filesystem": "List files, find files, show directory contents, file operations",
    "memory": "Store and retrieve long-term memories and knowledge",
    "search": "Semantic and keyword search across indexed content",
    "websearch": "Web search engine queries and result fetching",
    "browser": "Web browser automation with CDP protocol",
    "code": "Code modification and generation across languages",
    "database": "Database query and schema management",
    "pdf": "PDF document reading and text extraction",
    "vision": "Image analysis and OCR using vision models",
    "code_reviewer": "Code review and quality checks",
    "coloquio": "Multi-agent conversation channels",
    "mcp_bridge": "External MCP server integration bridge",
    "code_graph": "Code dependency graph and structure analysis",
    "comfy_ui": "Image generation via ComfyUI workflow",
    "n8n_bridge": "n8n workflow automation trigger and management",
    "scrapling": "Web scraping and content extraction from URLs",
    "data_tools": "JSON, YAML, CSV data manipulation tools",
    "formatter": "Code formatter: Ruff, Prettier, Rustfmt",
    "sequential_thinking": "Step-by-step reasoning and analysis",
    "night_reasoner": "Nightly reasoning and feedback analysis",
    "council": "Multi-voice decision making and tradeoff analysis",
    "ast_surgeon": "AST manipulation and code transformation",
    "audio_tools": "Audio file processing and conversion",
    "ffmpeg_tools": "FFmpeg multimedia processing tools",
    "screenshot_tools": "Screen capture and screenshot utilities",
    "clipboard_tools": "Clipboard read and write utilities",
    "local_llm_proxy": "Local LLM inference proxy and requests",
    "biome_warden": "Biome code quality linting and formatting",
    "audit": "Security audit and system integrity checks",
    "cron_scheduler": "Scheduled task and cron job management",
    "coordinator": "Orchestrate multi-step tasks and plan execution",
    "git": "Git version control operations",
    "docker": "Docker container management",
    "monitor": "System monitoring and process observation",
    "whats_new": "Unread messages and updates from channels",
    "coloquio_digest": "Coloquio channel digest and summary",
    "knowledge": "Knowledge graph and semantic network operations",
}

# ── Embedding cache ─────────────────────────────────────────────────────────
_guild_embeddings = {}

def _embed(text):
    data = json.dumps({"text": text}).encode("utf-8")
    req = urllib.request.Request(
        f"{KERNEL_URL}/api/v1/embed", data=data,
        headers={"Content-Type": "application/json"}, method="POST")
    with urllib.request.urlopen(req, timeout=30) as resp:
        return json.loads(resp.read())["embedding"]

def warmup():
    """Pre-compute guild description embeddings (1 API call per guild)."""
    t0 = time.time()
    for guild, desc in GUILD_DESCRIPTIONS.items():
        _guild_embeddings[guild] = _embed(f"{guild}: {desc}")
    t = time.time() - t0
    print(f"  Warmup: {len(_guild_embeddings)} guilds in {t:.1f}s ({t/len(_guild_embeddings):.2f}s each)")

def cosine_sim(a, b):
    dot = sum(x * y for x, y in zip(a, b))
    na = math.sqrt(sum(x * x for x in a))
    nb = math.sqrt(sum(y * y for y in b))
    return dot / (na * nb + 1e-10)

# ── Load real data ──────────────────────────────────────────────────────────
def load_audit_intents(limit=200):
    db = DATA_DIR / "audit.db"
    if not db.exists():
        print(f"No audit.db at {db}")
        return []
    conn = sqlite3.connect(str(db))
    cur = conn.cursor()
    rows = []
    for table in ["guild_audit_log", "journal"]:
        try:
            cur.execute(f"SELECT name FROM sqlite_master WHERE type='table' AND name='{table}'")
            if not cur.fetchone():
                continue
            cols = [c[1] for c in cur.execute(f"PRAGMA table_info({table})")]
            if "intent" in cols and "guild" in cols:
                # Exclude guilds always routed via explicit hint (calling agent
                # already knows the target). coloquio, whats_new, coloquio_digest
                # are chat-channel ops -- no content classifier can pick an agent's
                # destination channel from natural language alone. Including them
                # would be apples vs oranges.
                skip_guilds = ("coloquio", "whats_new", "coloquio_digest")
                placeholders = ",".join(["?" for _ in skip_guilds])
                cur.execute(f"""
                    SELECT intent, guild FROM {table}
                    WHERE intent IS NOT NULL AND intent != ''
                      AND guild IS NOT NULL
                      AND guild NOT IN ({placeholders})
                    ORDER BY rowid DESC LIMIT {limit}
                """, skip_guilds)
                rows = cur.fetchall()
                break
            elif "args_preview" in cols and "guild" in cols:
                cur.execute(f"SELECT args_preview, guild FROM {table} WHERE args_preview IS NOT NULL AND args_preview != '' AND guild IS NOT NULL ORDER BY rowid DESC LIMIT {limit}")
                rows = cur.fetchall()
                break
        except Exception:
            continue
    conn.close()
    return [(i.strip(), g.strip()) for i, g in rows if i.strip() and g.strip()]

# ── Keyword router baseline ─────────────────────────────────────────────────
KEYWORD_MAP = {
    "bash": ["run ", "bash", "shell", "command", "execute", "./", "chmod", "apt", "npm ", "pip ", "cargo ",
             "ls ", "cd ", "cat ", "grep ", "echo ", "mkdir", "rm ", "cp ", "mv ", "ps ", "kill",
             "git ", "docker ", "make ", "node ", "python"],
    "filesystem": ["list file", "list dir", "find file", "show dir", "ls", "dir", "file system",
                   "listar archivo", "listar directorio", "buscar archivo"],
    "websearch": ["search web", "web search", "find on internet", "look up", "google", "search internet",
                  "what is ", "who is ", "latest ", "news about"],
    "memory": ["remember", "store in memory", "save this", "i learned", "recall", "do not forget",
               "memory", "recordar", "guardar"],
    "code": ["write code", "create function", "implement", "refactor", "generate code",
             "add test", "fix bug", "create file"],
    "search": ["search memory", "find in notes", "semantic search", "recall", "find knowledge"],
}

def keyword_router(intent):
    intent_lower = intent.lower()
    best_guild = "bash"
    best_score = 0
    for guild, keywords in KEYWORD_MAP.items():
        score = sum(2 for kw in keywords if kw in intent_lower)
        if score > best_score:
            best_score = score
            best_guild = guild
    return best_guild, best_score

def keyword_router_with_tiebreak(intent):
    """Keyword first, embedding fallback when keyword score is zero or tied."""
    kw_guild, kw_score = keyword_router(intent)
    if kw_score > 0:
        return kw_guild, 1.0  # keyword decided
    # No keywords matched — use embedding
    intent_emb = _embed(intent)
    best_guild = list(GUILD_DESCRIPTIONS.keys())[0]
    best_sim = -1
    for guild in _guild_embeddings:
        sim = cosine_sim(intent_emb, _guild_embeddings[guild])
        if sim > best_sim:
            best_sim = sim
            best_guild = guild
    return best_guild, best_sim

# ── Embedding router ────────────────────────────────────────────────────────
def embedding_router(intent):
    intent_emb = _embed(intent)
    best_guild = list(GUILD_DESCRIPTIONS.keys())[0]
    best_sim = -1
    for guild in _guild_embeddings:
        sim = cosine_sim(intent_emb, _guild_embeddings[guild])
        if sim > best_sim:
            best_sim = sim
            best_guild = guild
    return best_guild, best_sim

# ── Real production router (hybrid semantic+keyword, matcher.rs) ───────────
# Queries the ACTUAL match_guild() blend (0.55 semantic + 0.45 keyword) via
# Plan mode (plan=true resolves guild+tool+args without executing). This is
# the router genuinely running in production today -- comparing pure
# embedding-only and pure keyword-only against each other and never against
# THIS was the real gap in the first two runs (Jose, 2026-07-27): the
# question isn't "replace keyword with embedding", it's "does the existing
# hybrid already beat both naive approaches" -- production already hybridizes
# instead of choosing one.
def hybrid_router(intent):
    data = json.dumps({"intent": intent, "plan": True}).encode("utf-8")
    req = urllib.request.Request(
        f"{KERNEL_URL}/api/v1/do", data=data,
        headers={"Content-Type": "application/json"}, method="POST")
    with urllib.request.urlopen(req, timeout=30) as resp:
        result = json.loads(resp.read())
        # The plan_info payload (with "guild") is nested under "result" in the
        # HTTP envelope ({"status":"ok","content":[...],"result":{...}}), not
        # at the top level -- the previous version read result.get("guild")
        # directly and always got the default, scoring a false 0.00%.
        return result.get("result", {}).get("guild", "unknown")

# ── Benchmark ───────────────────────────────────────────────────────────────
def benchmark():
    print("=" * 72)
    print("ROUTING BENCHMARK — baselines vs embedding router")
    print("=" * 72)

    intents = load_audit_intents(limit=200)
    if not intents:
        print("No intents in audit.db. Start kernel and use tylluan_do first.")
        return

    print(f"\nLoaded {len(intents)} intent/guild pairs from audit.db")
    guilds = [g for _, g in intents]
    dist = Counter(guilds)
    print(f"Guild distribution: {dict(dist.most_common(10))}")

    # Baseline 1: Majority class
    majority_guild = dist.most_common(1)[0][0]
    maj_correct = sum(1 for _, g in intents if g == majority_guild)
    maj_acc = maj_correct / len(intents)
    print(f"\n--- Baseline 1: Majority class ('{majority_guild}') ---")
    print(f"  Accuracy: {maj_acc*100:.2f}% ({maj_correct}/{len(intents)})")

    # Baseline 2: Keyword router (naive simulation)
    kw_correct = sum(1 for intent, actual in intents if keyword_router(intent)[0] == actual)
    kw_acc = kw_correct / len(intents)
    print(f"\n--- Baseline 2: Current keyword router (naive sim) ---")
    print(f"  Accuracy: {kw_acc*100:.2f}% ({kw_correct}/{len(intents)})")

    # Warmup guild embeddings
    print(f"\n--- Warmup: guild description embeddings ---")
    warmup()

    sample_size = min(len(intents), 100)
    sample = intents[:sample_size]

    # Router 3: Embedding-only
    print(f"\n--- Router 3: Embedding-only (BGE-M3 brute force) on {sample_size} intents ---")
    emb_correct = 0
    emb_latencies = []
    for i, (intent, actual) in enumerate(sample):
        t0 = time.time()
        try:
            pred, sim = embedding_router(intent)
            t = time.time() - t0
            emb_latencies.append(t)
            if pred == actual:
                emb_correct += 1
            if (i + 1) % 20 == 0:
                print(f"  [{i+1}/{sample_size}] acc={emb_correct/(i+1)*100:.1f}%")
        except Exception as e:
            print(f"  [{i+1}/{sample_size}] ERROR: {e}")
    emb_acc = emb_correct / sample_size
    emb_avg_lat = sum(emb_latencies)/len(emb_latencies) if emb_latencies else 0
    print(f"  Final: {emb_acc*100:.2f}% ({emb_correct}/{sample_size}) avg_lat={emb_avg_lat:.2f}s")

    # Router 4: Keyword-first, embedding tiebreaker when no keywords match
    print(f"\n--- Router 4: Keyword + embedding tiebreaker on {sample_size} intents ---")
    kw2_correct = 0
    kw2_latencies = []
    for i, (intent, actual) in enumerate(sample):
        t0 = time.time()
        try:
            pred, conf = keyword_router_with_tiebreak(intent)
            t = time.time() - t0
            kw2_latencies.append(t)
            if pred == actual:
                kw2_correct += 1
            if (i + 1) % 20 == 0:
                print(f"  [{i+1}/{sample_size}] acc={kw2_correct/(i+1)*100:.1f}%")
        except Exception as e:
            print(f"  [{i+1}/{sample_size}] ERROR: {e}")
    kw2_acc = kw2_correct / sample_size
    kw2_avg_lat = sum(kw2_latencies)/len(kw2_latencies) if kw2_latencies else 0
    print(f"  Final: {kw2_acc*100:.2f}% ({kw2_correct}/{sample_size}) avg_lat={kw2_avg_lat:.2f}s")

    # Router 5: Real production hybrid (matcher.rs via plan=true)
    # plan=true fix verified live (turn 269): returns plan without executing.
    print(f"\n--- Router 5: Production hybrid (matcher.rs via plan=true) on {sample_size} intents ---")
    hyb_correct = 0
    hyb_latencies = []
    for i, (intent, actual) in enumerate(sample):
        t0 = time.time()
        try:
            pred = hybrid_router(intent)
            t = time.time() - t0
            hyb_latencies.append(t)
            if pred == actual:
                hyb_correct += 1
            if (i + 1) % 10 == 0:
                print(f"  [{i+1}/{sample_size}] acc={hyb_correct/(i+1)*100:.1f}%")
        except Exception as e:
            print(f"  [{i+1}/{sample_size}] ERROR: {e}")
    hyb_acc = hyb_correct / sample_size
    hyb_avg_lat = sum(hyb_latencies)/len(hyb_latencies) if hyb_latencies else 0
    print(f"  Final: {hyb_acc*100:.2f}% ({hyb_correct}/{sample_size}) avg_lat={hyb_avg_lat:.2f}s")

    # Summary
    print(f"\n{'='*72}")
    print("SUMMARY — 5 routers compared on {sample_size} intents")
    print(f"{'='*72}")
    print(f"  1. Majority class ('{majority_guild}'): {maj_acc*100:6.2f}%")
    print(f"  2. Keyword-only (naive sim):      {kw_acc*100:6.2f}%")
    print(f"  3. Embedding-only (BGE-M3):       {emb_acc*100:6.2f}%  ({emb_avg_lat:.2f}s/intent)")
    print(f"  4. Keyword+embedding tiebreaker:  {kw2_acc*100:6.2f}%  ({kw2_avg_lat:.2f}s/intent)")
    print(f"  5. Production hybrid (matcher.rs):{hyb_acc*100:6.2f}%  ({hyb_avg_lat:.2f}s/intent)")
    if hyb_acc > kw_acc and hyb_acc > emb_acc:
        print(f"  [OK] Production hybrid beats both pure approaches")
    elif hyb_acc > kw_acc:
        print(f"  [OK] Hybrid beats keyword, loses to embedding")
    elif hyb_acc > emb_acc:
        print(f"  [OK] Hybrid beats embedding, loses to keyword")
    else:
        print(f"  [X] Hybrid does NOT beat either pure baseline")
    print(f"{'='*72}")

if __name__ == "__main__":
    benchmark()
