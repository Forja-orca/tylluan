#!/usr/bin/env bash
# check_readme_claims.sh — deterministic README guardian (read-only).
#
# WHY: README.md/README.es.md carry verifiable numeric and existence claims
# (sovereign tool count, test counts, version, guild files, file links).
# check_test_count.sh already forces test counts against LIVE cargo runs in CI,
# but nothing cross-checks the Spanish README or the rest of the claims
# statically. Found live on first run (2026-10-24, main 7b24da2): README.es.md
# claimed "1125 tests" while README.md and AGENTS.md agree on 1329.
#
# Read-only: never edits the READMEs, never runs cargo, never touches network.
# One line per claim: OK|DRIFT <category>: <detail>. Exit 1 if any DRIFT.
# Wiring into verify.sh is the TL's call, not this script's job.

set -u
cd "$(git rev-parse --show-toplevel)" || exit 2
drifts=0
report() { # $1 OK|DRIFT  $2 category  $3 detail
  printf '%s %s: %s\n' "$1" "$2" "$3"
  [ "$1" = "DRIFT" ] && drifts=$((drifts+1))
  return 0
}

TOOLS_RS="crates/tylluan-kernel/src/transport/server/tools.rs"
README_EN="README.md"; README_ES="README.es.md"; AGENTS="AGENTS.md"
CARGO="Cargo.toml"

# ---- 1. Sovereign tool count: tools.rs is the source of truth (CONTRACT-01)
if [ -f "$TOOLS_RS" ]; then
  code_n=$(sed -n '/sovereign_tool_names()/,/^    }/p' "$TOOLS_RS" | grep -oE '"tylluan_[a-z_]+"' | sort -u | wc -l | tr -d ' ')
  report OK tools "tools.rs declares $code_n sovereign tools (source of truth)"
  for f in "$README_EN" "$README_ES"; do
    [ -f "$f" ] || { report DRIFT tools "$f missing"; continue; }
    read_n=$(grep -oE 'tylluan_(do|remember|recall|think|graph|decide)' "$f" | sort -u | wc -l | tr -d ' ')
    if [ "$read_n" -eq "$code_n" ]; then
      report OK tools "$f lists exactly $read_n sovereign tool names"
    else
      report DRIFT tools "$f lists $read_n distinct tylluan_* tool names but tools.rs declares $code_n"
    fi
    # every named tool in the README must exist in the code list
    for t in $(grep -oE 'tylluan_(do|remember|recall|think|graph|decide)' "$f" | sort -u); do
      grep -q "\"$t\"" "$TOOLS_RS" || report DRIFT tools "$f names $t but tools.rs does not declare it"
    done
    # count claim in prose ("six tools" / "seis herramientas")
    if grep -qiE '(six|seis)[[:space:]]+(sovereign[[:space:]]+)?tools|herramientas[[:space:]]+soberanas[[:space:]]+exactamente?[[:space:]]+seis|exactamente[[:space:]]+seis' "$f" && [ "$code_n" -ne 6 ]; then
      report DRIFT tools "$f prose claims six tools but code declares $code_n"
    fi
  done
else
  report DRIFT tools "$TOOLS_RS not found"
fi

# ---- 2. Test counts: AGENTS.md per-component sum vs README claims
# (live per-suite counts are gated by check_test_count.sh in CI; here we
#  cross-check the numbers the docs publish against each other.)
if [ -f "$AGENTS" ]; then
  read -r a b c total <<<"$(grep -E '\*\*Tests:\*\*' "$AGENTS" | head -1 | grep -oE '[0-9]+' | head -4 | tr '\n' ' ')"
  if [ -n "${total:-}" ] && [ $((a+b+c)) -eq "$total" ]; then
    report OK tests "AGENTS.md component sum $a+$b+$c=$total is internally consistent"
  else
    report DRIFT tests "AGENTS.md claims $a+$b+$c but total says ${total:-?}"
  fi
  for f in "$README_EN" "$README_ES"; do
    [ -f "$f" ] || continue
    claim=$(grep -oE '[0-9]+ tests' "$f" | head -1 | grep -oE '[0-9]+')
    if [ -z "${claim:-}" ]; then report DRIFT tests "$f has no test-count claim to check"; continue; fi
    if [ "$claim" -eq "$total" ]; then
      report OK tests "$f claims $claim tests; AGENTS.md total is $total"
    else
      report DRIFT tests "$f claims $claim tests but AGENTS.md total is $total"
    fi
  done
else
  report DRIFT tests "$AGENTS not found"
fi

# ---- 3. Version: workspace Cargo.toml vs README badges/headings
real_v=$(grep -m1 '^version *= *"' "$CARGO" | sed -E 's/.*"([^"]+)".*/\1/')
[ -n "$real_v" ] || { report DRIFT version "cannot read version from $CARGO"; real_v=""; }
for f in "$README_EN" "$README_ES"; do
  [ -f "$f" ] || continue
  bad=0
  for v in $(grep -oE 'version-[0-9]+\.[0-9]+\.[0-9]+' "$f" | sed 's/version-//' | sort -u); do
    if [ "$v" != "$real_v" ]; then report DRIFT version "$f badge says v$v, $CARGO says v$real_v"; bad=1; fi
  done
  [ "$bad" -eq 0 ] && report OK version "$f version badges match $CARGO (v$real_v)"
done

# ---- 4. Guild mentions: `*.py` tokens cited in READMEs must exist under guilds/
for f in "$README_EN" "$README_ES"; do
  [ -f "$f" ] || continue
  found_any=0
  for tok in $(grep -oE '`[A-Za-z0-9_./-]+\.py`' "$f" | tr -d '`' | sort -u); do
    found_any=1
    case "$tok" in
      guilds/*) [ -e "$tok" ] && report OK guilds "$f cites $tok and it exists" || report DRIFT guilds "$f cites $tok but the path does not exist" ;;
      *) hit=$(find guilds/ -name "$tok" 2>/dev/null | head -1)
         [ -n "$hit" ] && report OK guilds "$f cites $tok and it exists at $hit" || report DRIFT guilds "$f cites $tok but no such file under guilds/" ;;
    esac
  done
  [ "$found_any" -eq 0 ] && report OK guilds "$f cites no guild .py files by path (nothing to check)"
done

# ---- 5. File links: relative markdown links must point at existing files
for f in "$README_EN" "$README_ES"; do
  [ -f "$f" ] || continue
  okc=0; badc=0
  for link in $(grep -oE '\]\(([^)h#][^)]*)\)' "$f" | sed -E 's/^\]\(//; s/\)$//; s/#.*$//' | sort -u); do
    [ -z "$link" ] && continue
    if [ -e "$link" ]; then okc=$((okc+1)); else report DRIFT links "$f links to $link but the file does not exist"; badc=$((badc+1)); fi
  done
  report OK links "$f: $okc relative file links exist, $badc broken"
done

if [ "$drifts" -gt 0 ]; then
  echo "RESULT: $drifts DRIFT line(s) — README claims diverge from the tree."
  exit 1
fi
echo "RESULT: all README claims verified against the tree (read-only; live test counts remain gated by check_test_count.sh in CI)."
exit 0
