#!/usr/bin/env python3
"""Where did the last N days of commits go? Counts commits per project block.

Why it exists: the TL (and any agent) can fix its attention on one block for weeks and lose
the global picture. This prints the objective split from git so the imbalance is visible.
A commit that touches several blocks counts once in each, so percentages add up to more
than 100. Process documents (IMPL reports, COLA files, STATUS, ROADMAP) ride along with
almost every integration commit, so that row overstates real documentation effort.

Usage: python scripts/project_balance.py [days]   (default 30)
"""
import collections
import re
import subprocess
import sys

BLOCKS = [
    ("Seguridad (auth, auditoria, permisos, guard)", r"(security/|transport/http/auth|oauth|handler_do/audit|federation/permissions|api_audit|SECURITY|ADR02[24]|ADR019)"),
    ("Memoria y recuperacion (silva, recall, grafo)", r"(memory/|silva|recall|graph|embed)"),
    ("Herramientas / guilds", r"(^guilds/|registry/|handler_do/)"),
    ("Federacion y malla (link, p2p)", r"(crates/tylluan-link|federation/|mesh|gossip|nat)"),
    ("Dashboard / interfaz", r"(^dashboard/|^packages/)"),
    ("Cara publica (README, guias, diagramas)", r"(^README|^docs/(guides|getting-started|concepts|assets)|CHANGELOG)"),
    ("Instalacion y release", r"(install\.|release|Dockerfile)"),
    ("Tests y CI", r"(tests/|\.github/workflows/ci|scripts/check|verify\.sh)"),
    ("Proceso de flota (IMPL, COLA, STATUS, ROADMAP)", r"(docs/internal/|^STATUS|^ROADMAP|^AGENTS)"),
]


def main() -> None:
    days = int(sys.argv[1]) if len(sys.argv) > 1 else 30
    out = subprocess.run(
        ["git", "log", f"--since={days}.days", "--name-only", "--pretty=format:@@%h"],
        capture_output=True, text=True, encoding="utf-8",
    ).stdout
    commits = out.split("@@")[1:]
    counts = collections.Counter()
    for commit in commits:
        files = commit.strip().split("\n")[1:]
        hit = {name for f in files for name, rx in BLOCKS if re.search(rx, f)}
        counts.update(hit)
    total = len(commits)
    print(f"commits in the last {days} days: {total}")
    for name, _ in BLOCKS:
        print(f"{counts[name]:5d}  {100 * counts[name] / total:5.1f}%  {name}")


if __name__ == "__main__":
    main()
