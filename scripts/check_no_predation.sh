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
# What it evaluates (T1275, 2026-10-18): EVERY commit of the range being
# pushed, not just HEAD. Real incident: a 5-commit push slipped through
# because HEAD was a docs-only commit while an EARLIER commit in the same
# push (abc8534) touched crates/tylluan-kernel/src/transport/ without a
# `## Impact` section -- the pre-push hook ran this gate against HEAD and
# saw only STATUS.md. Per-commit checks (ports, Impact) are therefore
# evaluated per commit; working-tree checks (WIP ajeno, alcance) compare
# against the UNION of all commits in the range, and against the range tip
# (the commit the working tree actually sits on).
#
# Range resolution:
#   - No argument: `origin/main..HEAD` (what a push to main would carry),
#     falling back to `HEAD` alone when the range is empty (everything
#     already pushed) or origin/main does not exist (fresh clone without
#     remote, detached worktree, empty repo guard). This keeps the classic
#     "run it right after `git commit`" usage identical: right after a
#     commit, origin/main..HEAD IS that commit.
#   - Optional argument: an explicit rev-range or single commit
#     (`scripts/check_no_predation.sh abc8534~1..abc8534`). The pre-push
#     hook can pass the exact range it received ($remote_sha..$local_sha).
#     An unresolvable explicit argument exits 2 (usage error -- a gate that
#     cannot evaluate must never silently pass).
#
# Four checks (ADR-013 §2), all preserved from the HEAD-only version:
#   1. Foreign WIP in the diff: tracked files modified/staged beyond the
#      range tip that are NOT part of any commit in the range; plus
#      low-confidence warning for untracked files whose basename matches a
#      path the range touches.
#   2. Port contracts: a commit that swaps port-like values (`:NNNN`,
#      `port = NNNNN`) in non-.toml files without any .toml in the SAME
#      commit. Evaluated per commit, because ADR-013 requires the config in
#      the same commit as the swap.
#   3. Impact declaration: EACH commit touching critical routes
#      (crates/tylluan-kernel/src/transport/, guilds/, integrations/,
#      tylluan*.toml) must carry a `## Impact` section in ITS OWN message.
#   4. Out-of-scope workload (heuristic, low-confidence, never fails
#      alone): modified files neither mentioned in any commit subject of
#      the range nor part of it.
#
# Regla de oro (ADR-013): the gate REPORTS, it never edits, reverts or blocks
# mechanically. Exit 1 means "findings exist -- resolve them before reporting
# this change as verified" (for an already-published commit the resolution is
# a RETROACTIVE `## Impact` note in the review report, never a history
# rewrite).
#
# Performance note (ADR-013 targets <1s): on Windows/MSYS every process spawn
# AND every bash subshell fork costs ~0.1-0.4s, so this script keeps the
# original discipline -- the whole range's messages AND patches arrive in ONE
# `git log` call (2 more for the working tree and untracked lists, +1 for the
# origin/main probe), and everything else (port values, file lists,
# critical-route matching, subject mentions) is parsed in PURE BASH with ZERO
# forks inside loops. Measured on the 2026-10-18 Windows/MSYS machine: 2.2s
# for the single-commit fallback and 4.0s for a 5-commit range -- above the
# ADR-013 <1s target on this OS (git spawns + the per-file sort in the port
# check dominate); acceptable for a report-only gate that runs once per
# push/CI job, and well under 1s on Linux/CI.
#
# Usage: scripts/check_no_predation.sh [rev-range|commit]
# Exit 0 if no hard findings (low-confidence findings are listed, count shown).
# Exit 1 if any hard finding (checks 1a/2/3) -- read the output, it names them.
# Exit 2 if an explicit argument cannot be resolved (usage error).
#
# Self-tested by scripts/test_check_no_predation.sh (T1275 fixtures: 2-commit
# range where the FIRST commit touches transport/ without Impact and the last
# is docs-only -> exit 1 -- the exact abc8534 escape; same range with Impact
# -> exit 0; single-commit and empty-repo regressions).

set -uo pipefail
cd "$(dirname "$0")/.."

problems=0
low_conf=0

# ── Range resolution ─────────────────────────────────────────────────────
range_arg="${1:-}"
defaulted=0
if [ -n "$range_arg" ]; then
    range="$range_arg"
elif git rev-parse --verify -q origin/main >/dev/null 2>&1; then
    range="origin/main..HEAD"
    defaulted=1
else
    range="HEAD"
    defaulted=1
fi

# Per-commit state, reset on every commit token.
cur_sha="" cur_subject="" cur_body="" cur_file="" tip_sha=""
in_msg=0
cur_files=()
declare -A cur_seenA=() port_add=() port_rem=()
# Range-wide state (never reset).
declare -A seen_files=() subject_lc_by_sha=()
ordered_shas=()
subjects_lc=""
commits_n=0
impact_findings=()
port_findings=()

short_sha() { local s="${cur_sha:0:12}"; printf '%s' "$s"; }

