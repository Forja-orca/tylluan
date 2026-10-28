# T-DSH-117 — Persistencia atómica del punto de inferencia en `tylluan.toml` (paso 4 de T-DSH-116)

- **Agente:** dsh
- **Fecha:** 2026-10-28
- **Base:** `origin/main = 6ba80c6` (push179: T-DSH-116 pasos 1–3 integrados). Diff sin commitear.
- **Encargo (T1966):** paso 4 de 116 = persistir el cambio de punto en `tylluan.toml` (escritura atómica, solo la clave `[inference.points.<p>]`, sin tocar el resto ni secretos; opt-in con flag `--persist` en REST/CLI; test con TOML temporal) + adaptadores de decisión OpenAI/Anthropic/Ollama **solo si caben sin romper el contrato**. Informe con `## Impact` y "no verificado".

## Persistencia atómica (`config.rs`)

- `render_inference_point_section(raw, point, &point)` — **función pura**: localiza `[inference.points.<p>]` y reemplaza/añade **solo** las claves `provider`, `endpoint`, `model`, `api_key_env`, `cloud_enabled`; el resto del texto (secciones, comentarios, orden, otras claves) se conserva **literalmente**. Si la sección no existe, se añade al final sin tocar nada más.
- `atomic_write(path, contents)` — escribe en un temporal en el **mismo directorio** (`.tylluan.toml.tmp.<pid>`), `sync_all()` y `rename()` sobre el destino; si falla el rename, borra el temporal.
- `persist_inference_point(path, name, &point)` — lee el TOML, renderiza y escribe atómico.
- **Nunca se escriben valores de clave**: solo el nombre de la variable (`api_key_env`); el resto del fichero (incluido cualquier `api_key` ajeno) no se toca.
- Camino: `TylluanConfig::find_config_file()` (respeta `--config`/`CONFIG_PATH_OVERRIDE`, cwd y `dirs::config_dir`).

## REST (opt-in)

- `POST /api/v1/inference/points/{point}` acepta `"persist": true` en el body (sigue siendo **clase operador**).
- Si `persist` no está o es `false`: solo memoria (comportamiento paso 3).
- Si `persist: true`: tras aplicar en memoria, escribe el TOML atómico y devuelve `persisted: {path}`. Si no hay `tylluan.toml` ⇒ **400**; si la escritura falla ⇒ **500** con aviso de que el cambio quedó solo en memoria.
- La respuesta incluye `state` actualizado en ambos casos.

## CLI (opt-in)

- `tylluan inference --port <p> set-point <point> --persist [...]` añade `persist: true` al POST y muestra `persisted`.
- Sin `--persist` sigue siendo solo en memoria.

## Adaptadores de decisión OpenAI/Anthropic/Ollama

**No implementados en este diff** (el encargo los condiciona a "solo si caben sin romper el contrato"). Razón: el punto `decision` usa el wire tipado de decisión (preguntas/opciones/distribución, contrato §1217); mapear Anthropic/Ollama a ese contrato exigiría inventar un esquema de respuesta (JSON de decisión) que hoy no existe, y OpenAI Decisions añade otro wire. Se deja `systemone_http` como único proveedor de decisión (paso 2) y el resto avisa + baseline de reglas. Queda propuesto como tarea aparte con contrato explícito.

## Verificación

```
cargo clippy -p tylluan-kernel --all-targets -- -D warnings → 0
cargo clippy -p tylluan-cli --all-targets -- -D warnings    → 0
cargo test -p tylluan-kernel --lib                          → 1371 passed; 0 failed; 4 ignored (42,27 s)
cargo test -p tylluan-kernel --test contracts_test          → 5 passed
cargo test -p tylluan-cli                                   → 34 passed
python scripts/check_async_guild_io.py                      → exit 0
```

Tests nuevos (4, `config::inference_points_persist_tests`, con TOML temporal):
- `render_updates_only_target_section` — actualiza la sección del punto y conserva cabecera/comentarios, `[security.sentinel]` (incluido un `api_key` ajeno) y `[night]`; la sección aparece una sola vez.
- `render_appends_missing_section_without_touching_rest` — añade la sección si falta y el resultado sigue siendo TOML válido.
- `persist_writes_atomically_and_roundtrips` — fichero temporal real: persiste, reparsea con `toml`, verifica el punto y que el resto (incluido el secreto ajeno) queda intacto; **no deja temporales**.
- `persist_never_writes_key_values_only_env_names` — la sección escrita solo contiene `api_key_env`, ningún `sk-`.

### Verificación en vivo (kernel test-run, puerto/DATA_DIR propios, BM25-only)

TOML del test-run con cabecera comentada, `[security.sentinel]` y `[night]` además de `[inference.points.chat]`:

```
POST /api/v1/inference/points/chat {ollama_compatible, endpoint 127.0.0.1, cloud_enabled true, persist true}
  → 200 {ok:true, persisted:{path:"tylluan.toml"}}
fichero: provider = "ollama_compatible" en [inference.points.chat]
         cabecera comentada, [security.sentinel] y [night] intactos
tylluan inference --port <p> set-point chat --provider local_ephemeral --persist
  → exit 0 {ok:true, persisted:{path:"tylluan.toml"}}
fichero: provider = "local_ephemeral"; cabecera y [security.sentinel] intactos
```

## Impact

- El cambio de punto de inferencia puede **persistir** de forma atómica y quirúrgica: solo su sección `[inference.points.<p>]`, sin reescribir el resto del `tylluan.toml` ni tocar secretos (solo el nombre de la variable de entorno), y siempre con opt-in explícito (`--persist`/`persist:true`) — por defecto sigue siendo solo memoria.
- El operador puede dejar el cambio fijado entre reinicios sin editar el TOML a mano.
- Los adaptadores de decisión adicionales se dejan fuera conscientemente para no romper el contrato de decisión (justificado).

## No verificado

- **Consistencia ante caída durante la escritura**: el patrón temp+`sync_all`+rename es atómico a nivel de fichero, pero no se ha probado un corte de energía/BSOD a mitad (no hay test de crash-consistency).
- **Reinicio real**: no se reinició el kernel tras persistir para comprobar que el nuevo valor se carga al arrancar (el parseo del TOML resultante sí se verificó con `toml::from_str` en test y en vivo se comprobó el texto).
- **Ruta con `--config`**: el test-run usó el `tylluan.toml` del cwd; no se probó con `--config <otra ruta>` (la función usa `find_config_file`, que respeta el override, pero no se ejercitó en vivo).
- **Escritura concurrente** de dos cambios de punto a la vez: no probada (el kernel serializa por el lock de config, pero el fichero podría ser escrito por otro proceso).
- **Adaptadores de decisión OpenAI/Anthropic/Ollama**: no implementados (justificación arriba).
- **Windows rename sobre fichero existente**: funcionó en vivo (el fichero se reemplazó), pero no se probó en otras plataformas.
- **No se tocó** `:47004`, `E:\tylluan` ni el 9000 ajeno.
