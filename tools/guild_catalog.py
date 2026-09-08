#!/usr/bin/env python3
"""
G6 — Canonical guild identity catalog.

This is the SINGLE SOURCE OF TRUTH for all guild identity in Tylluan.
Every consumer (I-7 dataset generator, benchmark evaluator, CI gate)
imports from here instead of maintaining its own copy.

Design: DESIGN_guild_identity_gate.md (2026-09-07, approved 2026-09-08)
Contract: each GuildId appears exactly once, with module path, aliases,
          status (routable|experimental|excluded), and description.

Usage:
    from tools.guild_catalog import GUILDS, GuildEntry, ROUTABLE_IDS

    # Get all routable guild IDs
    routable = ROUTABLE_IDS

    # Get a guild's module path
    mod = GUILDS["scrapling"].module  # "guilds/core/scrapling_web.py"

    # Get description for benchmark embedding
    desc = GUILDS["bash"].description

    # Generate snapshot JSON
    import json
    snapshot = generate_snapshot()
    print(json.dumps(snapshot, indent=2))
"""

from __future__ import annotations

import json
import sys

if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, List, Optional


@dataclass(frozen=True)
class GuildEntry:
    """Single canonical record for a guild."""

    id: str  # GuildId: snake_case, stable, canonical
    module: str  # Python import path (filesystem discovery)
    description: str  # One-line description for benchmarks
    status: str = "routable"  # routable | experimental | excluded
    registration: str = "lazy"  # lazy | always_on | v2
    aliases: tuple[str, ...] = ()  # Legacy input names (migration only)
    category: str = "core"  # builder | scholar | watcher | core
    keyword_rules: tuple[str, ...] = ()  # For keyword-based routing hints

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "module": self.module,
            "description": self.description,
            "status": self.status,
            "registration": self.registration,
            "aliases": list(self.aliases),
            "category": self.category,
        }


# ──────────────────────────────────────────────────────────────────────
# CANONICAL GUILD REGISTRY
#
# This dict is the single source of truth. When you add a guild:
#   1. Add its .py file under guilds/
#   2. Add a GuildEntry here
#   3. Run: python tools/check_guild_consistency.py
#
# The CI gate will tell you exactly which other files need updating.
# ──────────────────────────────────────────────────────────────────────

