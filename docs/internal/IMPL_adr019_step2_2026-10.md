# IMPL — ADR-019 paso 2: permisos por par y por espacio (implementación)

> **Tarea:** TL (turno 1176/1177 → aprobación de José). Implementar el diseño v3 en el worktree, **sin commitear** (commitea el TL).
> **Branch:** `agent/deep-adr019-step2` desde `origin/main` = `682d428` (el spike queda intacto en su rama `475c844`).
> **Autor:** Deep. **Fecha:** 2026-10-15. **Zona:** `E:\tylluan-wt-deep` — NADA en `E:\tylluan`.

## 1. Ficheros tocados (diff sin commitear)

**Núcleo (código nuevo):**
| Fichero | Qué |
|---|---|
| `crates/tylluan-kernel/src/federation/permissions.rs` | **NUEVO.** `PeerPermissions`/`SpacePermission`/`Direction`; `parse_permissions` fail-closed; `resolve_space` (topic_key→metadata.space→"default"); `may_send_space`/`may_receive_space`/`may_dispatch`; `validate_permissions` (rechaza `default` con error claro, wildcards, formato); `dangerous_dispatch_entries` (bash/filesystem/code); `peer_by_x25519`; `audit_permission_denied` (log_audit_entry). + 8 tests. |
| `federation/mod.rs` | Columna `permissions TEXT NOT NULL DEFAULT '{}'` (CREATE + ALTER patrón `ed25519_pubkey`); campo `permissions` en `FederationPeer` (serde default); `PeerDb::update_permissions`; `insert` persiste pero **nunca pisa** permissions en conflicto; `load_all` parsea fail-closed. |
| `memory/silva/nodes.rs` | **Endurecimiento (b):** `NodeWriteOptions.federation_ingest` (+builder); el ON CONFLICT pasa a `weight = nodes.weight` y `protected = nodes.protected` para ingesta de par; resto de callers sin cambio (SQL por rama, no dinámico en datos). |
| `transport/http/api_v1/api_federation.rs` | Helpers únicos `ingest_from_peer` (sig→protected→**gate espacio**→provenance→**frescura con `Err`=rechazo**→upsert endurecido) y `push_body_for_peer`; `decide_freshness` (puro); `IngestOutcome`; endpoint `PUT /peers/{name}/permissions`; **las 4 funciones de ingesta rewireadas al helper** (receive, pull manual, sync_both, pull_internal) y **los 3 flujos de envío** (push manual, sync_both, auto-sync) usan el builder por par; export filtra por espacio+dirección. |
| `transport/http/api_v1/routes.rs` | Ruta del PUT (admin, fuera de `is_public_bypass_path`). |
| `transport/http/mod.rs` | **Gate de dispatch P2P:** el handler recibe `caller_x25519` (de `pipe.peer_id`), resuelve par con `peer_by_x25519` y aplica `may_dispatch` **antes** de `call_tool`; denegación → respuesta `success:false` + warn + **auditoría**; fail-closed si el par no resuelve. |
| `crates/tylluan-link/src/p2p.rs` | `P2pHandlerFn` pasa a `Fn(GuildDispatchRequest, String)` (segundo arg = X25519 hex del llamante); call-site en la misma función de accept. |
| `transport/mdns.rs`, `tests/mesh_audit.rs`, `tylluan-link/tests/p2p_dst.rs` | Adaptación a la nueva firma / nuevo campo (mecánica). |

**Tests:**
| Fichero | Tests |
|---|---|
| `federation/permissions.rs` (mod tests) | 8: fail-closed (vacío/inválido), direcciones, dispatch exacto, resolve_space, validate (default/wildcard/formato), familias peligrosas, `peer_by_x25519`, serde roundtrip → cubren **T7/T9/T10/T11/T12/T13**. |
| `api_v1/api_federation.rs` (mod step2_unit_tests) | 3: **T17** (Err de frescura = rechazo) + ramas Ok(None)/Identical. |
| `memory/silva/tests.rs` | 1: **T16** (protegido sigue protegido; peso existente no cambia; nuevo=0/1.0; control de callers no-ingesta intacto). |
| `tests/federation_audit.rs` | 6 nuevos: **T1** default-deny, **T3+T4** export por espacio/dirección (HTTP e2e con descifrado), **T5+T6+T7** gates de ingesta + **T14** flujo "both" (mismo helper), **T8** push por par, **T15** denegación → fila hash-chained en `guild_audit_log` (seam `TYLLUAN_AUDIT_DB`), **T2** migración legacy (DB pre-columna → deny-all). + 2 tests preexistentes adaptados (asumían receive/export sin permisos; ahora con espacio etiquetado + grant — su intención original intacta). |

**Docs (solo cifras de tests, mecánico):** `README.md` + `AGENTS.md` (vía `scripts/check_test_count.sh --fix`), `STATUS.md` + `CLAUDE.md` (a mano) → 1093 → **1105** (1019+74+12). Si el TL prefiere moverlo a su commit de docs, son 6 líneas exactas.

