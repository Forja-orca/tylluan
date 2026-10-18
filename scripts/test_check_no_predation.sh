#!/usr/bin/env bash
# scripts/test_check_no_predation.sh -- fixtures for the ADR-013 gate's
# RANGE evaluation (T1275, 2026-10-18).
#
# Why this exists: the gate evaluated only HEAD, so a multi-commit push
# slipped through -- pre-push saw the docs-only tip and never looked at the
# earlier commit (abc8534) that touched transport/ without `## Impact`.
# These fixtures lock the fixed semantics in disposable repos:
#
#   F1  range of 2 commits: FIRST touches transport/ WITHOUT Impact, last is
#       docs-only (the exact abc8534 escape)           -> exit 1, names sha
#   F2  same range, FIRST carries `## Impact`          -> exit 0
#   F3  explicit range argument `HEAD~2..HEAD`         -> F1 repo exit 1,
#                                                         F2 repo exit 0
#   F4  single-commit fallback (no origin/main ref):   transport w/o Impact
#       -> exit 1 (HEAD-only usage preserved); with Impact -> exit 0
#   F5  empty repo (no HEAD)                           -> exit 0
#   F6  unresolvable explicit argument                 -> exit 2
#
# Every fixture builds a throwaway git repo under mktemp, copies the LIVE
# gate into it (the gate cd's to its own repo root, so it must live in
# <repo>/scripts/), runs it and asserts the exit code. The real checkout,
# its refs and its working tree are never modified. Hermetic: no network,
# no cargo, no processes outside test-run temp dirs.
#
# Usage: scripts/test_check_no_predation.sh
# Exit 0 if all fixtures pass; exit 1 with a summary otherwise.

set -uo pipefail
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GATE_SRC="$REPO_ROOT/scripts/check_no_predation.sh"

if [ ! -f "$GATE_SRC" ]; then
    echo "❌ $GATE_SRC not found -- run from the repo root."
    exit 2
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

failures=0
t()   { printf '%s\n' "-- $1"; }
okt() { echo "   ✅ $1"; }
badt() { echo "   ❌ $1"; failures=$((failures+1)); }

# make_repo <dir> [--no-origin]: fresh repo with the live gate committed.
make_repo() {
    local d="$1"
    mkdir -p "$d/scripts"
    git -C "$d" init -q
    git -C "$d" config user.email "gate-fixture@test.local"
    git -C "$d" config user.name "gate-fixture"
    git -C "$d" config commit.gpgsign false
    cp "$GATE_SRC" "$d/scripts/check_no_predation.sh"
    mkdir -p "$d/docs"
    echo "fixture harness" > "$d/docs/harness.md"
    git -C "$d" add -A
    git -C "$d" commit -qm "chore: fixture harness with gate"
}

# add_transport_commit <dir> <message...>: touches transport/ (critical route).
add_transport_commit() {
    local d="$1"; shift
    mkdir -p "$d/crates/tylluan-kernel/src/transport/http"
    echo "pub fn route() {} // fixture" > "$d/crates/tylluan-kernel/src/transport/http/fixture_route.rs"
    git -C "$d" add -A
    git -C "$d" commit -qm "$*"
}

# add_docs_commit <dir> <message...>: docs-only tip commit.
add_docs_commit() {
    local d="$1"; shift
    echo "fixture note" >> "$d/docs/harness.md"
    git -C "$d" add -A
    git -C "$d" commit -qm "$*"
}

# set_origin_main <dir>: point refs/remotes/origin/main at HEAD (no remote needed).
set_origin_main() {
    local d="$1"
    git -C "$d" update-ref refs/remotes/origin/main "$(git -C "$d" rev-parse HEAD)"
}

run_gate() {   # run_gate <repo> [arg...] -> GATE_OUT / GATE_RC
    local d="$1"; shift
    GATE_OUT=$( (cd "$d" && timeout 60 bash scripts/check_no_predation.sh "$@") 2>&1 )
    GATE_RC=$?
}

# ── F1: FIRST commit touches transport/ without Impact; last is docs-only ──
t "F1: rango de 2 commits, el PRIMERO toca transport/ sin Impact y el ultimo es solo docs -> exit 1"
f1="$TMP/f1"; make_repo "$f1"
set_origin_main "$f1"
add_transport_commit "$f1" "feat(api): new diagnostic endpoint"
c1_sha=$(git -C "$f1" rev-parse --short=12 HEAD)
add_docs_commit "$f1" "docs: update note"
run_gate "$f1"
if [ "$GATE_RC" -eq 1 ] \
   && echo "$GATE_OUT" | grep -q "$c1_sha" \
   && echo "$GATE_OUT" | grep -q "## Impact" \
   && echo "$GATE_OUT" | grep -q "transport/http/fixture_route.rs"; then
    okt "exit 1, nombra al commit infractor ($c1_sha) y la ruta critica"