GUILDS: Dict[str, GuildEntry] = {
    # ── Builders ──────────────────────────────────────────────────────
    "audio_tools": GuildEntry(
        id="audio_tools",
        module="guilds.builders.plugins.audio_tools",
        description="Process, convert, transcribe audio files and spectrograms",
        category="builder",
        keyword_rules=("audio", "transcribe", "wav", "mp3", "spectrogram"),
    ),
    "bash": GuildEntry(
        id="bash",
        module="guilds.builders.plugins.bash",
        description="Execute shell commands, scripts, and system binaries",
        category="builder",
        registration="always_on",
        keyword_rules=("run", "bash", "shell", "command", "execute", "exec", "./", "chmod", "apt", "npm", "pip", "cargo", "kill", "process"),
    ),
    "clipboard_tools": GuildEntry(
        id="clipboard_tools",
        module="guilds.builders.plugins.clipboard_tools",
        description="Read from or write text into the system clipboard",
        category="builder",
        keyword_rules=("clipboard", "copy", "paste", "portapapeles"),
    ),
    "code": GuildEntry(
        id="code",
        module="guilds.builders.plugins.code",
        description="Modify, generate, and edit source code files",
        category="builder",
        keyword_rules=("code", "function", "implement", "refactor", "unit test", "fix bug", "código", "función"),
    ),
    "code_analysis": GuildEntry(
        id="code_analysis",
        module="guilds.builders.plugins.code_analysis",
        description="Static code analysis, complexity metrics, dead code detection",
        category="builder",
        keyword_rules=("complexity", "dead code", "cyclomatic", "analisis de codigo", "metricas"),
    ),
    "database": GuildEntry(
        id="database",
        module="guilds.builders.plugins.database",
        description="SQL database queries, SQLite, Postgres, schema inspections",
        category="builder",
        keyword_rules=("database", "sql", "sqlite", "postgres", "query", "schema", "table", "tabla", "base de datos"),
    ),
    "docker": GuildEntry(
        id="docker",
        module="guilds.builders.plugins.docker",
        description="Docker container lifecycle, images, logs, compose",
        category="builder",
        keyword_rules=("docker", "container", "image", "dockerfile", "compose", "contenedor"),
    ),
    "ffmpeg_tools": GuildEntry(
        id="ffmpeg_tools",
        module="guilds.builders.plugins.ffmpeg_tools",
        description="Video and audio slicing, encoding, transcode via ffmpeg",
        category="builder",
        keyword_rules=("ffmpeg", "video", "audio slice", "encode", "transcode"),
    ),
    "filesystem": GuildEntry(
        id="filesystem",
        module="guilds.builders.plugins.filesystem",
        description="List files, find files, show directory contents, file operations",
        category="builder",
        registration="always_on",
        keyword_rules=("file", "dir", "directory", "folder", "list file", "list dir", "find file", "archivo", "directorio", "carpeta"),
    ),
    "formatter": GuildEntry(
        id="formatter",
        module="guilds.builders.plugins.formatter",
        description="Format code files with Ruff, Prettier, Rustfmt",
        category="builder",
        keyword_rules=("format", "formatter", "prettier", "rustfmt", "ruff", "formatear", "estilo"),
    ),
    "git": GuildEntry(
        id="git",
        module="guilds.builders.plugins.git",
        description="Git version control, commits, branches, diffs, log",
        category="builder",
        registration="always_on",
        keyword_rules=("git", "commit", "branch", "diff", "checkout", "push", "pull", "merge", "repo", "repository", "stash", "log"),
    ),
    "local_llm_proxy": GuildEntry(
        id="local_llm_proxy",
        module="guilds.builders.plugins.local_llm_proxy",
        description="Proxy requests to external Ollama, LM Studio, or vLLM",
        category="builder",
        keyword_rules=("ollama", "lm studio", "vllm", "external llm"),
    ),
    "sandbox": GuildEntry(
        id="sandbox",
        module="guilds.builders.plugins.sandbox",
        description="Sandboxed code execution in isolated containers",
        category="builder",
        status="experimental",
    ),
    "screenshot_tools": GuildEntry(
        id="screenshot_tools",
        module="guilds.builders.plugins.screenshot_tools",
        description="Capture screenshot of the screen or active window",
        category="builder",
        keyword_rules=("screenshot", "captura", "screen capture"),
    ),

    # ── Core ──────────────────────────────────────────────────────────
    "browser": GuildEntry(
        id="browser",
        module="guilds.core.browser",
        description="Headless browser automation, click, type, scrape with CDP",
        category="core",
        keyword_rules=("browser", "chrome", "navigate", "click", "cdp", "puppeteer", "abrir pagina", "url"),
    ),
    "code_graph": GuildEntry(
        id="code_graph",
        module="guilds.core.code_graph",
        description="Dependency graphs, symbol call trees, module hierarchy",
        category="core",
        registration="always_on",
        keyword_rules=("dependency", "call tree", "hierarchy", "graph", "dependencias", "arbol de llamadas"),
    ),
    "code_reviewer": GuildEntry(
        id="code_reviewer",
        module="guilds.core.code_reviewer",
        description="Automated code review, security smell and bug detection",
        category="core",
        keyword_rules=("review", "pr", "pull request", "smell", "race condition", "revisar codigo", "deadlock"),
    ),
    "coloquio": GuildEntry(
        id="coloquio",
        module="guilds.core.coloquio",
        description="Send and read messages in multi-agent Coloquio channels",
        category="core",
        registration="always_on",
    ),
    "coloquio_digest": GuildEntry(
        id="coloquio_digest",
        module="guilds.core.coloquio_digest",
        description="Generate executive digests of Coloquio conversations",
        category="core",
    ),
    "comfy_ui": GuildEntry(
        id="comfy_ui",
        module="guilds.core.comfy_ui",
        description="Generate images via local ComfyUI Stable Diffusion workflow",
        category="core",
        registration="always_on",
    ),
    "coordinator": GuildEntry(
        id="coordinator",
        module="guilds.core.coordinator",
        description="Decompose complex multi-step tasks and orchestrate sub-agents",
        category="core",
    ),
    "deep_web_research": GuildEntry(
        id="deep_web_research",
        module="guilds.core.deep_web_research",
        description="Multi-hop web search synthesis and paper extraction",
        category="core",
        registration="always_on",
        keyword_rules=("deep research", "paper", "arxiv", "investiga a fondo", "scientific literature"),
    ),
    "llama_backend": GuildEntry(
        id="llama_backend",
        module="guilds.core.llama_backend",
        description="Direct GGUF inference via local llama-server",
        category="core",
    ),
    "mcp_bridge": GuildEntry(
        id="mcp_bridge",
        module="guilds.core.mcp_bridge",
        description="Bridge to external Model Context Protocol tool servers",
        category="core",
        keyword_rules=("mcp", "mcp tool", "mcp bridge", "protocol server"),
    ),
    "n8n_bridge": GuildEntry(
        id="n8n_bridge",
        module="guilds.core.n8n_bridge",
        description="Trigger and manage n8n automation workflows and webhooks",
        category="core",
        keyword_rules=("n8n", "webhook", "automation workflow", "flujo n8n"),
    ),
    "night_reasoner": GuildEntry(
        id="night_reasoner",
        module="guilds.core.night_reasoner",
        description="Nightly memory consolidation, pattern abstraction, dream cycle",
        category="core",
    ),
    "scheduler": GuildEntry(
        id="scheduler",
        module="guilds.core.scheduler",
        description="Create, list, and cancel scheduled tasks and reminders",
        category="core",
        registration="always_on",
    ),
    "scrapling": GuildEntry(
        id="scrapling",
        module="guilds.core.scrapling_web",
        description="Undetected web scraping and HTML content extraction",
        category="core",
        registration="always_on",
        aliases=("scrapling_web",),
        keyword_rules=("scrape", "scraping", "html extract", "crawler", "extraer web"),
    ),
    "seed_tools": GuildEntry(
        id="seed_tools",
        module="guilds.core.seed_tools",
        description="Seed and bootstrap initial tool configurations",
        category="core",
    ),
    "vision": GuildEntry(
        id="vision",
        module="guilds.core.vision",
        description="General image analysis, OCR, visual question answering",
        category="core",
        registration="always_on",
    ),
    "vision_moondream": GuildEntry(
        id="vision_moondream",
        module="guilds.core.vision_moondream",
        description="Legacy Moondream vision (superseded by vision.py SmolVLM2)",
        category="core",
        status="excluded",
    ),
    "websearch": GuildEntry(
        id="websearch",
        module="guilds.core.websearch",
        description="Web search engine queries and internet search",
        category="core",
        registration="always_on",
        keyword_rules=("web search", "search web", "internet", "google", "noticias", "what is", "who is", "latest"),
    ),

    # ── Scholars ──────────────────────────────────────────────────────
    "ast_surgeon": GuildEntry(
        id="ast_surgeon",
        module="guilds.scholars.plugins.ast_surgeon",
        description="AST parsing, node transformations, syntax tree refactoring",
        category="scholar",
        keyword_rules=("ast", "syntax tree", "node", "parse ast", "transform node", "arbol sintactico"),
    ),
    "data_tools": GuildEntry(
        id="data_tools",
        module="guilds.scholars.plugins.data_tools",
        description="Parse, transform, query JSON, YAML, CSV, Parquet data",
        category="scholar",
        keyword_rules=("json", "csv", "yaml", "parquet", "parse json", "convert csv"),
    ),
    "deep_analysis": GuildEntry(
        id="deep_analysis",
        module="guilds.scholars.plugins.deep_analysis",
        description="In-depth document analysis, summarization and theme extraction",
        category="scholar",
        keyword_rules=("thematic", "clustering", "deep analysis", "transcript", "analisis profundo", "resumen extenso"),
    ),
    "ingest": GuildEntry(
        id="ingest",
        module="guilds.scholars.plugins.ingest",
        description="Ingest and chunk raw documents into SilvaDB",
        category="scholar",
        keyword_rules=("ingest", "chunk docs", "index files", "ingestar"),
    ),
    "knowledge": GuildEntry(
        id="knowledge",
        module="guilds.scholars.plugins.knowledge",
        description="Query and traverse SilvaDB knowledge graph and triples",
        category="scholar",
        registration="always_on",
    ),
    "memory": GuildEntry(
        id="memory",
        module="guilds.scholars.plugins.memory",
        description="Store, recall, and manage sovereign long-term memory",
        category="scholar",
    ),
    "pdf": GuildEntry(
        id="pdf",
        module="guilds.scholars.plugins.pdf",
        description="Extract text, tables, and metadata from PDF files",
        category="scholar",
        keyword_rules=("pdf", "paper.pdf", "documento pdf", "extract text pdf"),
    ),
    "search": GuildEntry(
        id="search",
        module="guilds.scholars.plugins.search",
        description="Hybrid search across indexed codebase and documents",
        category="scholar",
        keyword_rules=("search memory", "find in notes", "hybrid search", "buscar en memoria"),
    ),
    "sequential_thinking": GuildEntry(
        id="sequential_thinking",
        module="guilds.scholars.plugins.sequential_thinking",
        description="Structured chain-of-thought and step-by-step reasoning",
        category="scholar",
    ),

    # ── Wardens ───────────────────────────────────────────────────────
    "audit": GuildEntry(
        id="audit",
        module="guilds.wardens.plugins.audit",
        description="Security audit, token leak detection, permission checks",
        category="watcher",
        keyword_rules=("audit", "token leak", "credentials", "security check", "auditoria de seguridad", "secretos"),
    ),
    "biome_warden": GuildEntry(
        id="biome_warden",
        module="guilds.wardens.plugins.biome_warden",
        description="Biome linter, fast JS/TS code checking and formatting",
        category="watcher",
        keyword_rules=("biome", "biome check", "lint ts", "lint react"),
    ),

    # ── Watchers ──────────────────────────────────────────────────────
    "cron_scheduler": GuildEntry(
        id="cron_scheduler",
        module="guilds.watchers.plugins.cron_scheduler",
        description="Schedule, list, and cancel recurring cron jobs",
        category="watcher",
        keyword_rules=("cron", "schedule", "recurring", "cron job", "programar tarea"),
    ),
    "monitor": GuildEntry(
        id="monitor",
        module="guilds.watchers.plugins.monitor",
        description="Observe running processes, system health, and alerts",
        category="watcher",
        registration="always_on",
        keyword_rules=("monitor", "observe", "watch task", "daemon alert", "vigilar"),
    ),
    "system_metrics": GuildEntry(
        id="system_metrics",
        module="guilds.watchers.plugins.system_metrics",
        description="Inspect CPU, RAM, disk, network utilization",
        category="watcher",
        registration="always_on",
        keyword_rules=("cpu", "ram", "memory usage", "disk space", "temperature", "metricas", "memoria ram"),
    ),
}