finalize_commit() {
    [ -z "$cur_sha" ] && return
    # ── Check 2: port contracts, per commit (config must ride the SAME
    #    commit as the swap -- ADR-013 §2). Pure bash, zero forks in loops. ──
    local toml_in=0 f r a a_sorted r_sorted
    local -a port_changed=()
    for f in "${cur_files[@]}"; do
        case "$f" in *.toml) toml_in=1 ;; esac
    done
    for f in "${cur_files[@]}"; do
        case "$f" in *.toml|"") continue ;; esac
        r="${port_rem[$f]:-}"
        a="${port_add[$f]:-}"
        [ -z "$r" ] && continue
        [ -z "$a" ] && continue   # pure removal of a port reference is not a migration (HEAD-only semantics, kept)
        a_sorted=$(printf '%s' "$a" | tr ' ' '\n' | sort -u | tr '\n' ' ')
        r_sorted=$(printf '%s' "$r" | tr ' ' '\n' | sort -u | tr '\n' ' ')
        if [ "$a_sorted" != "$r_sorted" ]; then
            port_changed+=("$f")
        fi
    done
    if [ "${#port_changed[@]}" -gt 0 ] && [ "$toml_in" -eq 0 ]; then
        for f in "${port_changed[@]}"; do
            port_findings+=("$(short_sha): '$f' cambia valores con forma de puerto sin que ningún .toml (tylluan.toml / tylluan.example.toml) forme parte del MISMO commit -- si es una migración de puerto real, la config va en el mismo commit (ADR-013 §2).")
        done
        problems=1
    fi

    # ── Check 3: impact declaration, per commit -- each commit touching
    #    critical routes carries `## Impact` in ITS OWN message. ──
    local -a crit=()
    for f in "${cur_files[@]}"; do
        case "$f" in
            crates/tylluan-kernel/src/transport/*|guilds/*|integrations/*|tylluan*.toml)
                crit+=("$f") ;;
        esac
    done
    if [ "${#crit[@]}" -gt 0 ]; then
        local body_lc="${cur_body,,}"
        if [[ $body_lc != *'## impact'* ]]; then
            for f in "${crit[@]}"; do
                impact_findings+=("$(short_sha): toca la ruta crítica '$f' sin sección '## Impact' en SU mensaje (ADR-013 §1). Corrige con: git commit --amend si aún no se ha empujado; si ya está publicado, nota Impact retroactiva en el informe de revisión (NUNCA reescribiendo historia).")
            done
            problems=1
        fi
    fi

    # Range-wide accumulation for checks 1a/1b/4 and the header.
    subjects_lc+=" ${cur_subject,,}"
    subject_lc_by_sha["$cur_sha"]="${cur_subject,,}"

    cur_sha="" cur_subject="" cur_body="" in_msg=0
    cur_files=(); port_add=(); port_rem=(); cur_seenA=()
}

extract_ports() {
    local line="$1" re='(:|port[[:space:]]*=|PORT[[:space:]]*=)[[:space:]]*([0-9]{4,5})'
    PORTS_OUT=""
    while [[ $line =~ $re ]]; do
        PORTS_OUT+="${BASH_REMATCH[2]} "
        line="${line#*"${BASH_REMATCH[2]}"}"
    done
}

evaluate_range() {
    commits_n=0
    ordered_shas=()
    subjects_lc=""
    impact_findings=()
    port_findings=()
    # One git call for the whole range: messages AND -U0 patches. A single
    # rev (HEAD fallback or explicit sha, i.e. no '..' in the range) needs
    # -1: `git log <sha>` would walk the ENTIRE history reachable from it
    # (caught live: the HEAD fallback parsed the whole repo, T1275).
    local -a gl=(git log --format=$'@@@NPTOKEN %H%n%s%n%b' -U0)
    case "$range" in
        *..*) gl+=(--no-merges) ;;
        *)    gl+=(-1) ;;
    esac
    while IFS= read -r line; do
        case "$line" in
            '@@@NPTOKEN '*)
                finalize_commit
                cur_sha="${line#@@@NPTOKEN }"
                ordered_shas+=("$cur_sha")
                commits_n=$((commits_n+1))
                [ -z "$tip_sha" ] && tip_sha="$cur_sha"   # git log lists tip first
                in_msg=1
                ;;
            'diff --git '*)
                in_msg=0
                cur_file="${line##* b/}"
                if [ -n "$cur_file" ] && [ -z "${cur_seenA[$cur_file]:-}" ]; then
                    cur_seenA["$cur_file"]=1
                    cur_files+=("$cur_file")
                    seen_files["$cur_file"]=1
                fi
                ;;
            +++*|---*|\\*) continue ;;
            +*) if [ "$in_msg" -eq 1 ]; then cur_body+="$line"$'\n'; else extract_ports "${line#+}"; [ -n "$PORTS_OUT" ] && port_add["$cur_file"]+="$PORTS_OUT "; fi ;;
            -*) if [ "$in_msg" -eq 1 ]; then cur_body+="$line"$'\n'; else extract_ports "${line#-}"; [ -n "$PORTS_OUT" ] && port_rem["$cur_file"]+="$PORTS_OUT "; fi ;;
            *)
                if [ "$in_msg" -eq 1 ]; then
                    if [ -z "$cur_subject" ] && [ -n "$cur_sha" ]; then
                        cur_subject="$line"
                    else
                        cur_body+="$line"$'\n'
                    fi
                fi
                ;;
        esac
    done < <("${gl[@]}" "$range" 2>/dev/null)
    finalize_commit
    [ "$commits_n" -gt 0 ]
}

# Empty-repo / empty-range guards (kept from the HEAD-only version).
if ! git rev-parse --verify -q HEAD >/dev/null 2>&1; then
    echo "✅ No HEAD commit to evaluate (empty repo)."
    exit 0
fi

if ! evaluate_range; then
    if [ -n "$range_arg" ]; then
        echo "❌ Rango explícito no resoluble: '$range_arg' -- un gate que no puede evaluar nunca pasa en silencio (error de uso)."
        exit 2
    fi
    if [ "$defaulted" -eq 1 ] && [ "$range" = "origin/main..HEAD" ]; then
        echo "ℹ️  origin/main..HEAD está vacío (todo ya empujado) -- se evalúa HEAD solo (uso clásico)."
        range="HEAD"
        tip_sha=""
        evaluate_range || { echo "✅ No HEAD commit to evaluate (empty repo)."; exit 0; }
    fi
fi

# git call: working-tree changes beyond the range tip.
mapfile -t wt_files < <(git diff --name-only "${tip_sha:-HEAD}" 2>/dev/null || true)
# git call: untracked files.
mapfile -t untracked < <(git ls-files --others --exclude-standard 2>/dev/null || true)

echo "Rango evaluado: $range ($commits_n commit(s))"
for sha in "${ordered_shas[@]}"; do
    echo "  - ${sha:0:12} ${subject_lc_by_sha[$sha]}"
done
echo "Files in range: ${#seen_files[@]} | tracked files modified beyond the tip: ${#wt_files[@]}"
echo ""

# ── Check 1a: foreign WIP -- tracked files modified/staged beyond the range
#    tip that NO commit in the range contains (someone else's uncommitted
#    work sitting in this checkout while I commit mine). ──
foreign_wip=()
for f in "${wt_files[@]}"; do
    [ -n "${seen_files[$f]:-}" ] && continue
    foreign_wip+=("$f")
done
if [ "${#foreign_wip[@]}" -gt 0 ]; then
    for f in "${foreign_wip[@]}"; do
        echo "❌ WIP ajeno: '$f' está modificado en el working tree pero NO forma parte de ningún commit del rango evaluado -- posible trabajo de otro agente en este checkout (ADR-013 §2)."
    done
    problems=1
fi

# ── Check 1b (low-confidence): untracked files whose basename matches a path
#    the range touches -- possible foreign/ignored file silently excluded. ──
declare -A range_bases=()
for f in "${!seen_files[@]}"; do range_bases["${f##*/}"]="$f"; done
for u in "${untracked[@]}"; do
    b="${u##*/}"
    [ -n "${range_bases[$b]:-}" ] || continue
    [ "${range_bases[$b]}" = "$u" ] && continue
    echo "⚠️  low-confidence: el archivo sin trackear '$u' coincide en nombre con '${range_bases[$b]}' que SÍ toca el rango, pero no forma parte de él -- posible WIP ajeno o ignorado."
    low_conf=$((low_conf+1))