else
    badt "esperaba exit 1 nombrando $c1_sha, got $GATE_RC"; echo "$GATE_OUT" | tail -8
fi

# ── F2: same shape, FIRST commit carries ## Impact -> exit 0 ─────────────
t "F2: mismo rango con '## Impact' en el PRIMER commit -> exit 0"
f2="$TMP/f2"; make_repo "$f2"
set_origin_main "$f2"
add_transport_commit "$f2" $'feat(api): new diagnostic endpoint\n\n## Impact\n- dos rutas GET nuevas clase operador, sin escrituras'
add_docs_commit "$f2" "docs: update note"
run_gate "$f2"
if [ "$GATE_RC" -eq 0 ]; then
    okt "exit 0"
else
    badt "esperaba exit 0, got $GATE_RC"; echo "$GATE_OUT" | tail -8
fi

# ── F3: explicit range argument locks the optional-arg usage ────────────
t "F3: argumento explicito HEAD~2..HEAD -> F1 exit 1 / F2 exit 0"
run_gate "$f1" "HEAD~2..HEAD"
if [ "$GATE_RC" -eq 1 ] && echo "$GATE_OUT" | grep -q "## Impact"; then
    okt "rango explicito sin Impact -> exit 1"
else
    badt "esperaba exit 1, got $GATE_RC"; echo "$GATE_OUT" | tail -5
fi
run_gate "$f2" "HEAD~2..HEAD"
if [ "$GATE_RC" -eq 0 ]; then
    okt "rango explicito con Impact -> exit 0"
else
    badt "esperaba exit 0, got $GATE_RC"; echo "$GATE_OUT" | tail -5
fi

# ── F4: single-commit fallback (no origin/main ref) -> HEAD-only usage ──
t "F4: sin origin/main, commit suelto toca transport/ -> exit 1 (sin Impact) y exit 0 (con Impact)"
f4a="$TMP/f4a"; make_repo "$f4a"          # no set_origin_main on purpose
add_transport_commit "$f4a" "feat(api): endpoint sin impact"
run_gate "$f4a"
if [ "$GATE_RC" -eq 1 ] && echo "$GATE_OUT" | grep -q "## Impact"; then
    okt "commit suelto sin Impact -> exit 1"
else
    badt "esperaba exit 1, got $GATE_RC"; echo "$GATE_OUT" | tail -5
fi
f4b="$TMP/f4b"; make_repo "$f4b"
add_transport_commit "$f4b" $'feat(api): endpoint con impact\n\n## Impact\n- nueva ruta GET, sin escrituras'
run_gate "$f4b"
if [ "$GATE_RC" -eq 0 ]; then
    okt "commit suelto con Impact -> exit 0"
else
    badt "esperaba exit 0, got $GATE_RC"; echo "$GATE_OUT" | tail -5
fi

# ── F5: empty repo (no HEAD) -> exit 0 ──────────────────────────────────
t "F5: repo vacio (sin HEAD) -> exit 0"
f5="$TMP/f5"; mkdir -p "$f5/scripts"
cp "$GATE_SRC" "$f5/scripts/check_no_predation.sh"
git -C "$f5" init -q
run_gate "$f5"
if [ "$GATE_RC" -eq 0 ]; then
    okt "exit 0"
else
    badt "esperaba exit 0, got $GATE_RC"; echo "$GATE_OUT" | tail -5
fi

# ── F6: unresolvable explicit range -> exit 2 (never silently pass) ─────
t "F6: rango explicito no resoluble -> exit 2"
run_gate "$f1" "no-such-ref..HEAD"
if [ "$GATE_RC" -eq 2 ]; then
    okt "exit 2 (error de uso, no un passe silencioso)"
else
    badt "esperaba exit 2, got $GATE_RC"; echo "$GATE_OUT" | tail -5
fi

echo
if [ "$failures" -eq 0 ]; then
    echo "✅ check_no_predation.sh fixtures: todas las fixtures en verde (rango completo, Impact por commit, uso clasico preservado)."
    exit 0
else
    echo "❌ $failures fixture(s) fallaron -- el gate de ADR-013 no cumple el contrato de rango (T1275)."
    exit 1
fi
