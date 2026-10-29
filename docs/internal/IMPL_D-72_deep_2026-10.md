# IMPL D-72 — 401 con código estable para el panel

**Agente:** Deep
**Base:** `origin/main` = `8060a5b`.
**Tipo:** diff sin commitear.

## Cambio

En `crates/tylluan-kernel/src/transport/http/auth.rs`, el 401 del middleware bearer añade un campo aditivo `code`:

- sin cabecera `Authorization` y sin `token=`/`Authorization=` en query → `"missing_token"`;
- con una credencial presente pero inválida → `"invalid_token"`.

El estado HTTP sigue siendo **401** y el cuerpo conserva `error: "unauthorized"` y el `message` existente. No cambia la autenticación ni la clase operador.

Contrato para Antigravity/AG-14:

```json
{
  "error": "unauthorized",
  "code": "missing_token",
  "message": "Valid Bearer token required. Check your .tylluan-token file."
}
```

## Test

`crates/tylluan-kernel/tests/oauth_f2_scope_challenges.rs`:

- `unauthorized_without_credentials_has_missing_token_code`: petición sin token a ruta protegida → 401 y `code = missing_token`.

Resultados:

```text
cargo test -p tylluan-kernel --test oauth_f2_scope_challenges -j 2 -> 5 passed
cargo test -p tylluan-kernel --test security_audit -j 2            -> 40 passed
cargo clippy -p tylluan-kernel --all-targets -j 2 -- -D warnings   -> exit 0
```

## Impact

- **Dashboard:** puede distinguir “falta token” de “token inválido” sin inferirlo del texto.
- **Compatibilidad:** campo JSON aditivo; los clientes que ignoran campos extra no cambian.
- **No depredación:** solo middleware y su test.

## No verificado

- No probé el panel AG-14 en vivo; el contrato se verificó en el router real de tests, no en el dashboard.
- No añadí códigos distintos para token revocado/expirado; ambos caen en `invalid_token`.