## 2. Validación ejecutada (cifras reales)

| Comando | Resultado |
|---|---|
| `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` | ✅ limpio (0 warnings/errores) |
| `cargo clippy -p tylluan-link --all-targets -- -D warnings` | ✅ limpio |
| `cargo test -p tylluan-kernel --lib` | ✅ **1019 passed; 0 failed; 3 ignored** (27.79 s) — base 1007 + 12 nuevos = 1019 |
| `cargo test -p tylluan-kernel --test federation_audit` | ✅ **12/12** |
| `cargo test -p tylluan-kernel --test mesh_audit` | ✅ **11/11** |
| `cargo test -p tylluan-link --all-targets` | ✅ 74 lib + bins (7/5/6/4/3/3) todo verde |
| `scripts/check_test_count.sh` | ✅ 1105 (tras `--fix` + STATUS/CLAUDE) |
| `python scripts/check_doc_links.py` | ✅ 161 enlaces relativos OK |
| `scripts/check_docs_reality.sh` | 0 errores duros; exit 1 por warnings low-confidence de ADRs **idéntico en main** (pre-existente, no regresión) |
| `scripts/check_no_predation.sh` | exit 0 — "no HEAD commit to evaluate" (la entrega va sin commitear; el TL lo corre en su commit) |

**No arranqué el kernel de producción ni toqué `data/` de producción. No leí ni imprimí `.tylluan-token`.** Los tests de auditoría usan el seam `TYLLUAN_AUDIT_DB` a un temporal (instalado en `test_state()`), sin contaminar `./data/`.

## 3. Desviaciones del diseño v3 (conscientes, con razón)

1. **Filtro de envío (export/push) sin auditoría por nodo excluido** — el v3 decía "+audit por nodo excluido". Implementado como **política silenciosa** (`trace` con contador): auditar ahí encadenaría una fila por nodo no compartido en CADA ciclo de auto-sync (30 s) — flood de `guild_audit_log`. La auditoría queda para **denegaciones intentadas por el par** (receive con espacio no permitido, dispatch rechazado) — que es donde hay un actor haciéndolo mal. Si el TL quiere filas de envío-filtrado, es un cambio localizado.
2. **Ubicación de helpers:** predicados puros en `federation/permissions.rs`; `ingest_from_peer`/`push_body_for_peer`/`decide_freshness`/`IngestOutcome` en `api_federation.rs` (acoplados a `HttpState`) y `pub` (los usan los tests de integración; también son la API natural para las rutas).
3. **`default`:** rechazado en el PUT (decisión TL) — pero el **gate no bloquea** un `"default"` introducido por edición directa de la DB (la validación es de API). Defensa en profundidad futura si se quiere cerrar también a nivel de consumo.
4. **Auditoría de permisos peligrosos** (escritura del PUT): usa `audit_permission_denied` → fila con `success=false` (marca "evento que requiere atención"); semántica válida pero no es una "denegación". Alternativa trivial si se prefiere otra etiqueta.
5. **Cifras de tests**: cambio mecánico obligado por los +12 tests (gate). Reversible en 6 líneas si el TL lo quiere en su pase de docs.

## 4. ## Impact (propuesto para el mensaje de commit del TL — toca `transport/`)

> **Nota (2026-10-15, tras revisión del TL):** la mención original a `[p2p] enabled=false` en el toml de producción queda **eliminada** — ese flag lo cambió un agente y José no lo ha confirmado. El dato verificado que se mantiene es: **0 pares aprobados** (`GET /api/v1/federation/peers` → `[]`, verificado en vivo el 2026-10-15).

```
## Impact
Cambio de comportamiento SOLO en federación P2P (opt-in; 0 pares aprobados hoy, verificado en vivo). Añade permisos deny-by-default por par/espacio: export/receive/pull/push filtrados por espacio+dirección, dispatch P2P con allow-list exacta guild.tool (denegado por defecto, auditado), ingesta endurecida (un par nunca cambia protected/weight de un nodo existente) y frescura con error = rechazo. Rutas locales (tylluan_do/recall/remember/graph) y el Coherence Gate NO se tocan. Sin cambios en tylluan.toml ni timeouts. 12 tests nuevos (T1-T17 parciales); kernel 1019+74+12=1105.
```

## 5. NO VERIFICADO

- **E2E P2P real entre dos kernels** (gate de dispatch probado con unit tests + compile de mesh_audit; no levanté kernels).
- **Auto-sync en vivo con pares reales** (los caminos unitarios están cubiertos; el bucle real no se ejecutó con red).
- **Revocación en vuelo** (paso 4 del ADR — fuera de alcance; el gate sí es fail-closed si un par aprobado no resuelve).
- `default` vía edición directa de DB (ver desviación 3).
- Comportamiento de `sync_both` como llamada HTTP saliente completa (push+pull contra un peer remoto) — el helper compartido está probado; el POST saliente no (mismo patrón que el código previo).
