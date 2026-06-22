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
.\tylluan.bat
```
Requisitos: Windows 10 1903+ (DirectML.dll viene con el sistema). Funciona con NVIDIA, AMD e Intel.
Verificación: el log de arranque debe decir `🚀 Inference device: DirectML (GPU accelerated)`.

## Opción 2 — CUDA (NVIDIA, máximo rendimiento)

**Build** (la feature compila los bindings del provider):
```bat
cargo build -p tylluan-kernel --features cuda
```

**Runtime** — tres piezas de NVIDIA/Microsoft que ningún crate puede empaquetar (licencias):
1. **onnxruntime con CUDA**: descargar `onnxruntime-win-x64-gpu-<ver>.zip` de
   https://github.com/microsoft/onnxruntime/releases (versión 1.20+), extraer y apuntar:
   ```bat
   set ORT_DYLIB_PATH=C:\ruta\a\onnxruntime-win-x64-gpu\lib\onnxruntime.dll
   ```
   (el kernel usa `load-dynamic`: carga esa DLL en runtime; sin la variable usa la del sistema, que es CPU-only)
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
| ARM (aarch64) | ✓ (NEON) | — | — |