done

# ── Per-commit findings collected during parsing. ──
if [ "${#port_findings[@]}" -gt 0 ]; then
    for m in "${port_findings[@]}"; do
        echo "❌ Contrato de puerto: $m"
    done
fi
if [ "${#impact_findings[@]}" -gt 0 ]; then
    for m in "${impact_findings[@]}"; do
        echo "❌ Declaración de impacto: $m"
    done
fi

# ── Check 4 (heuristic, low-confidence, never fails alone): out-of-scope
#    workload -- modified files neither mentioned in any commit subject of
#    the range nor part of it. ──
scope_hits=()
for f in "${wt_files[@]}"; do
    [ -n "${seen_files[$f]:-}" ] && continue
    base="${f##*/}"
    dir="${f%/*}"
    if [[ $subjects_lc == *"${base,,}"* || $subjects_lc == *"${dir,,}"* ]]; then
        continue
    fi
    scope_hits+=("$f")
done
if [ "${#scope_hits[@]}" -gt 0 ]; then
    echo "⚠️  low-confidence: ${#scope_hits[@]} archivo(s) modificado(s) fuera del alcance declarado del rango (no mencionados en ningún mensaje y no parte de él):"
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
echo "   Resuélvelos (o declara '## Impact' vía git commit --amend si aún no se empujó;"
echo "   si el commit ya está publicado, nota Impact retroactiva en el informe de revisión)"
echo "   antes de reportar el cambio como verificado (ADR-013)."
exit 1
