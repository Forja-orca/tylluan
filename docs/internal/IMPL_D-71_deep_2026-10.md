# IMPL D-71 — Migración a `default_role = guest`

**Agente:** Deep
**Base:** `origin/main` = `8060a5b` (push202; `git rev-parse --short HEAD` confirmado).
**Tipo:** diff sin commitear. Solo el TL comitea y empuja.

## Cambios

- `SECURITY.md`: sección **OAuth clients after the fail-closed ACL default** con los tres pasos para un cliente OAuth de solo lectura:
  1. `[security.acl.roles] reader = ["knowledge", "coloquio", "monitor", "websearch"]`;
  2. el cliente envía `X-Agent-Id: oauth-reader`;
  3. `.tylluan/agents.toml` declara `[agents.oauth-reader] role = "reader"`.
- `tylluan.example.toml`: el mismo ejemplo mínimo, comentado, junto a `[security.acl.tokens]`.
- Sin cambios de comportamiento fuera de D-68: el rol por defecto sigue siendo `guest`; el mapeo es explícito.

La identidad OAuth no se resuelve por el `sub` del JWT (hoy `mcp`): se resuelve por el `agent_id` que el cliente declara y el contrato local `.tylluan/agents.toml`, o por un bearer estático en `[security.acl.tokens]`. Sin una de esas dos vías el JWT queda en `guest` y recibe 403 en guilds.

## Tests

En `crates/tylluan-kernel/src/transport/http/auth.rs`, módulo `d71_oauth_reader_tests`:

- `mapped_oauth_agent_gets_reader_and_unmapped_stays_guest`: con `roles.reader` + contrato `oauth-reader` el rol resuelto es `reader`, accede a `knowledge` y no a `bash`; sin `agent_id`/contrato el rol es `guest` y no accede a `knowledge`.

Resultados:

```text
cargo test -p tylluan-kernel --lib d71_oauth_reader_tests -j 2 -> 1 passed
cargo test -p tylluan-kernel --test security_audit -j 2        -> 40 passed
cargo clippy -p tylluan-kernel --all-targets -j 2 -- -D warnings -> exit 0
```

## Impact

- **Instalaciones nuevas:** el operador tiene un camino documentado para dar lectura a un cliente OAuth sin volver a `admin`; sin mapeo se mantiene fail-closed.
- **Compatibilidad:** no cambia código de resolución ni defaults; solo documentación y test de la ruta existente.
- **No depredación:** `SECURITY.md`, `tylluan.example.toml` y `auth.rs`; no se tocó `tylluan.toml` de producción.

## No verificado

- No ejecuté un flujo OAuth completo con un JWT real y `X-Agent-Id`; el test cubre la resolución pura que usa el middleware.
- No verifiqué el mapeo por `[security.acl.tokens]` en vivo; queda como alternativa documentada.
