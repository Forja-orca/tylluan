#!/usr/bin/env bash
# scripts/check_no_predation.sh -- 4th mechanical gate (ADR-013, Non-Predation
# Contract): verifies that the change under evaluation never preys on the work
# of another rung (otro agente, otro guild, otro servicio, otro contrato).
#
# Why this exists: ADR-013 (docs/reference/adr/ADR013_non_predation_contract.md)
# turns the fleet's operating discipline -- "la mejora debe ser para todos, sin
# que un solo escalón sufra" -- from prose in AGENTS.md into an executable
# contract, the same way check_head_sync.sh (HEAD), check_test_count.sh (tests)
# and check_docs_reality.sh (docs) made their three dimensions mechanical.
# Without this gate, an agent could ship a change that breaks another agent's
# WIP, migrates a port without its config, or touches a critical route without
# declaring impact -- and nobody would catch it until the damage surfaced.
#
# What it evaluates: the HEAD commit (run it right after `git commit`, before
# pushing or reporting as verified) plus the working tree left behind.
#
# Four checks (ADR-013 §2):
#   1. Foreign WIP in the diff: tracked files modified/staged beyond the commit
#      that are NOT part of it; plus low-confidence warning for untracked files
#      whose basename matches a path the commit touches.
#   2. Port contracts: a commit that swaps port-like values (`:NNNN`,
#      `port = NNNNN`) in non-.toml files without any .toml in the same commit.
#   3. Impact declaration: commits touching critical routes
#      (crates/tylluan-kernel/src/transport/, guilds/, integrations/,
#      tylluan*.toml) without a `## Impact` section in the message.
#   4. Out-of-scope workload (heuristic, low-confidence, never fails alone):
#      modified files neither mentioned in the commit subject nor part of it.
#
# Regla de oro (ADR-013): the gate REPORTS, it never edits, reverts or blocks
# mechanically. Exit 1 means "findings exist -- resolve them or add `## Impact`
# via `git commit --amend` before reporting this change as verified".
#
# Performance note (ADR-013 targets <1s): on Windows/MSYS every process spawn
# AND every bash subshell fork costs ~0.1-0.4s, so this script makes exactly
# 4 git calls and parses everything -- port values out of the diff, file
# lists out of the patch headers, critical-route matching, basename/dirname,
# subject mentions -- in PURE BASH with ZERO forks inside loops (the port
# extractor communicates through a global instead of $(), which cost ~60ms
# per diff line and made two earlier implementations measure 5.6s and >15s
# on the same commit). Measured on the 2026-09-13 machine: ~1s wall on
# Windows/MSYS (4 git spawns dominate), well under 1s on Linux/CI.
#
# Usage: scripts/check_no_predation.sh
# Exit 0 if no hard findings (low-confidence findings are listed, count shown).
# Exit 1 if any hard finding (checks 1a/2/3) -- read the output, it names them.
#
# Self-tested 2026-09-13 against known-truth cases from ADR-013 Verification §2
# (critical route without `## Impact` -> exit 1; with `## Impact` -> exit 0;
# port swap without .toml -> exit 1; port swap with .toml -> exit 0), each in
# a disposable worktree whose checked-out tree matches the tested commit.

set -uo pipefail
cd "$(dirname "$0")/.."

problems=0
low_conf=0

# git call 1 of 4: subject + body in one shot (doubles as the empty-repo guard).
combined=$(git log -1 --format=%s%n%b HEAD 2>/dev/null) || {
    echo "✅ No HEAD commit to evaluate (empty repo)."
    exit 0
}
head_subject="${combined%%$'\n'*}"
head_body="${combined#*$'\n'}"

# git call 2 of 4: the whole-commit -U0 patch. File list AND per-line port
# values are parsed from this single output -- no separate --name-only call.
head_files=()
declare -A seen_files=() port_add=() port_rem=()
cur_file=""
PORTS_OUT=""
extract_ports() {
    local line="$1" re='(:|port[[:space:]]*=|PORT[[:space:]]*=)[[:space:]]*([0-9]{4,5})'
    PORTS_OUT=""
    while [[ $line =~ $re ]]; do
        PORTS_OUT+="${BASH_REMATCH[2]} "
        line="${line#*"${BASH_REMATCH[2]}"}"
    done
}
while IFS= read -r line; do
    case "$line" in
        'diff --git '*)
            cur_file="${line##* b/}"
            if [ -n "$cur_file" ] && [ -z "${seen_files[$cur_file]:-}" ]; then
                seen_files[$cur_file]=1
                head_files+=("$cur_file")
            fi
            ;;
        +++*|---*|\\*) continue ;;
        +*) extract_ports "${line#+}"; [ -n "$PORTS_OUT" ] && port_add["$cur_file"]+="$PORTS_OUT " ;;
        -*) extract_ports "${line#-}"; [ -n "$PORTS_OUT" ] && port_rem["$cur_file"]+="$PORTS_OUT " ;;
    esac
done < <(git show --format= -U0 HEAD 2>/dev/null || true)

# git call 3 of 4: working-tree changes beyond the commit.
mapfile -t wt_files < <(git diff --name-only HEAD 2>/dev/null || true)
# git call 4 of 4: untracked files.
mapfile -t untracked < <(git ls-files --others --exclude-standard 2>/dev/null || true)

declare -A in_head=()
for f in "${head_files[@]}"; do in_head["$f"]=1; done

echo "Evaluating: $head_subject"
echo "Files in commit: ${#head_files[@]} | tracked files modified beyond it: ${#wt_files[@]}"
echo ""

