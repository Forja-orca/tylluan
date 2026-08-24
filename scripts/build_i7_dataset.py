#!/usr/bin/env python3
"""
Task I-7: Curated Routing Dataset Generator and Validator.
Constructs a comprehensive, grounded dataset of intents across all 45+ guilds,
integrating real logs from data/audit.db with expert-curated cross-guild ambiguity
and semantic paraphrase test cases for rigorous J-13 (embedding tiebreaker) evaluation.
"""

import json
import sqlite3
import random
from pathlib import Path
from collections import Counter

OUTPUT_JSON = Path("benchmarks/dataset_i7_routing_curated.json")
AUDIT_DB = Path("data/audit.db")

# Complete catalogue of 45 guilds categorized
GUILD_CATALOG = {
    # Builders
    "bash": {"category": "builder", "desc": "Execute shell commands, scripts, system binaries"},
    "filesystem": {"category": "builder", "desc": "List, read, write, copy, move, delete files and directories"},
    "git": {"category": "builder", "desc": "Git version control, commits, branches, diffs, log"},
    "docker": {"category": "builder", "desc": "Docker container lifecycle, images, logs, compose"},
    "database": {"category": "builder", "desc": "SQL queries, SQLite, Postgres, schema inspections"},
    "code": {"category": "builder", "desc": "Modify, generate, and edit source code files"},
    "formatter": {"category": "builder", "desc": "Format code files with Ruff, Prettier, Rustfmt"},
    "ast_surgeon": {"category": "builder", "desc": "AST parsing, node transformations, syntax tree refactoring"},
    "code_graph": {"category": "builder", "desc": "Dependency graphs, symbol call trees, module hierarchy"},
    "code_analysis": {"category": "builder", "desc": "Static code analysis, complexity metrics, dead code detection"},
    "code_reviewer": {"category": "builder", "desc": "Automated code review, security smell and bug detection"},
    "biome_warden": {"category": "builder", "desc": "Biome linter, fast JS/TS code checking and formatting"},
    "n8n_bridge": {"category": "builder", "desc": "Trigger and manage n8n automation workflows and webhooks"},
    "mcp_bridge": {"category": "builder", "desc": "Bridge to external Model Context Protocol tool servers"},

    # Scholars
    "search": {"category": "scholar", "desc": "Hybrid search across indexed codebase and documents"},
    "websearch": {"category": "scholar", "desc": "Web search queries via Google/DuckDuckGo API"},
    "browser": {"category": "scholar", "desc": "Headless browser automation, click, type, scrape with CDP"},
    "scrapling": {"category": "scholar", "desc": "Undetected web scraping and HTML content extraction"},
    "deep_web_research": {"category": "scholar", "desc": "Multi-hop web search synthesis and paper extraction"},
    "deep_analysis": {"category": "scholar", "desc": "In-depth document analysis, summarization and theme extraction"},
    "pdf": {"category": "scholar", "desc": "Extract text, tables, and metadata from PDF files"},
    "data_tools": {"category": "scholar", "desc": "Parse, transform, query JSON, YAML, CSV, Parquet data"},
    "knowledge": {"category": "scholar", "desc": "Query and traverse SilvaDB knowledge graph and triples"},
    "ingest": {"category": "scholar", "desc": "Ingest and chunk raw documents into SilvaDB"},

    # Watchers & Systems
    "monitor": {"category": "watcher", "desc": "Observe running processes, system health, and alerts"},
    "system_metrics": {"category": "watcher", "desc": "Inspect CPU, RAM, disk, network utilization"},
    "audit": {"category": "watcher", "desc": "Security audit, token leak detection, permission checks"},
    "cron_scheduler": {"category": "watcher", "desc": "Schedule, list, and cancel recurring cron jobs"},
    "whats_new": {"category": "watcher", "desc": "Check unread notifications, channel updates, and diffs"},
    "clipboard_tools": {"category": "watcher", "desc": "Read from or write text into the system clipboard"},
    "screenshot_tools": {"category": "watcher", "desc": "Capture screenshot of the screen or active window"},

    # Multimedia & Perception
    "vision": {"category": "scholar", "desc": "General image analysis, OCR, visual question answering"},
    "comfy_ui": {"category": "builder", "desc": "Generate images via local ComfyUI Stable Diffusion workflow"},
    "audio_tools": {"category": "scholar", "desc": "Process, convert, transcribe audio files and spectrograms"},
    "ffmpeg_tools": {"category": "builder", "desc": "Video and audio slicing, encoding, transcode via ffmpeg"},

    # Cognitive & Coordination (Core)
    "memory": {"category": "core", "desc": "Store, recall, and manage sovereign long-term memory"},
    "coloquio": {"category": "core", "desc": "Send and read messages in multi-agent Coloquio channels"},
    "coloquio_digest": {"category": "core", "desc": "Generate executive digests of Coloquio conversations"},
    "coordinator": {"category": "core", "desc": "Decompose complex multi-step tasks and orchestrate sub-agents"},
    "council": {"category": "core", "desc": "Multi-agent deliberative debate and consensus synthesis"},
    "sequential_thinking": {"category": "core", "desc": "Structured chain-of-thought and step-by-step reasoning"},
    "night_reasoner": {"category": "core", "desc": "Nightly memory consolidation, pattern abstraction, dream cycle"},
    "llama_backend": {"category": "core", "desc": "Direct GGUF inference via local llama-server"},
    "local_llm_proxy": {"category": "core", "desc": "Proxy requests to external Ollama, LM Studio, or vLLM"}
}