# ──────────────────────────────────────────────────────────────────────
# DERIVED SETS (consumers import these, not the raw dict)
# ──────────────────────────────────────────────────────────────────────

ROUTABLE_IDS: tuple[str, ...] = tuple(
    sorted(gid for gid, g in GUILDS.items() if g.status == "routable")
)

ROUTABLE_DESCRIPTIONS: Dict[str, str] = {
    gid: g.description for gid, g in GUILDS.items() if g.status == "routable"
}

ROUTABLE_KEYWORDS: Dict[str, tuple[str, ...]] = {
    gid: g.keyword_rules for gid, g in GUILDS.items()
    if g.status == "routable" and g.keyword_rules
}

ALL_IDS: tuple[str, ...] = tuple(sorted(GUILDS.keys()))

# Aliases map: alias -> canonical GuildId
ALIASES: Dict[str, str] = {}
for _gid, _g in GUILDS.items():
    for _alias in _g.aliases:
        ALIASES[_alias] = _gid


# ──────────────────────────────────────────────────────────────────────
# SNAPSHOT GENERATOR
# ──────────────────────────────────────────────────────────────────────

def generate_snapshot() -> dict:
    """Generate a deterministic JSON snapshot of the canonical catalog."""
    return {
        "guilds": {
            gid: g.to_dict()
            for gid, g in sorted(GUILDS.items())
        },
        "metadata": {
            "generated_at": datetime.now(timezone.utc).isoformat(),
            "source": "tools/guild_catalog.py",
            "total": len(GUILDS),
            "total_routable": sum(1 for g in GUILDS.values() if g.status == "routable"),
            "total_experimental": sum(1 for g in GUILDS.values() if g.status == "experimental"),
            "total_excluded": sum(1 for g in GUILDS.values() if g.status == "excluded"),
        },
    }


if __name__ == "__main__":
    snapshot = generate_snapshot()
    if "--json" in sys.argv:
        print(json.dumps(snapshot, indent=2))
    else:
        m = snapshot["metadata"]
        print(f"Guild catalog: {m['total']} guilds "
              f"({m['total_routable']} routable, "
              f"{m['total_experimental']} experimental, "
              f"{m['total_excluded']} excluded)")
        for gid, g in sorted(GUILDS.items()):
            status_mark = {"routable": "[ok]", "experimental": "[~]", "excluded": "[!!]"}.get(g.status, "??")
            print(f"  {status_mark} {gid:25s} {g.module}")