# ── Check 1a: foreign WIP -- tracked files modified/staged beyond the commit
#    that the commit itself does not contain (someone else's uncommitted work
#    sitting in this checkout while I commit mine). ──
foreign_wip=()
for f in "${wt_files[@]}"; do
    [ -n "${in_head[$f]:-}" ] && continue
    foreign_wip+=("$f")
done
if [ "${#foreign_wip[@]}" -gt 0 ]; then
    for f in "${foreign_wip[@]}"; do
        echo "❌ WIP ajeno: '$f' está modificado en el working tree pero NO forma parte del commit evaluado -- posible trabajo de otro agente en este checkout (ADR-013 §2)."
    done
    problems=1
fi

# ── Check 1b (low-confidence): untracked files whose basename matches a path
#    the commit touches -- possible foreign/ignored file silently excluded. ──
declare -A head_bases=()
for f in "${head_files[@]}"; do head_bases["${f##*/}"]="$f"; done
for u in "${untracked[@]}"; do
    b="${u##*/}"
    [ -n "${head_bases[$b]:-}" ] || continue
    [ "${head_bases[$b]}" = "$u" ] && continue
    echo "⚠️  low-confidence: el archivo sin trackear '$u' coincide en nombre con '${head_bases[$b]}' que SÍ toca el commit, pero no forma parte de él -- posible WIP ajeno o ignorado."
    low_conf=$((low_conf+1))
done

# ── Check 2: port contracts -- the commit swaps port-like values in non-.toml
#    files without any .toml in the same commit. A pure addition of a port
#    reference is NOT a migration and is ignored. ──
toml_in_commit=0
for f in "${head_files[@]}"; do
    case "$f" in *.toml) toml_in_commit=1 ;; esac
done
port_changed=()
for f in "${!port_add[@]}"; do
    case "$f" in *.toml|"") continue ;; esac
    r="${port_rem[$f]:-}"
    [ -z "$r" ] && continue
    a_sorted=$(printf '%s' "${port_add[$f]}" | tr ' ' '\n' | sort -u | tr '\n' ' ')
    r_sorted=$(printf '%s' "$r" | tr ' ' '\n' | sort -u | tr '\n' ' ')
    if [ "$a_sorted" != "$r_sorted" ]; then
        port_changed+=("$f")
    fi
done
if [ "${#port_changed[@]}" -gt 0 ] && [ "$toml_in_commit" -eq 0 ]; then
    for f in "${port_changed[@]}"; do
        echo "❌ Contrato de puerto: '$f' cambia valores con forma de puerto sin que ningún .toml (tylluan.toml / tylluan.example.toml) forme parte del commit -- si es una migración de puerto real, la config va en el MISMO commit (ADR-013 §2)."
    done
    problems=1
fi

# ── Check 3: impact declaration -- commits touching critical routes must
#    carry a `## Impact` section in the message (ADR-013 §1). Pure-bash
#    glob matching against the four route patterns. ──
critical_hits=()
for f in "${head_files[@]}"; do
    case "$f" in
        crates/tylluan-kernel/src/transport/*|guilds/*|integrations/*|tylluan*.toml)
            critical_hits+=("$f") ;;
    esac
done
if [ "${#critical_hits[@]}" -gt 0 ]; then
    body_lcase="${head_body,,}"
    if [[ $body_lcase != *'## impact'* ]]; then
        for f in "${critical_hits[@]}"; do
            echo "❌ Declaración de impacto: el commit toca la ruta crítica '$f' sin sección '## Impact' en el mensaje (ADR-013 §1). Corrige con: git commit --amend"
        done
        problems=1
    fi
fi

# ── Check 4 (heuristic, low-confidence, never fails alone): out-of-scope
#    workload -- modified files neither mentioned in the commit subject nor
#    part of the commit itself. ──
subject_lcase="${head_subject,,}"
scope_hits=()
for f in "${wt_files[@]}"; do
    [ -n "${in_head[$f]:-}" ] && continue
    base="${f##*/}"
    dir="${f%/*}"
    if [[ $subject_lcase == *"${base,,}"* || $subject_lcase == *"${dir,,}"* ]]; then
        continue
    fi
    scope_hits+=("$f")
done
if [ "${#scope_hits[@]}" -gt 0 ]; then
    echo "⚠️  low-confidence: ${#scope_hits[@]} archivo(s) modificado(s) fuera del alcance declarado del commit (no mencionados en el mensaje y no parte de él):"
    n=0
    for f in "${scope_hits[@]}"; do
        n=$((n+1))
        [ "$n" -le 8 ] && echo "    - $f"
    done
    if [ "${#scope_hits[@]}" -gt 8 ]; then
        echo "    ... y $(( ${#scope_hits[@]} - 8 )) más"
    fi
    low_conf=$((low_conf+1))
fi

echo ""
if [ "$problems" -eq 0 ]; then
    if [ "$low_conf" -gt 0 ]; then
        echo "✅ No predation: sin hallazgos duros ($low_conf hallazgo(s) low-confidence listados arriba -- triage humano, nunca bloqueo)."
    else
        echo "✅ No predation: sin hallazgos (WIP ajeno, contratos de puerto, rutas críticas con Impact y alcance declarado -- todo limpio)."
    fi
    exit 0
fi

echo ""
echo "❌ Hallazgos de no-depredación arriba. El gate es REPORT-ONLY: nunca edita ni revierte."
echo "   Resuélvelos (o declara '## Impact' vía git commit --amend) antes de reportar el cambio como verificado (ADR-013)."
exit 1
