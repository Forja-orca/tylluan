# IMPL — E1: el token maestro por query-string no satisface la clase operador

- **Estado: IMPLEMENTADO — diff sin commitear.** Base: `origin/main` `f6c5187`.
- **Autor:** Deep. **Fecha:** 2026-10-16. **Encargo:** Coloquio T1237 (decisión de José: cerrar E1).

## Qué cambia

- **`src/transport/http/auth.rs`**:
  - Nueva variante `AclAuthKind::MasterTokenQuery` — el MEDIO de autenticación se propaga sin colapsarse: cabecera `Authorization: Bearer` → `MasterToken`; `?token=` → `MasterTokenQuery`; JWT OAuth → `OAuthJwt`; dev_mode → `DevBypass` (intactos).
  - `operator_gate_allows` mantiene la lista (`MasterToken | DevBypass`): la variante de query queda excluida **por construcción** (fail-closed), sin tocar la lógica del gate. Doc actualizada.
  - Mensaje del 403 actualizado: "…requires the master operator token in the Authorization header. OAuth tokens, query-string tokens and peer bypasses are not accepted."
- **Decisión 401 vs 403** (pedida por el TL): **403** con el sobre habitual `{"error":"operator_required",…}`. Razón: el llamante está AUTENTICADO con una credencial válida; lo que no está autorizado es el MEDIO en esas rutas — mismo trato que un JWT OAuth (R14). El 401 queda reservado a credencial ausente o inválida (middleware bearer).
- Sin cambios en `dev_mode`, JWT, `/sse?token=` ni sandbox del dashboard (rutas no-operador siguen autenticando por query; verificado por el TL que el dashboard no usa `?token=` en ninguna ruta de operador).

## TDD (rojo → verde)

- **Rojo**: `e1_query_token_does_not_satisfy_operator_class` falla contra `f6c5187` (el gate aceptaba el query-token; los guards de dev_mode/SSE ya pasaban).
- **Verde**: `tests/oauth_operator_gate.rs` **6/6**:
  - `e1_query_token_does_not_satisfy_operator_class` — (a) `PUT /api/v1/federation/peers/…/permissions?token=` → **403**; (b) `/api/v1/bash?token=` → **403** (ejemplo del TL); (c) el mismo token por **cabecera** → 404 (el gate sigue pasando).
  - `e1_sse_query_token_still_authenticates` — `/sse` sin token → 401; `/sse?token=` → ni 401 ni 403 (SSE del dashboard sigue autenticando).
  - `e1_dev_mode_operator_gate_unchanged` — dev_mode=true sin token → 404 (DevBypass igual que antes).
  - R12/R13/R14/R15 (OAuth Fase 1) siguen verdes.

## Evidencia

- `cargo test -p tylluan-kernel --test oauth_operator_gate` → 6/6.
- Lib: **1048 + 3 ignored** (sin tests lib nuevos → conteo estable); `check_test_count.sh` ✅ 1134.
- `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` limpio.

## Ficheros

`crates/tylluan-kernel/src/transport/http/auth.rs` · `crates/tylluan-kernel/tests/oauth_operator_gate.rs` · este informe.

## Impact (ADR-013 — para el mensaje de commit)

- **Cierra E1**: el token maestro por query-string deja de satisfacer las rutas de operador (403 `operator_required`), cerrando el bypass de `/api/v1/bash?token=…` y el resto de la clase operador.
- Sin cambio de comportamiento fuera de esas rutas: query-string sigue autenticando `/sse` y sandbox (verificado por test), dev_mode y JWT intactos, sobre de error existente reutilizado.
- Sin cambios de contrato de respuesta ni de esquema; fail-closed por construcción (variante excluida de la lista del gate).
