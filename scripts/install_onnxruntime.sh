#!/usr/bin/env bash
# D204 (H-20261118-ORT-404): instala libonnxruntime.so 1.22.0 desde **PyPI**.
#
# Por qué existe: el 2026-11-18 github.com/microsoft/onnxruntime empezó a devolver
# 404 entero (repo, releases y API), y con él se cayó CI, Docker Smoke y el job
# programado: los tres bajaban el .tgz de `releases/download/v1.22.0`. Este script
# no toca github.com/microsoft: baja la **rueda de onnxruntime** de PyPI, comprueba
# el sha256 fijado (leído del JSON del índice) ANTES de extraer nada, y saca de ahí
# el `libonnxruntime.so.1.22.0`.
#
# Los hashes son del índice de PyPI (`https://pypi.org/pypi/onnxruntime/1.22.0/json`),
# no calculados a mano:
#   x86_64  onnxruntime-1.22.0-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl
#   aarch64 onnxruntime-1.22.0-cp312-cp312-manylinux_2_27_aarch64.manylinux_2_28_aarch64.whl
# cp312 en las dos: el `.so` es el mismo con independencia de la etiqueta de Python,
# y así el pin es uno solo por plataforma. Recalcular ambos si sube la versión.
#
# Uso:
#   bash scripts/install_onnxruntime.sh              # a /usr/lib (necesita root)
#   bash scripts/install_onnxruntime.sh --dest DIR   # otro destino
#   bash scripts/install_onnxruntime.sh --dry-run    # descarga, verifica, no instala

set -euo pipefail

VERSION="1.22.0"
DEST="/usr/lib"
DRY_RUN=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --dest) DEST="${2:?--dest necesita una ruta}"; shift ;;
    --dry-run) DRY_RUN=1 ;;
    -h|--help) sed -n '2,24p' "$0"; exit 0 ;;
    *) echo "opción desconocida: $1" >&2; exit 2 ;;
  esac
  shift
done

case "$(uname -m)" in
  x86_64|amd64)
    WHEEL="onnxruntime-1.22.0-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl"
    SHA256="6964a975731afc19dc3418fad8d4e08c48920144ff590149429a5ebe0d15fb3c"
    URL="https://files.pythonhosted.org/packages/8c/60/16d219b8868cc8e8e51a68519873bdb9f5f24af080b62e917a13fff9989b/${WHEEL}"
    ;;
  aarch64|arm64)
    WHEEL="onnxruntime-1.22.0-cp312-cp312-manylinux_2_27_aarch64.manylinux_2_28_aarch64.whl"
    SHA256="c8601128eaef79b636152aea76ae6981b7c9fc81a618f584c15d78d42b310f1c"
    URL="https://files.pythonhosted.org/packages/03/79/36f910cd9fc96b444b0e728bba14607016079786adf032dae61f7c63b4aa/${WHEEL}"
    ;;
  *) echo "arquitectura no soportada: $(uname -m)" >&2; exit 1 ;;
esac

for bin in curl sha256sum unzip; do
  command -v "$bin" >/dev/null 2>&1 || { echo "falta '$bin' (instálalo antes de llamar a este script)" >&2; exit 1; }
done

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "ONNX Runtime ${VERSION} desde PyPI: ${WHEEL}"
curl -fsSL --retry 6 --retry-delay 10 --retry-all-errors -o "${TMP}/ort.whl" "$URL"

# Verificación ANTES de tocar el zip: si el hash no cuadra, no se extrae nada.
echo "${SHA256}  ${TMP}/ort.whl" | sha256sum -c - >/dev/null
echo "sha256 verificado: ${SHA256}"

unzip -qq -j "${TMP}/ort.whl" "onnxruntime/capi/libonnxruntime.so.${VERSION}" -d "${TMP}/lib"
test -s "${TMP}/lib/libonnxruntime.so.${VERSION}" || { echo "la rueda no trae libonnxruntime.so.${VERSION}" >&2; exit 1; }

if [ "$DRY_RUN" -eq 1 ]; then
  echo "dry-run: librería extraída y verificada en ${TMP}/lib (no se instala)"
  ls -l "${TMP}/lib"
  exit 0
fi

mkdir -p "$DEST"
# El enlazador busca `libonnxruntime.so`: se instala el fichero versionado y el enlace.
mv "${TMP}/lib/libonnxruntime.so.${VERSION}" "${DEST}/"
ln -sf "libonnxruntime.so.${VERSION}" "${DEST}/libonnxruntime.so"
if command -v ldconfig >/dev/null 2>&1 && [ "$(id -u)" -eq 0 ]; then
  ldconfig
fi
echo "instalado: ${DEST}/libonnxruntime.so -> libonnxruntime.so.${VERSION}"
