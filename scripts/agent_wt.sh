#!/usr/bin/env bash
# agent_wt.sh — worktrees de agentes baratos y desechables (protocolo: docs/internal/PROTOCOLO_worktrees_agentes.md)
#
# Uso (desde cualquier sitio):  bash /e/tylluan/scripts/agent_wt.sh <orden> [agente] [opciones]
#
#   status                    estado de todos los worktrees (HEAD, limpio o sucio, carpeta de compilación)
#   setup  <agente>           deja el worktree listo: compilación en E:\cargo-target\<agente> y caché de modelos compartida
#   reset  <agente>           pone el worktree en origin/main SIN perder nada: si hay cambios los guarda antes
#   gc     [--days N] [--apply]   limpia carpetas de compilación sin uso (por defecto solo informa; N=7)
#   migrate <agente>          borra la carpeta `target` antigua que quedó DENTRO del worktree (tras setup)
#
# Reglas que cumple este script:
#   - nunca borra cambios sin guardar: reset los guarda en E:\tylluan-patches\<agente>\_saved\ y en un stash
#   - nunca toca E:\tylluan (checkout principal, producción) ni su carpeta target
#   - no borra nada mientras haya cargo o rustc en marcha
#   - no comitea, no empuja, no toca ningún token

set -u

MAIN_REPO="/e/tylluan"
TARGET_ROOT="/e/cargo-target"
PATCH_ROOT="/e/tylluan-patches"
FASTEMBED_SHARED="E:/tylluan/.fastembed_cache"
AGENTS="deep buffy dsh antigravity codex readme hermes scratch"

die() { echo "ERROR: $*" >&2; exit 1; }

# T-DSH-62: bajo WSL (bash por defecto en algunos hosts) /e/... no existe y status pintaba
# una tabla vacia con exit 0. Mejor fallar claro.
[ -d "$MAIN_REPO" ] || die "MAIN_REPO=$MAIN_REPO no existe: usa Git Bash (MSYS), no WSL"

wt_path() {
  case "$1" in
    hermes) echo "/d/hermes-projects/tylluan-wt-hermes" ;;
    *)      echo "/e/tylluan-wt-$1" ;;
  esac
}

valid_agent() {
  for a in $AGENTS; do [ "$a" = "$1" ] && return 0; done
  return 1
}

need_agent() {
  [ -n "${1:-}" ] || die "falta el nombre del agente ($AGENTS)"
  valid_agent "$1" || die "agente desconocido: $1 (válidos: $AGENTS)"
  [ -d "$(wt_path "$1")/.git" ] || [ -f "$(wt_path "$1")/.git" ] || die "no existe el worktree $(wt_path "$1")"
}

cargo_running() {
  tasklist 2>/dev/null | grep -qiE '^(cargo|rustc)\.exe'
}

win_path() { cygpath -m "$1"; }

# T-DSH-63: borrado recursivo portable. `cmd //c rd` es idioma de Git Bash y falla en
# otros shells; MSYS2_ARG_CONV_EXCL evita que MSYS convierta /c, /s y /q en rutas.
win_rm() { # $1 = ruta POSIX a borrar (recursivo)
  local p="$1" wp
  if command -v cmd.exe >/dev/null 2>&1; then
    wp="$p"
    command -v cygpath >/dev/null 2>&1 && wp="$(cygpath -w "$p")"
    MSYS2_ARG_CONV_EXCL='*' cmd.exe /c rd /s /q "$wp" >/dev/null 2>&1 && return 0
  fi
  rm -rf -- "$p"
}

cmd_setup() {
  need_agent "${1:-}"
  local a="$1" wt tgt
  wt="$(wt_path "$a")"
  tgt="$TARGET_ROOT/$a"
  mkdir -p "$tgt" "$wt/.cargo"
  cat > "$wt/.cargo/config.toml" <<EOF
# Generado por scripts/agent_wt.sh setup (no se comitea: está en .git/info/exclude).
# Compilación fuera del worktree para que sea desechable y fácil de limpiar.
[build]
target-dir = "$(win_path "$tgt")"

[env]
# Caché de modelos compartida (solo lectura en la práctica): no se copian modelos a cada worktree.
FASTEMBED_CACHE_DIR = { value = "$FASTEMBED_SHARED", force = false }
EOF
  local excl
  excl="$(git -C "$wt" rev-parse --git-common-dir)/info/exclude"
  mkdir -p "$(dirname "$excl")"
  grep -qxF '.cargo/config.toml' "$excl" 2>/dev/null || echo '.cargo/config.toml' >> "$excl"
  echo "OK setup $a: compilación en $(win_path "$tgt")"
}

