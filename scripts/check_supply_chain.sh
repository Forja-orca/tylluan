#!/usr/bin/env bash
# T-DSH-56 — Comprobación de cadena de suministro (SOLO INFORME: siempre exit 0).
#
# Imprime una tabla OK/FALLA de:
#   1. cargo deny check advisories  (Rust; exige "advisories ok" y NINGÚN
#      warning advisory-not-detected, que delata ignores obsoletos en deny.toml)
#   2. npm audit --omit=dev en dashboard/  (dependencias de producción del dashboard)
#   3. certifi >= 2026.10.19 y pip >= 26.2.2 en .venv
#   4. lo mismo en el runtime vendorizado de guilds-python de ESTA plataforma
#      (windows-x86_64 en Windows, linux-x86_64 en Linux)
#
# Uso: bash scripts/check_supply_chain.sh
# Test/uso en otro checkout sin escribir nada: TYLLUAN_REPO_ROOT=/ruta bash scripts/check_supply_chain.sh
#
# NO bloquea CI (exit 0 siempre) y no modifica nada: solo lee lockfiles,
# ejecuta las herramientas de auditoría y consulta versiones de Python.

set -uo pipefail

ROOT="${TYLLUAN_REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
cd "$ROOT" || { echo "check_supply_chain: no puedo entrar en $ROOT"; exit 0; }

FAILS=0
printf '\n%-40s | %-5s | %s\n' "COMPROBACION" "ESTADO" "DETALLE"
printf '%s\n' "------------------------------------------------------------------------------"

row() { # $1 comprobacion, $2 OK|FALLA, $3 detalle
    printf '%-40s | %-5s | %s\n' "$1" "$2" "$3"
    [ "$2" = "FALLA" ] && FAILS=$((FAILS + 1))
}

version_ge() { # $1 >= $2 (sort -V; portable en GNU coreutils/Git Bash)
    [ "$(printf '%s\n%s\n' "$2" "$1" | sort -V | head -n1)" = "$2" ]
}

# ─── 1. cargo deny (Rust) ────────────────────────────────────────────────────
if command -v cargo-deny >/dev/null 2>&1; then
    deny_out="$(cargo deny check advisories 2>&1)"
    deny_rc=$?
    if [ "$deny_rc" -eq 0 ] && printf '%s' "$deny_out" | grep -q 'advisories ok'; then
        if printf '%s' "$deny_out" | grep -q 'advisory-not-detected'; then
            row "cargo deny advisories" "FALLA" "advisories ok pero con advisory-not-detected (ignore obsoleto en deny.toml)"
        else
            row "cargo deny advisories" "OK" "advisories ok, sin ignores obsoletos"
        fi
    else
        row "cargo deny advisories" "FALLA" "cargo deny devolvio $deny_rc (ver salida)"
    fi
else
    row "cargo deny advisories" "FALLA" "cargo-deny no esta instalado"
fi

# ─── 2. npm audit (dashboard, solo dependencias de produccion) ───────────────
if [ -f dashboard/package-lock.json ] && command -v npm >/dev/null 2>&1; then
    audit_json="$(cd dashboard && npm audit --omit=dev --json 2>/dev/null)"
    audit_rc=$?
    audit_total="$(printf '%s' "$audit_json" | node -e 'let s="";process.stdin.on("data",d=>s+=d);process.stdin.on("end",()=>{try{const j=JSON.parse(s);process.stdout.write(String(j.metadata?.vulnerabilities?.total ?? ""))}catch(e){process.stdout.write("")}})' 2>/dev/null)"
    if [ "$audit_rc" -eq 0 ] && [ "${audit_total:-x}" = "0" ]; then
        row "npm audit (dashboard, prod)" "OK" "0 vulnerabilidades"
    else
        row "npm audit (dashboard, prod)" "FALLA" "exit=$audit_rc total=${audit_total:-?}"
    fi
else
    row "npm audit (dashboard, prod)" "FALLA" "falta dashboard/package-lock.json o npm"
fi

# ─── 3/4. certifi y pip en los entornos Python ──────────────────────────────
check_python_env() { # $1 etiqueta, $2 interprete
    local label="$1" py="$2" vers cver pver
    if [ ! -e "$py" ]; then
        row "$label" "FALLA" "no existe el interprete ($py)"
        return
    fi
    vers="$("$py" -c 'import certifi, pip; print(certifi.__version__, pip.__version__)' 2>/dev/null)"
    if [ -z "$vers" ]; then
        row "$label" "FALLA" "no se pudo importar certifi/pip con $py"
        return
    fi
    cver="${vers%% *}"
    pver="${vers##* }"
    if version_ge "$cver" "2026.10.19" && version_ge "$pver" "26.2.2"; then
        row "$label" "OK" "certifi $cver, pip $pver"
    else
        row "$label" "FALLA" "certifi $cver (min 2026.10.19), pip $pver (min 26.2.2)"
    fi
}

if [ -x .venv/Scripts/python.exe ]; then
    check_python_env ".venv (certifi/pip)" ".venv/Scripts/python.exe"
elif [ -x .venv/bin/python ]; then
    check_python_env ".venv (certifi/pip)" ".venv/bin/python"
else
    row ".venv (certifi/pip)" "FALLA" "no existe .venv en $ROOT"
fi

case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*) gdir="guilds-python/windows-x86_64" ;;
    Linux*)               gdir="guilds-python/linux-x86_64" ;;
    *)                    gdir="" ;;
esac
if [ -n "$gdir" ] && [ -d "$gdir" ]; then
    gpy="$(find "$gdir" -maxdepth 4 \( -path '*/python.exe' -o -path '*/bin/python3' \) \( -type f -o -type l \) 2>/dev/null | head -n1)"
    if [ -n "$gpy" ]; then
        check_python_env "guilds-python ($(basename "$gdir"))" "$gpy"
    else
        row "guilds-python ($(basename "$gdir"))" "FALLA" "no encuentro el interprete portatil"
    fi
else
    row "guilds-python (plataforma actual)" "FALLA" "no existe $gdir"
fi

# ─── Resumen ────────────────────────────────────────────────────────────────
printf '%s\n' "------------------------------------------------------------------------------"
if [ "$FAILS" -eq 0 ]; then
    echo "RESUMEN: todo OK"
else
    echo "RESUMEN: $FAILS comprobacion(es) en FALLA (solo informe: este script no bloquea)"
fi
exit 0