# Curated synthetic cases designed to test:
# 1. Clear keyword match
# 2. Pure semantic paraphrase (zero common keywords)
# 3. Cross-guild ambiguity (ambiguous cases where J-13 tiebreaker is required)
# 4. Multi-step / Coordinator triggers
# 5. Negative distractor / Anti-keyword cases
CURATED_EXEMPLARS = [
    # --- Bash vs Filesystem vs Git vs Code ---
    {"intent": "execute `cargo check --lib` and show warnings", "target_guild": "bash", "ambiguity_type": "clear_keyword"},
    {"intent": "spawn a background process to tail the server logs", "target_guild": "bash", "ambiguity_type": "semantic_paraphrase"},
    {"intent": "haz un kill -9 al proceso que está ocupando el puerto 4000", "target_guild": "bash", "ambiguity_type": "semantic_paraphrase"},
    {"intent": "what files are located inside the crates/tylluan-kernel/src directory", "target_guild": "filesystem", "ambiguity_type": "clear_keyword"},
    {"intent": "dime cuántos archivos .rs hay en este proyecto", "target_guild": "filesystem", "ambiguity_type": "semantic_paraphrase"},
    {"intent": "check if the configuration file tylluan.toml exists on disk", "target_guild": "filesystem", "ambiguity_type": "cross_guild_ambiguity"},
    {"intent": "stage all modified files and commit with message 'fix: router edge case'", "target_guild": "git", "ambiguity_type": "clear_keyword"},
    {"intent": "show the commit history for the last 48 hours on branch main", "target_guild": "git", "ambiguity_type": "clear_keyword"},
    {"intent": "¿cuál fue el último commit que tocó el archivo schema.rs?", "target_guild": "git", "ambiguity_type": "cross_guild_ambiguity"},
    {"intent": "create a new branch called feature/vector-tiering", "target_guild": "git", "ambiguity_type": "clear_keyword"},

    # --- Code vs AST vs Code Reviewer vs Formatter vs Biome ---
    {"intent": "refactor the handle_call function to return Result instead of Option", "target_guild": "code", "ambiguity_type": "clear_keyword"},
    {"intent": "add a unit test covering empty node arrays in search.rs", "target_guild": "code", "ambiguity_type": "clear_keyword"},
    {"intent": "analyze the AST structure to find all public structs in this file", "target_guild": "ast_surgeon", "ambiguity_type": "clear_keyword"},
    {"intent": "rename the parameter `target_guild` across all function call nodes in the syntax tree", "target_guild": "ast_surgeon", "ambiguity_type": "semantic_paraphrase"},
    {"intent": "review this pull request for potential concurrency deadlocks and race conditions", "target_guild": "code_reviewer", "ambiguity_type": "clear_keyword"},
    {"intent": "haz una auditoría de calidad al nuevo código de consensus.rs buscando vulnerabilidades", "target_guild": "code_reviewer", "ambiguity_type": "cross_guild_ambiguity"},
    {"intent": "run prettier and rustfmt on the modified frontend and backend files", "target_guild": "formatter", "ambiguity_type": "clear_keyword"},
    {"intent": "formatea este archivo TypeScript según las reglas de estilo del proyecto", "target_guild": "formatter", "ambiguity_type": "clear_keyword"},
    {"intent": "run biome check to lint our React TypeScript components in dashboard/src", "target_guild": "biome_warden", "ambiguity_type": "clear_keyword"},
    {"intent": "encuentra dependencias circulares en los módulos del crate tylluan-kernel", "target_guild": "code_graph", "ambiguity_type": "semantic_paraphrase"},
    {"intent": "generate the function call hierarchy for the embedding pipeline", "target_guild": "code_graph", "ambiguity_type": "semantic_paraphrase"},
    {"intent": "calculate cyclomatic complexity and lines of dead code in handler_do.rs", "target_guild": "code_analysis", "ambiguity_type": "clear_keyword"},

    # --- Docker vs Database vs n8n ---
    {"intent": "list all running docker containers and check container health status", "target_guild": "docker", "ambiguity_type": "clear_keyword"},
    {"intent": "rebuild the Dockerfile image with tag tylluan:latest", "target_guild": "docker", "ambiguity_type": "clear_keyword"},
    {"intent": "run an EXPLAIN QUERY PLAN on the SELECT * FROM nodes query", "target_guild": "database", "ambiguity_type": "clear_keyword"},
    {"intent": "inspecciona el esquema de la tabla node_embeddings en data/silva.db", "target_guild": "database", "ambiguity_type": "cross_guild_ambiguity"},
    {"intent": "trigger the automated webhook workflow on our n8n server", "target_guild": "n8n_bridge", "ambiguity_type": "clear_keyword"},
    {"intent": "call external mcp tool `fetch_web_snapshot` from chrome-devtools server", "target_guild": "mcp_bridge", "ambiguity_type": "clear_keyword"},

    # --- WebSearch vs Browser vs Scrapling vs Deep Research ---
    {"intent": "search the web for latest benchmarks on BGE-M3 embedding models in 2026", "target_guild": "websearch", "ambiguity_type": "clear_keyword"},
    {"intent": "¿qué novedades hay sobre el protocolo MCP de Anthropic esta semana?", "target_guild": "websearch", "ambiguity_type": "semantic_paraphrase"},
    {"intent": "navigate to https://news.ycombinator.com and click on the top submission", "target_guild": "browser", "ambiguity_type": "clear_keyword"},
    {"intent": "abre la página de inicio de sesión y toma una captura de pantalla del botón", "target_guild": "browser", "ambiguity_type": "cross_guild_ambiguity"},
    {"intent": "scrape the full text article from this URL bypassing cloudflare bot protection", "target_guild": "scrapling", "ambiguity_type": "clear_keyword"},
    {"intent": "extrae el contenido limpio en markdown de https://example.com/blog/post-1", "target_guild": "scrapling", "ambiguity_type": "semantic_paraphrase"},
    {"intent": "perform deep research across multiple scientific sources on synaptic decay", "target_guild": "deep_web_research", "ambiguity_type": "clear_keyword"},
    {"intent": "investiga a fondo el estado del arte de la memoria de agentes en arXiv y compila un reporte", "target_guild": "deep_web_research", "ambiguity_type": "clear_keyword"},

    # --- PDF vs Data Tools vs Deep Analysis vs Ingest ---
    {"intent": "extract text, tables, and references from the attached paper.pdf", "target_guild": "pdf", "ambiguity_type": "clear_keyword"},
    {"intent": "lee el archivo PDF de la especificación y resume la sección 4", "target_guild": "pdf", "ambiguity_type": "clear_keyword"},
    {"intent": "parse this 50MB JSON file and filter out records where status != 'active'", "target_guild": "data_tools", "ambiguity_type": "clear_keyword"},
    {"intent": "convierte este archivo CSV a formato Parquet comprimido con zstd", "target_guild": "data_tools", "ambiguity_type": "clear_keyword"},
    {"intent": "perform comprehensive thematic clustering on this 100-page meeting transcript", "target_guild": "deep_analysis", "ambiguity_type": "clear_keyword"},
    {"intent": "analiza este documento largo y extrae los 5 temas principales con citas textuales", "target_guild": "deep_analysis", "ambiguity_type": "semantic_paraphrase"},
    {"intent": "ingest and chunk the entire docs/ directory into SilvaDB knowledge base", "target_guild": "ingest", "ambiguity_type": "clear_keyword"},

    # --- Vision & Multimedia ---
    {"intent": "describe what is shown in this UI mockup screenshot image.png", "target_guild": "vision", "ambiguity_type": "clear_keyword"},
    {"intent": "extrae el texto de este recibo escaneado usando OCR", "target_guild": "vision", "ambiguity_type": "cross_guild_ambiguity"},
    {"intent": "generate a futuristic cybernetic owl illustration via ComfyUI SDXL", "target_guild": "comfy_ui", "ambiguity_type": "clear_keyword"},
    {"intent": "transcribe speech from audio.mp3 to timestamped subtitles", "target_guild": "audio_tools", "ambiguity_type": "clear_keyword"},
    {"intent": "trim the first 30 seconds of video.mp4 and convert to 1080p webm", "target_guild": "ffmpeg_tools", "ambiguity_type": "clear_keyword"},
    {"intent": "recorta este archivo de vídeo y extrae el canal de audio a wav", "target_guild": "ffmpeg_tools", "ambiguity_type": "cross_guild_ambiguity"},
    {"intent": "capture a full-resolution screenshot of the primary monitor", "target_guild": "screenshot_tools", "ambiguity_type": "clear_keyword"},
    {"intent": "read the current text contents of the system clipboard", "target_guild": "clipboard_tools", "ambiguity_type": "clear_keyword"},
    {"intent": "copia este texto al portapapeles del sistema operativo", "target_guild": "clipboard_tools", "ambiguity_type": "clear_keyword"},

    # --- Watchers, Metrics, Security ---
    {"intent": "check current CPU temperature, RAM usage, and available disk space", "target_guild": "system_metrics", "ambiguity_type": "clear_keyword"},
    {"intent": "¿cuánta memoria RAM libre le queda a la máquina?", "target_guild": "system_metrics", "ambiguity_type": "semantic_paraphrase"},
    {"intent": "watch background kernel processes and alert if any task crashes", "target_guild": "monitor", "ambiguity_type": "clear_keyword"},
    {"intent": "audit codebase for hardcoded API keys, bearer tokens, or plain credentials", "target_guild": "audit", "ambiguity_type": "clear_keyword"},
    {"intent": "verifica que no haya secretos expuestos en los archivos de configuración", "target_guild": "audit", "ambiguity_type": "semantic_paraphrase"},
    {"intent": "schedule a recurring backup cron job every day at 03:00 UTC", "target_guild": "cron_scheduler", "ambiguity_type": "clear_keyword"},
    {"intent": "cancela el cron job con ID backup-daily", "target_guild": "cron_scheduler", "ambiguity_type": "clear_keyword"},
    {"intent": "check if there are any unread messages or notifications in channels", "target_guild": "coloquio", "ambiguity_type": "cross_guild_ambiguity"},

    # --- Memory & Knowledge (SilvaDB) ---
    {"intent": "save this key fact: 'Tylluan listens on port 4000 without proxy'", "target_guild": "memory", "ambiguity_type": "clear_keyword"},
    {"intent": "recuerda que José prefiere commits pequeños y verificados con tests", "target_guild": "memory", "ambiguity_type": "semantic_paraphrase"},
    {"intent": "retrieve all stored memories regarding SQLite schema migrations", "target_guild": "memory", "ambiguity_type": "clear_keyword"},
    {"intent": "¿qué aprendimos la semana pasada sobre el bug de desempate en el router?", "target_guild": "memory", "ambiguity_type": "semantic_paraphrase"},
    {"intent": "find all graph triples connecting entity 'SilvaDB' with 'PageRank'", "target_guild": "knowledge", "ambiguity_type": "clear_keyword"},
    {"intent": "calcula la centralidad de grado de los nodos en el grafo de conocimiento", "target_guild": "knowledge", "ambiguity_type": "cross_guild_ambiguity"},
    {"intent": "search across indexed documentation for ADR-012 lifecycle specification", "target_guild": "search", "ambiguity_type": "clear_keyword"},

    # --- Multi-Agent & Coloquio ---
    {"intent": "post a message to channel #mision-activa informing team that CI is green", "target_guild": "coloquio", "ambiguity_type": "clear_keyword"},
    {"intent": "publica en coloquio equipo: 'Fase 1 completada con 685 tests pasando'", "target_guild": "coloquio", "ambiguity_type": "clear_keyword"},
    {"intent": "generate an executive summary digest of yesterday's debate in #general", "target_guild": "coloquio_digest", "ambiguity_type": "clear_keyword"},
    {"intent": "resume las decisiones tomadas en el canal mision-activa en los últimos 20 turnos", "target_guild": "coloquio_digest", "ambiguity_type": "semantic_paraphrase"},
    {"intent": "break down this complex refactoring into 4 parallel tasks and assign to sub-agents", "target_guild": "coordinator", "ambiguity_type": "clear_keyword"},
    {"intent": "convene a council debate between Claude and Deep to evaluate SQLite triggers vs app logic", "target_guild": "night_reasoner", "ambiguity_type": "cross_guild_ambiguity"},
    {"intent": "think step-by-step through the failure modes of the distributed gossip loop", "target_guild": "sequential_thinking", "ambiguity_type": "clear_keyword"},
    {"intent": "ejecuta una cadena de razonamiento secuencial para resolver este problema de concurrencia", "target_guild": "sequential_thinking", "ambiguity_type": "clear_keyword"},
    {"intent": "trigger the night consolidation cycle to cluster memories and resolve contradictions", "target_guild": "night_reasoner", "ambiguity_type": "clear_keyword"},

    # --- Local LLM & Proxy ---
    {"intent": "generate response using local smollm2 GGUF model via llama.cpp", "target_guild": "llama_backend", "ambiguity_type": "clear_keyword"},
    {"intent": "forward this prompt to our local Ollama instance running on port 11434", "target_guild": "local_llm_proxy", "ambiguity_type": "clear_keyword"},
    {"intent": "evalúa la coherencia de este párrafo con el modelo local en segundo plano", "target_guild": "llama_backend", "ambiguity_type": "cross_guild_ambiguity"},

    # --- Cross-Guild Ambiguities (Where J-13 Semantic Tiebreaker shines) ---
    {"intent": "search git commit history to find when the memory decay was introduced", "target_guild": "git", "ambiguity_type": "cross_guild_ambiguity"},
    {"intent": "write code to parse pdf documents and extract tables into sqlite database", "target_guild": "code", "ambiguity_type": "cross_guild_ambiguity"},
    {"intent": "list all files modified in git that haven't been committed yet", "target_guild": "git", "ambiguity_type": "cross_guild_ambiguity"},
    {"intent": "search web for how to format rust code with rustfmt in github actions", "target_guild": "websearch", "ambiguity_type": "cross_guild_ambiguity"},
    {"intent": "extract data from invoice.pdf and save as json file on filesystem", "target_guild": "pdf", "ambiguity_type": "cross_guild_ambiguity"},
    {"intent": "audit docker container configuration for insecure root privileges", "target_guild": "audit", "ambiguity_type": "cross_guild_ambiguity"},
    {"intent": "search the knowledge graph for memories related to docker deployment", "target_guild": "knowledge", "ambiguity_type": "cross_guild_ambiguity"},
    {"intent": "capture screenshot of the webpage and analyze visual elements", "target_guild": "browser", "ambiguity_type": "cross_guild_ambiguity"},
    {"intent": "read the latest 10 messages in coloquio and extract action items into a task list", "target_guild": "coloquio_digest", "ambiguity_type": "cross_guild_ambiguity"},
    {"intent": "run benchmark script and calculate memory and CPU utilization", "target_guild": "system_metrics", "ambiguity_type": "cross_guild_ambiguity"}
]