cmd_reset() {
  need_agent "${1:-}"
  local a="$1" wt ts saved
  wt="$(wt_path "$a")"
  ts="$(date +%Y%m%d_%H%M%S)"
  if [ -n "$(git -C "$wt" status --porcelain)" ]; then
    saved="$PATCH_ROOT/$a/_saved"
    mkdir -p "$saved/$ts.untracked"
    # 1) cambios en ficheros seguidos por git, como parche
    git -C "$wt" diff HEAD > "$saved/$ts.patch" || die "no pude guardar el parche; no toco nada"
    # 2) ficheros nuevos sin seguir: copia literal conservando rutas
    git -C "$wt" ls-files --others --exclude-standard > "$saved/$ts.untracked.txt"
    while IFS= read -r f; do
      [ -n "$f" ] || continue
      mkdir -p "$saved/$ts.untracked/$(dirname "$f")"
      cp -p "$wt/$f" "$saved/$ts.untracked/$f" 2>/dev/null || die "no pude copiar $f; no toco nada"
    done < "$saved/$ts.untracked.txt"
    git -C "$wt" status --porcelain > "$saved/$ts.status.txt"
    # 3) además, un stash completo (incluye lo no seguido)
    git -C "$wt" stash push -u -m "reset-$a-$ts" >/dev/null || die "stash falló; no toco nada"
    echo "GUARDADO: parche $saved/$ts.patch, ficheros nuevos en $saved/$ts.untracked/ y stash 'reset-$a-$ts'"
  fi
  git -C "$wt" fetch -q origin || die "git fetch falló"
  git -C "$wt" checkout -q --detach origin/main || die "checkout falló"
  cmd_setup "$a"
  echo "OK reset $a: $(git -C "$wt" log -1 --format='%h %s' | cut -c1-80)"
}

cmd_status() {
  # T-DSH-63: el fetch es opt-in (AGENT_WT_FETCH=1). `status` no debe depender de la red
  # ni pagar ~1 s de fetch en cada ejecución.
  if [ "${AGENT_WT_FETCH:-0}" = "1" ]; then
    git -C "$MAIN_REPO" fetch -q origin 2>/dev/null
  fi
  local main_head
  main_head="$(git -C "$MAIN_REPO" rev-parse --short origin/main)"
  echo "origin/main = $main_head"
  printf '%-12s %-9s %-8s %-9s %s\n' AGENTE HEAD CAMBIOS ATRASO COMPILACION
  for a in $AGENTS; do
    local wt
    wt="$(wt_path "$a")"
    if [ ! -e "$wt/.git" ]; then printf '%-12s %s\n' "$a" "(sin worktree)"; continue; fi
    local head dirty behind tgt
    head="$(git -C "$wt" rev-parse --short HEAD)"
    dirty="$(git -C "$wt" status --porcelain | wc -l | tr -d ' ')"
    behind="$(git -C "$wt" rev-list --count HEAD..origin/main 2>/dev/null || echo '?')"
    if [ -f "$wt/.cargo/config.toml" ] && grep -q 'target-dir' "$wt/.cargo/config.toml"; then tgt="compartida ($TARGET_ROOT/$a)"
    elif [ -d "$wt/target" ]; then tgt="EN EL WORKTREE (antigua: usar setup y migrate)"
    else tgt="(ninguna)"; fi
    printf '%-12s %-9s %-8s %-9s %s\n' "$a" "$head" "$dirty" "$behind" "$tgt"
  done
}

cmd_migrate() {
  need_agent "${1:-}"
  local a="$1" wt
  wt="$(wt_path "$a")"
  [ -f "$wt/.cargo/config.toml" ] || die "ejecuta primero: agent_wt.sh setup $a"
  cargo_running && die "hay cargo/rustc en marcha: espera a que acabe"
  [ -d "$wt/target" ] || { echo "OK $a: no hay target dentro del worktree"; return 0; }
  echo "borrando $wt/target (regenerable)"
  win_rm "$wt/target"
  [ -d "$wt/target" ] && die "no se pudo borrar $wt/target del todo" || echo "OK migrate $a"
}

cmd_gc() {
  local days=7 apply=0
  while [ $# -gt 0 ]; do
    case "$1" in
      --days) days="$2"; shift 2 ;;
      --apply) apply=1; shift ;;
      *) die "opción desconocida: $1" ;;
    esac
  done
  [ -d "$TARGET_ROOT" ] || { echo "no existe $TARGET_ROOT"; return 0; }
  [ "$apply" = 1 ] && cargo_running && die "hay cargo/rustc en marcha: no borro nada"
  echo "Carpetas de compilación en $TARGET_ROOT sin uso desde hace más de $days días:"
  local found=0
  for d in "$TARGET_ROOT"/*/; do
    [ -d "$d" ] || continue
    local fp newest
    fp="$d"
    # el uso reciente se nota en la carpeta .fingerprint de cualquier perfil
    newest="$(find "$d" -maxdepth 3 -name .fingerprint -type d -mtime -"$days" 2>/dev/null | head -1)"
    if [ -z "$newest" ]; then
      found=1
      echo "  SIN USO: $d"
      if [ "$apply" = 1 ]; then win_rm "$d"; echo "  borrada"; fi
    fi
  done
  [ "$found" = 0 ] && echo "  (ninguna)"
  [ "$apply" = 0 ] && echo "Para borrar: añade --apply"
  return 0
}

case "${1:-help}" in
  status)  shift; cmd_status "$@" ;;
  setup)   shift; cmd_setup "$@" ;;
  reset)   shift; cmd_reset "$@" ;;
  migrate) shift; cmd_migrate "$@" ;;
  gc)      shift; cmd_gc "$@" ;;
  *) sed -n '2,16p' "$0" ;;
esac
