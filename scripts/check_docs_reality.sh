#!/usr/bin/env bash
# Verifies that every file path and port number cited in the docs actually
# matches reality, and that AGENTS.md/CLAUDE.md don't contradict each other
# on the current version.
#
# Why this exists: real doc-drift incidents in the 2026-09-11 cycle -- the
# README's architecture Mermaid diagram broke on GitHub, README/STATUS
# claimed "49 guilds" when the real catalog had 46, the OpenClaw/Hermes
# template cited port 3030 while the real kernel port is 4000 (fixed in
# d72f93d), and AGENTS.md:62 cited "v0.17.0 (tagged 2026-08-23)" while
# CLAUDE.md:90 cited "v0.16.0+ (unreleased)" -- the universal agent file and
# the Claude Code file contradicting each other on the current milestone.
# Nothing forced docs to stay honest about the paths, ports and versions they
# cite -- the same gap check_head_sync.sh (HEAD) and check_test_count.sh
# (tests) already close for their two dimensions. This is that same pattern.
#
# Three checks:
#   1. Every backtick-cited path in STATUS.md, README.md, AGENTS.md,
#      CLAUDE.md and docs/**/*.md that looks like a repo path (contains '/'
#      or a known source extension) must exist on disk.
#   2. Every port cited as :NNNN in those docs must equal the real kernel
#      port from tylluan.toml [nexus] (line ~201: port = 4000). EXCEPTION:
#      lines that are clearly historical context ("antes", "histórico",
#      "migró", "previously", "commit <hash> migro") are not errors -- the
#      port was different at an earlier point. If we can't tell confidently,
#      we skip it rather than risk a hard false positive.
#   3. The "## Estado actual" version in AGENTS.md must match the one in
#      CLAUDE.md (different audiences, same current milestone).
#
# Usage: scripts/check_docs_reality.sh
# Exit 0 if no findings, exit 1 if any. This script only REPORTS -- it never
# edits docs. A human or agent reviews the output and fixes the docs.
#
# NOT wired into scripts/verify.sh yet: it must first run clean against the
# real repo without false positives (historical port refs are the known trap).

set -euo pipefail
cd "$(dirname "$0")/.."

DOCS=(STATUS.md README.md AGENTS.md CLAUDE.md)
while IFS= read -r d; do
    DOCS+=("$d")
done < <(find docs -name '*.md' -type f 2>/dev/null | sort || true)

# The kernel's real port lives in the [nexus] section (line ~201: port = 4000);
# tylluan.toml also has OTHER services' ports (llama 9000, etc.).
real_port=$(awk '/^\[nexus\]/{f=1} f&&/^port[[:space:]]*=/{gsub(/[^0-9]/,"",$0); print; exit}' tylluan.toml | head -1)
echo "Real port (tylluan.toml [nexus]): ${real_port:-<none>}"
echo "Docs scanned: ${#DOCS[@]} (STATUS.md, README.md, AGENTS.md, CLAUDE.md, docs/**/*.md)"
echo ""

problems=0
low_conf=0

# Lines that mark a port mention as historical context, not current truth.
hist_regex='antes|historico|hist[oó]rico|migro|migr[oó]|previously|was port|old port|former port|commit [0-9a-f]{7,}'

# ── Check 1: backtick-cited paths must exist ──────────────────────────────
# One grep pass per doc: GNU grep -n -o prints "lineno:match" per match.
for doc in "${DOCS[@]}"; do
    [ -f "$doc" ] || continue
    while IFS= read -r m; do
        lineno="${m%%:*}"
        tok="${m#*:}"
        # Skip obviously non-path tokens.
        case "$tok" in
            *://*|git@*|www.*|:*|http*) continue ;;
            */*|*.rs|*.py|*.toml|*.json|*.sh|*.md|*.yml|*.yaml|*.ts|*.tsx|*.html|*.css|*.svg|*.lock|*.db)
                ;;
            *) continue ;;
        esac
        # Strip ./ prefix, trailing line/col refs and punctuation.
        p="${tok#./}"
        p="${p%%:[0-9]*}"
        p="${p%%#L*}"
        p="${p%%,}"
        p="${p%%)}"
        p="${p%,}"
        p="${p%.}"
        [ -n "$p" ] || continue
        if [ ! -e "$p" ]; then
            echo "❌ $doc:$lineno: cita \`$tok\` (path \`$p\`) que no existe en el repo"
            problems=1
        fi
    done < <(grep -nEo '\`[^\`]+\`' "$doc" || true)
done

# ── Check 2: cited ports must match the real port ─────────────────────────
for doc in "${DOCS[@]}"; do
    [ -f "$doc" ] || continue
    while IFS= read -r m; do
        lineno="${m%%:*}"
        tok="${m#*:}"
        case "$tok" in
            arXiv:*) continue ;; # arXiv IDs (:2602.01848) are NOT ports
        esac
        if echo "$tok" | grep -qE "$hist_regex"; then
            continue
        fi
        p="${tok#:}"
        if [ "$p" != "$real_port" ]; then
            # ADRs record decisions at a point in time -- old ports there are
            # inherently historical context, report as low-confidence, never
            # a hard error.
            case "$doc" in
                docs/reference/adr/*)
                    echo "⚠️  low-confidence $doc:$lineno: cita puerto :$p, el real es :$real_port (ADRs son registros historicos por naturaleza)"
                    low_conf=$((low_conf+1))
                    ;;
                *)
                    echo "❌ $doc:$lineno: cita puerto :$p, el real es :$real_port"
                    problems=1
                    ;;
            esac
        fi
    done < <(grep -nEo 'arXiv:[0-9]{4,5}|:[0-9]{4,5}\b' "$doc" || true)
done

# ── Check 3: AGENTS.md vs CLAUDE.md version/milestone consistency ────────
# AGENTS.md is the UNIVERSAL file (read by Deep/Mimo/Codex on connect);
# CLAUDE.md is Claude Code-specific. Different audiences, but they must not
# contradict each other on the current version/milestone.
agents_ver=$(grep -E '^## Estado actual' AGENTS.md | head -1 | grep -oE 'v[0-9]+\.[0-9]+(\.[0-9]+)?' | head -1 || true)
claude_ver=$(grep -E '^## Estado actual' CLAUDE.md | head -1 | grep -oE 'v[0-9]+\.[0-9]+(\.[0-9]+)?' | head -1 || true)
if [ -n "$agents_ver" ] && [ -n "$claude_ver" ] && [ "$agents_ver" != "$claude_ver" ]; then
    echo "❌ AGENTS.md cita $agents_ver pero CLAUDE.md cita $claude_ver en '## Estado actual' — los dos archivos se contradicen en la version/milestone actual"
    problems=1
elif [ -z "$agents_ver" ] || [ -z "$claude_ver" ]; then
    echo "⚠️  low-confidence: no se pudo extraer '## Estado actual' de AGENTS.md (${agents_ver:-vacio}) o CLAUDE.md (${claude_ver:-vacio})"
    low_conf=$((low_conf+1))
fi

echo ""
if [ "$problems" -eq 0 ]; then
    echo "✅ No doc-path, doc-port or version drift found ($low_conf low-confidence findings suppressed)."
    exit 0
fi

echo "⚠️  Findings above: each needs human/agent triage (this script only reports, never edits)."
exit 1