# IMPL D-70 — Endurecer la extracción de `tylluan update`

**Agente:** Deep
**Base:** `origin/main` = `4f0cb3a`.
**Tipo:** diff sin commitear. Solo el TL comitea y empuja.

## Cambios

En `crates/tylluan-cli/src/main.rs`:

- `MAX_UPDATE_UNCOMPRESSED_BYTES = 512 * 1024 * 1024` (512 MiB) como cota total de los dos binarios extraídos.
- `extract_update_binaries` rechaza cualquier entrada esperada que no sea un fichero regular (`!entry.header().entry_type().is_file()`): symlink, directorio, hardlink o dispositivo.
- Antes de `unpack`, suma el tamaño declarado y aborta si supera la cota o desborda `u64`.
- El destino sigue siendo `stage/<nombre esperado>`; la ruta interna del tar nunca se usa como destino.

## Tests

En el módulo `d70_tests` del CLI:

- symlink llamado como un binario → error “not a regular file”;
- directorio llamado como un binario → el mismo error;
- entrada regular cuyo tamaño declarado supera 512 MiB → error de límite de descompresión;
- el test existente de extracción regular con ambos binarios sigue pasando.

Resultado: **41/41 tests de `tylluan-cli`**, 0 fallos; `cargo clippy -p tylluan-cli --all-targets -j 2 -- -D warnings` exit 0.

## Impact

- **Seguridad:** el updater ya no puede extraer enlaces o directorios con nombre de binario ni aceptar una expansión sin cota; el checksum externo de D-64 sigue siendo el control de integridad previo.
- **Compatibilidad:** los releases actuales usan ficheros regulares y quedan por debajo de 512 MiB; no cambia el flujo normal.
- **No depredación:** solo CLI y sus tests; no se tocó producción.

## No verificado

- No probé con un `.tar.gz` real de release ni con un binario en ejecución; los tests son tar sintéticos sin red.
- La cota es total para los dos binarios esperados; no es una cota del archivo comprimido ni de miembros no esperados que se ignoran.
