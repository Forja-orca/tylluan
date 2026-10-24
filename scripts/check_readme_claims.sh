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

# ---- 6. R1 bilingual parity: the SET of claimed values per unit noun must
# match between README.md and README.es.md. Deliberately a value SET, not a
# frequency multiset: prose structure legitimately differs across languages
# (word order, how often a number is repeated), and locale-formatted figures
# (latency "ms", percentages in profile tables) are excluded to avoid false
# positives — verified live: en/es differ on those today with no drift.
# Catches the 1125/1329 class on ANY shared claim (tests, questions, guilds),
# not only tests as section 2 does against AGENTS.
claim_set() { # $1=file  -> sorted unique numeric values for claim nouns
  # "tools/herramientas" deliberately NOT here: sovereign tool count is
  # already gated name-by-name against tools.rs in section 1, bilingually.
  grep -oiE '[0-9]+([[:space:]]+[a-z-]+){0,3}[[:space:]]+(tests|questions|preguntas|guilds)' "$1" \
    | grep -oE '^[0-9]+' | sort -un | tr '\n' ','
}
semver_set() { grep -oE 'v[0-9]+\.[0-9]+\.[0-9]+' "$1" | sort -u | tr '\n' ','; }
if [ -f "$README_EN" ] && [ -f "$README_ES" ]; then
  en_n=$(claim_set "$README_EN"); es_n=$(claim_set "$README_ES")
  if [ "$en_n" = "$es_n" ]; then
    report OK bilingual "numeric claim values (tests/questions/guilds) identical in README.md and README.es.md: $en_n"
  else
    report DRIFT bilingual "README.md claims {value set: $en_n} but README.es.md claims {$es_n} — one language is stale"
  fi
  en_v=$(semver_set "$README_EN"); es_v=$(semver_set "$README_ES")
  if [ "$en_v" = "$es_v" ]; then
    report OK bilingual "version mentions match across READMEs: $en_v"
  else
    report DRIFT bilingual "README.md mentions versions {$en_v} but README.es.md mentions {$es_v}"
  fi
fi

# ---- 7. R3 benchmark backing: every Recall@/Latency figure in a benchmark
# table must exist as a number in the benchmarks/*.json cited by that README,
# and the table must carry the historical-run disclaimer (version + date).
# Found live 2026-10-24 (H-26 audit): numbers currently back correctly
# (82/90/46/12.9 in benchmarks/longmemeval_v0.12.0.json); the rule exists to
# keep fabricated or drifted figures out of future edits.
bench_json=$(grep -hoE 'benchmarks/[A-Za-z0-9_.-]+\.json' "$README_EN" "$README_ES" 2>/dev/null | sort -u | head -1)
if [ -n "${bench_json:-}" ] && [ -f "$bench_json" ]; then
  claimed=0; backed=0; missing=""
  for f in "$README_EN" "$README_ES"; do
    [ -f "$f" ] || continue
    while IFS='|' read -r label value; do
      label=$(echo "$label" | tr -d ' '); value=$(echo "$value" | tr -d ' |%ms'); val=${value%%.*}
      case "$label" in
        Recall@*) key="" ;;                      # any Recall@K: match the number anywhere in the JSON
        Latency*|Latencia*) key="" ;;
        *) continue ;;
      esac
      claimed=$((claimed+1))
      if grep -qE "(^|[^0-9.])${val}(\.0)?([^0-9]|$)" "$bench_json"; then
        backed=$((backed+1))
      else
        missing="$missing $f:$label=$value"
      fi
    done < <(grep -E '^\|[[:space:]]*(Recall@|Latency|Latencia)' "$f" | sed -E 's/^\|([^|]*)\|([^|]*)\|.*/\1|\2/')
  done
  if [ "$claimed" -gt 0 ] && [ "$backed" -eq "$claimed" ]; then
    report OK benchmark "$claimed benchmark table figures all present in $bench_json"
  elif [ "$claimed" -gt 0 ]; then
    report DRIFT benchmark "benchmark figures without backing value in $bench_json:$missing"
  fi
  for f in "$README_EN" "$README_ES"; do
    [ -f "$f" ] || continue
    grep -E '^\|[[:space:]]*(Recall@|Latency|Latencia)' "$f" >/dev/null || continue
    if grep -B10 -E '^\|[[:space:]]*(Recall@|Latency|Latencia)' "$f" | grep -qE 'v[0-9]+\.[0-9]+\.[0-9]+.{0,80}\(?[0-9]{4}-[0-9]{2}-[0-9]{2}'; then
      report OK benchmark "$f benchmark table carries version+date historical disclaimer"
    else
      report DRIFT benchmark "$f benchmark table lacks a version+date disclaimer within 10 lines (run provenance must be explicit)"
    fi
  done
fi

if [ "$drifts" -gt 0 ]; then
  echo "RESULT: $drifts DRIFT line(s) — README claims diverge from the tree."
  exit 1
fi
echo "RESULT: all README claims verified against the tree (read-only; live test counts remain gated by check_test_count.sh in CI)."
exit 0
