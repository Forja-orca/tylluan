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

# ---- 8. R2 guild count with the KERNEL's own criterion, not find|wc.
# scan_guilds_directory (crates/tylluan-kernel/src/router/catalog.rs:536+)
# counts: each direct subdirectory of guilds/ (skip _-prefixed), searching
# guilds/<fam>/plugins/ when it exists else guilds/<fam>/ itself; each *.py
# at that level whose stem does not start with '_'; the file must carry a
# FastMCP(...) marker (extract_guild_name, catalog.rs:353 — any line with
# FastMCP( whose inner name is non-empty and has no whitespace); then
# guild_name = name_override(stem) or stem (name_override: catalog.rs:167,
# scrapling_web->scrapling today) and EXCLUDED_GUILDS (catalog.rs:98, parsed
# below — never hardcoded here) are dropped. A naive find *.py | wc gives 64
# on this tree and would report drift where there is none AND hide drift
# where there is (H-26 audit). The claimed counts in the READMEs must match
# this number; "13 additional guilds" style historical lines are excluded
# from the claim set by keyword (additional/adicionales).
CAT="crates/tylluan-kernel/src/router/catalog.rs"
if [ -d guilds ] && [ -f "$CAT" ]; then
  excluded=$(grep -m1 'const EXCLUDED_GUILDS' "$CAT" | grep -oE '"[a-z0-9_]+"' | tr -d '"' | tr '\n' ' ')  # single-line const; -m1 — a /];/ range would swallow required_args from guild_overrides (real bug in first run)
  # name overrides: match arms `"stem" => "name",` inside fn name_override
  ov=$(sed -n '/fn name_override/,/^}/p' "$CAT" | grep -oE '"[a-z0-9_]+"[[:space:]]*=>[[:space:]]*"[a-z0-9_]+"' | sed -E 's/"([a-z0-9_]+)"[[:space:]]*=>[[:space:]]*"([a-z0-9_]+)"/\1=\2/')
  gcount=0; gskip_excl=0; gskip_nomark=0
  for fam in guilds/*/; do
    fambase=${fam#guilds/}; fambase=${fambase%/}
    case "$fambase" in _*) continue ;; esac
    sd="$fam"; [ -d "$fam/plugins" ] && sd="$fam/plugins"
    for p in "$sd"/*.py; do
      [ -e "$p" ] || continue
      stem=$(basename "$p" .py)
      case "$stem" in _*) continue ;; esac
      # extract_guild_name emulation: FastMCP( line with non-empty quoted-ish name
      mark="no"
      if grep -q "FastMCP(" "$p" && ! grep -q "FastMCP()" "$p"; then mark="yes"; fi
      [ "$mark" = "yes" ] || { gskip_nomark=$((gskip_nomark+1)); continue; }
      name="$stem"
      for pair in $ov; do
        [ "${pair%%=*}" = "$stem" ] && name="${pair#*=}"
      done
      hit=0
      for e in $excluded; do [ "$e" = "$name" ] && hit=1; done
      if [ "$hit" = "1" ]; then gskip_excl=$((gskip_excl+1)); continue; fi
      gcount=$((gcount+1))
    done
  done
  ok_r2=1
  for f in "$README_EN" "$README_ES"; do
    [ -f "$f" ] || continue
    # filter historical lines FIRST (keyword), then extract leading number
    # of each match — never grep -n: chaining -n makes ^[0-9]+ grab LINE
    # numbers (59, 217...) instead of claim values (real bug in first run).
    hist=$(grep -viE 'additional|adicionales' "$f" | grep -oiE '[0-9]+([[:space:]]+[a-z-]+){0,3}[[:space:]]+guilds' | grep -oE '^[0-9]+' | sort -un | tr '\n' ',' | sed 's/,$//')
    if [ "$hist" = "$gcount" ]; then
      report OK guildcount "$f claims exactly $gcount active guilds; kernel-criterion scan finds $gcount ($gskip_excl excluded via [$excluded], $gskip_nomark .py without FastMCP marker)"
    else
      report DRIFT guildcount "$f claims active-guild values {$hist} but kernel-criterion scan (scan_guilds_directory equivalent) finds $gcount"
      ok_r2=0
    fi
  done
  [ "$ok_r2" = "1" ] || true
else
  report DRIFT guildcount "cannot apply R2: guilds/ or $CAT missing"
fi

# ---- 9. R4 anchor liveness: every `path:LINE[-LINE]` anchor cited in
# ROADMAP.md/STATUS.md must (a) name a file that exists in the tree and
# (b) point at line numbers within that file's length. When the citing
# line is NOT a historical milestone entry (entries mentioning vN.N or
# ISO dates cite where code lived AT THE TIME — graph.rs:739 for the
# v0.10.0 degree bug is correct history, not drift), additionally (c)
# at least one code identifier belonging to the anchor's own SENTENCE
# segment (split on . ! ? | to avoid token bleed from neighbouring
# claims on long lines) — snake_case, camelCase, or any word inside
# backticks — must occur within LINE±5 of the cited file. Found live
# 2026-10-24 (H-30 audit + H-31 build): config.rs:2100 pointed at a
# struct while the dev_mode warning it describes lives at 2294+, and
# main.rs:1758-1772 cited wal_checkpoint wiring that lives at 2140.
# Ambiguous basenames (main.rs exists 5x) resolve across all tree
# candidates: OK if ANY satisfies range(+content); finds are cached per
# basename and prune target/.git/node_modules/dist (first run walked
# node_modules and timed out the whole suite).
# Test-injection override: CHECK_README_ANCHOR_DOCS replaces the docs
# list (space-separated) so clean/drift fixtures run without touching
# the real ROADMAP/STATUS — same injection pattern used for R2.
ANCHOR_DOCS_DEFAULT=("ROADMAP.md" "STATUS.md")
if [ -n "${CHECK_README_ANCHOR_DOCS:-}" ]; then
  read -r -a ANCHOR_DOCS <<< "$CHECK_README_ANCHOR_DOCS"
else
  ANCHOR_DOCS=("${ANCHOR_DOCS_DEFAULT[@]}")
fi
anchors_total=0; anchors_bad=0; BT='`'
declare -A find_cache=()
for doc in "${ANCHOR_DOCS[@]}"; do
  [ -f "$doc" ] || { report DRIFT anchors "anchor doc $doc missing"; anchors_bad=$((anchors_bad+1)); continue; }
  ln=0
  while IFS= read -r line || [ -n "$line" ]; do
    ln=$((ln+1))
    # pure-bash guard: skip lines that cannot host an anchor (MSYS fork
    # per pipeline is ~100ms; 600 lines x grep would time the suite out)
    case "$line" in *.rs:*|*.toml:*|*.yml:*|*.py:*|*.md:*|*.sh:*|*.ps1:*|*.json:*) ;; *) continue ;; esac
    hist=0
    case "$line" in *v[0-9]*.[0-9]*|*[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]*|*estaba*|*hist*|*fixed*|*was*|*fue\ un*) hist=1 ;; esac
    for a in $(printf '%s\n' "$line" | grep -oE '[A-Za-z0-9_./-]+\.(rs|toml|yml|py|md|sh|ps1|json):[0-9]+(-[0-9]+)?'); do
      anchors_total=$((anchors_total+1))
      afile=${a%%:*}; rest=${a#*:}; l1=${rest%%-*}; l2=${rest##*-}
      seg=$(printf '%s\n' "$line" | sed 's/[.!?|]/\n/g' | grep -F -- "$a" | head -1)
      [ -n "$seg" ] || seg=$(printf '%s\n' "$line" | sed 's/[.!?|]/\n/g' | grep -F -- "$afile:" | head -1)
      [ -n "$seg" ] || seg="$line"
      seg=${seg//$a/}   # drop the anchor itself: its dots/slashes otherwise become junk tokens through tr/grep
      code_toks=$( { printf '%s' "$seg" | grep -oE '[A-Za-z][A-Za-z0-9]*_[A-Za-z0-9_]+|[a-z]+[A-Z][A-Za-z0-9]*'
                    printf '%s' "$seg" | grep -oE "${BT}[^${BT}]{3,150}${BT}" | grep -oE '[A-Za-z_][A-Za-z0-9_]{3,}'
                  } 2>/dev/null | sort -u | tr '\n' ' ')
      bkey=$(basename "$afile")
      if [ -f "$afile" ]; then cands="$afile"
      elif [ -n "${find_cache[$bkey]+x}" ]; then cands="${find_cache[$bkey]}"
      else cands=$(find . \( -name target -o -name .git -o -name node_modules -o -name dist \) -prune -o -type f -name "$bkey" -print 2>/dev/null | sed 's|^\./||'); find_cache[$bkey]="$cands"; fi
      if [ -z "$cands" ]; then
        report DRIFT anchors "$doc:$ln $a — file $bkey not found in the tree"
        anchors_bad=$((anchors_bad+1)); continue
      fi
      hit_any=0; any_inrange=0; range_note=""; voc_miss_note=""
      for cand in $cands; do
        nl=$(wc -l < "$cand")
        if [ "$l1" -gt "$nl" ] || [ "$l2" -gt "$nl" ]; then range_note="$cand has only $nl lines (cited $l1-$l2)"; continue; fi
        any_inrange=1
        [ "$hist" = 1 ] && { hit_any=1; break; }
        lo=$((l1>5 ? l1-5 : 1)); hi=$((l2+5))
        window=$(sed -n "${lo},${hi}p" "$cand")
        if [ -z "${code_toks//[[:space:]]/}" ]; then hit_any=1; break; fi
        for t in $code_toks; do
          if printf '%s' "$window" | grep -qw -- "$t"; then hit_any=1; break; fi
        done
        if [ "$hit_any" = 1 ]; then break; fi
        # One whole-file grep (OR of tokens) decides whether this file's
        # own vocabulary is even in play: if tokens live elsewhere in the
        # same file, the anchor is stale (content moved away); if NO
        # candidate contains the tokens at all, the sentence is not
        # describing these files (absence/cross-file claims) and we do
        # not over-claim drift. A candidate that lacks the vocabulary
        # must never grant OK while another candidate has it (real bug
        # caught by the H-31 drift fixture: main.rs:100 + wal_checkpoint
        # passed via cli main.rs while kernel main.rs has it at 2140).
        if grep -qw $(printf '%s' "$code_toks" | awk '{for(i=1;i<=NF;i++) printf "-e%s ",$i}') -- "$cand" 2>/dev/null; then
          voc_miss_note="$voc_miss_note $cand (tokens live elsewhere in it)"
        fi
      done
      if [ "$hit_any" = 1 ] && [ "$hist" = 1 ]; then
        report OK anchors "$doc:$ln $a — file resolves, lines in range (historical line: content check skipped by design)"
      elif [ "$hit_any" = 1 ]; then
        report OK anchors "$doc:$ln $a — file resolves, lines in range, clause content verified near anchor"
      elif [ "$any_inrange" = 0 ]; then
        report DRIFT anchors "$doc:$ln $a — cited line(s) beyond EOF ($range_note)"
        anchors_bad=$((anchors_bad+1))
      elif [ -n "$voc_miss_note" ]; then
        report DRIFT anchors "$doc:$ln $a — file and lines exist, but the clause tokens occur only elsewhere in the file, not near the cited lines (anchor stale, content moved):$voc_miss_note"
        anchors_bad=$((anchors_bad+1))
      else
        report OK anchors "$doc:$ln $a — file resolves, lines in range; no clause token exists in any candidate file (claim is not describing these files' content — existence+range only)"
      fi
    done
  done < "$doc"
done
if [ "$anchors_total" -gt 0 ] && [ "$anchors_bad" -eq 0 ]; then
  report OK anchors "all $anchors_total path:line anchors in ${ANCHOR_DOCS[*]} live and content-backed"
fi

if [ "$drifts" -gt 0 ]; then
  echo "RESULT: $drifts DRIFT line(s) — README claims diverge from the tree."
  exit 1
fi
echo "RESULT: all README claims verified against the tree (read-only; live test counts remain gated by check_test_count.sh in CI)."
exit 0
