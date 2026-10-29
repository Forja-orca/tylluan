# IMPL D-68 — ACL fail-closed y token maestro operador

**Agente:** Deep
**Base:** `origin/main` = `4f0cb3a` (ff-only; `REVIEW.md` leído).
**Tipo:** diff sin commitear. Solo el TL comitea y empuja.

## Cambios

1. `AclConfig::default()` pasa de `default_role = "admin"` a `"guest"`, sin roles por defecto (`crates/tylluan-kernel/src/config.rs`, `default_acl_default_role`).
2. El token maestro deja de heredar `default_role`: `MasterToken`, `MasterTokenQuery` y `DevBypass` reciben rol ACL `admin` explícito mediante `acl_role_for_auth_kind` (`transport/http/auth.rs`). El token por query sigue sin satisfacer la clase operador (E1).
3. El gate de guild se evalúa siempre que haya un rol HTTP resuelto, no solo si `[security.acl.roles]` está poblado:
   - `transport/http/api_v1/api_guilds.rs`
   - `transport/server/handler_do/prepare.rs`
   - `transport/server/handler_do/a2a.rs`
4. `tylluan.example.toml`: `default_role = "guest"` y `roles.guest = []`; comentario de fail-closed.
5. `SECURITY.md`: la descripción de la limitación ACL refleja el nuevo default.

La ausencia de rol task-local se trata como invocación interna/directa (tests, stdio), no como un rol HTTP desconocido; el middleware HTTP siempre instala un rol para las rutas protegidas.

## Tests

- `d68_acl_tests::default_acl_role_is_guest_and_denies_every_guild`: default guest, roles vacíos, guest denegado en todos los guilds, admin permitido.
- `d68_acl_tests::master_credential_is_explicit_operator_role`: master header/query y dev bypass → admin; OAuth JWT y guild token sin override.
- `d68_acl_tests::unlisted_oauth_identity_gets_guest_not_admin`: identidad JWT sin mapeo queda en guest y no accede a guild.
- `security_audit`: 40/40.
- `oauth_f2_adversarial_spec`: 8/8.
- `cola_d34_acceptance`: 5/5 (maestro sigue pasando, guild token sigue 403 en rutas operador).
- `cargo test -p tylluan-kernel --lib -j 2`: **1425 passed**, 0 failed, 4 ignored.
- `cargo clippy -p tylluan-kernel --all-targets -j 2 -- -D warnings`: exit 0.

## Impact

- **Seguridad:** un token no mapeado ya no cae en admin; el maestro conserva acceso operativo por clase explícita. Sin `roles`, el gate deniega en HTTP en vez de quedar desactivado.
- **Compatibilidad:** llamadas internas/directas sin contexto siguen funcionando; clientes HTTP deben mapear tokens o usar el maestro. La configuración de producción (`tylluan.toml`) no se tocó.
- **No depredación:** solo kernel, ejemplo y documentación de seguridad; no se tocó el checkout compartido ni el kernel vivo.

## No verificado

- No arranqué un kernel real con `dev_mode = false` para repetir la matriz HTTP completa de Buffy; los tests unitarios/integración cubren la resolución y los gates, no una sesión OAuth completa contra producción.
- No medí el impacto de guest en un despliegue con muchos tokens no mapeados; el cambio es intencionadamente fail-closed.
