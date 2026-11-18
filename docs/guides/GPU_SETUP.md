# GPU para inferencia ONNX — Setup de producción (M25-A)

El kernel selecciona el execution provider con `[inference] device` en `tylluan.toml`.
Fallback siempre seguro: si el provider falla al registrarse, cae a CPU con un WARN en el log.

## Opción 1 — DirectML (recomendada en Windows: cualquier GPU, cero instalación)

```toml
# tylluan.toml
[inference]
device = "directml"
```
```bat
cargo build -p tylluan-kernel
.\tylluan-mcp.bat
```
Requisitos: Windows 10 1903+ (DirectML.dll viene con el sistema). Funciona con NVIDIA, AMD e Intel.
Verificación: el log de arranque debe decir `🚀 Inference device: DirectML (GPU accelerated)`.

## Opción 2 — CUDA (NVIDIA, máximo rendimiento)

**Build** (la feature compila los bindings del provider):
```bat
cargo build -p tylluan-kernel --features cuda
```

**Runtime** — tres piezas de NVIDIA/Microsoft que ningún crate puede empaquetar (licencias):
1. **onnxruntime con CUDA**: la página de releases de Microsoft devuelve **404** desde el
   2026-11-18 (repositorio, releases y API caen a la vez). La vía que funciona es PyPI:
   ```bat
   pip download onnxruntime-gpu==1.22.0 --no-deps -d ort
   copy ort\onnxruntime_gpu-1.22.0-*.whl ort\ort.zip
   powershell -Command "Expand-Archive -Force ort\ort.zip -DestinationPath ort\wheel"
   ```
   La DLL queda en `onnxruntime\capi\onnxruntime.dll` dentro de la rueda (comprobado en la
   rueda de Windows `onnxruntime-1.22.0-cp312-cp312-win_amd64.whl`: las dos DLL que trae son
   `onnxruntime/capi/onnxruntime.dll` y `onnxruntime/capi/onnxruntime_providers_shared.dll`).
   Apunta el kernel a ella:
   ```bat
   set ORT_DYLIB_PATH=C:\ruta\a\ort\wheel\onnxruntime\capi\onnxruntime.dll
   ```
   Sin `ORT_DYLIB_PATH`, el kernel carga la DLL del sistema, que es **CPU-only**. La versión
   tiene que ser **1.22.0**, la misma que fijan CI y la imagen: el feature `load-dynamic` carga
   el ORT API de esa DLL concreta y mezclar versiones puede fallar al cargar.
2. **CUDA Runtime 12.x** — instalador de NVIDIA o las DLLs (`cudart64_12.dll` etc.) en PATH.
3. **cuDNN 9** — DLLs en PATH.

```toml
[inference]
device = "cuda"
```

Verificación: log `🚀 Inference device: CUDA (GPU accelerated)`. Si falta cualquier DLL,
verás un WARN de registro del provider y el kernel sigue en CPU — nunca crashea por esto.

## Qué esperar

| Operación | CPU (baseline medido) | GPU esperado |
|---|---|---|
| Embedding BGE-M3 | 2-8 s | 50-200 ms |
| Rerank Jina (50 pares) | ~60 ms | ~5-10 ms |
| tylluan_recall end-to-end | ~5 s | sub-segundo |

## Matriz de soporte

| Plataforma | cpu | directml | cuda |
|---|---|---|---|
| Windows x64 | ✓ | ✓ (sin feature) | ✓ (`--features cuda` + DLLs) |
| Linux x64 | ✓ | — | ✓ (`--features cuda` + libs .so) |