def load_real_audit_intents():
    """Extract real historical intents from data/audit.db with clean normalization."""
    if not AUDIT_DB.exists():
        return []
    
    conn = sqlite3.connect(AUDIT_DB)
    cur = conn.cursor()
    real_items = []
    
    # Try guild_audit_log
    try:
        cur.execute("""
            SELECT intent, guild FROM guild_audit_log
            WHERE intent IS NOT NULL AND intent != ''
              AND guild IS NOT NULL AND guild != ''
              AND guild NOT IN ('kernel', 'seed_tools')
            ORDER BY id DESC
        """)
        for intent, guild in cur.fetchall():
            intent_clean = intent.strip()
            guild_clean = guild.strip().lower()
            if len(intent_clean) >= 6 and guild_clean in GUILD_CATALOG:
                real_items.append({
                    "intent": intent_clean,
                    "target_guild": guild_clean,
                    "source": "audit_log",
                    "ambiguity_type": "historical_real"
                })
    except Exception as e:
        print(f"Warning: Could not read guild_audit_log: {e}")
        
    conn.close()
    return real_items

def build_curated_dataset():
    print(f"Building I-7 Curated Routing Dataset...")
    
    # 1. Start with curated exemplars
    dataset = []
    seen_intents = set()
    
    for item in CURATED_EXEMPLARS:
        norm = item["intent"].strip().lower()
        if norm not in seen_intents:
            seen_intents.add(norm)
            guild = item["target_guild"]
            meta = GUILD_CATALOG.get(guild, {"category": "core", "desc": ""})
            dataset.append({
                "id": f"i7_curated_{len(dataset)+1:04d}",
                "intent": item["intent"],
                "target_guild": guild,
                "category": meta["category"],
                "ambiguity_type": item["ambiguity_type"],
                "source": "expert_curated"
            })
            
    print(f"  Added {len(dataset)} expert-curated benchmark items.")
    
    # 2. Add deduplicated real historical audit intents
    real_items = load_real_audit_intents()
    real_added = 0
    
    # Stratified cap per guild to avoid coloquio/bash skewing everything
    guild_counts = Counter()
    for item in dataset:
        guild_counts[item["target_guild"]] += 1
        
    for item in real_items:
        norm = item["intent"].strip().lower()
        guild = item["target_guild"]
        # Limit historical items to max 8 per guild to maintain class balance
        if norm not in seen_intents and guild_counts[guild] < 12:
            seen_intents.add(norm)
            guild_counts[guild] += 1
            meta = GUILD_CATALOG.get(guild, {"category": "core", "desc": ""})
            dataset.append({
                "id": f"i7_audit_{len(dataset)+1:04d}",
                "intent": item["intent"],
                "target_guild": guild,
                "category": meta["category"],
                "ambiguity_type": "historical_real",
                "source": "guild_audit_log"
            })
            real_added += 1
            
    print(f"  Added {real_added} balanced real historical items from data/audit.db.")
    
    # 3. Deterministic Train (60%) / Held-Out (40%) Split with fixed random seed
    random.seed(42)
    random.shuffle(dataset)
    
    train_count = int(len(dataset) * 0.60)
    for i, item in enumerate(dataset):
        item["split"] = "train" if i < train_count else "held_out"
        
    # Validation metrics
    held_out = [d for d in dataset if d["split"] == "held_out"]
    train = [d for d in dataset if d["split"] == "train"]
    
    print("\n" + "=" * 60)
    print(f"DATASET I-7 SUMMARY:")
    print(f"  Total items:      {len(dataset)}")
    print(f"  Train split:      {len(train)} (60%)")
    print(f"  Held-out split:   {len(held_out)} (40%)")
    print(f"  Guilds covered:   {len(set(d['target_guild'] for d in dataset))} / {len(GUILD_CATALOG)}")
    print(f"  Ambiguity types:  {dict(Counter(d['ambiguity_type'] for d in dataset))}")
    print("=" * 60)
    
    # Save JSON
    OUTPUT_JSON.parent.mkdir(parents=True, exist_ok=True)
    with open(OUTPUT_JSON, "w", encoding="utf-8") as f:
        json.dump({
            "version": "1.0.0",
            "task": "I-7",
            "description": "Curated routing intent dataset with train/held-out split for J-13 tiebreaker validation",
            "total_count": len(dataset),
            "train_count": len(train),
            "held_out_count": len(held_out),
            "guild_count": len(set(d['target_guild'] for d in dataset)),
            "items": dataset
        }, f, indent=2, ensure_ascii=False)
        
    print(f"Successfully generated dataset at: {OUTPUT_JSON}")
    return dataset

if __name__ == "__main__":
    build_curated_dataset()
