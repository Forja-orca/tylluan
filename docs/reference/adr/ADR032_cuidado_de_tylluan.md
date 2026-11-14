# ADR-032: Cuidado de Tylluan — doctor real, copia verificada y actualizar sin perder memoria

**Status:** Borrador para revisión del TL (clase A, **sin código**). Aprobación: José.
**Autor:** Deep. **Pareja de revisión:** Hermes. **Fecha:** 2026-11-14.
**Requisito (TL T4086, decisión de José 14-nov):** doctor real + copia verificada + actualizar sin perder memoria, en
Linux y Windows. **La copia incluye la identidad de los agentes (`identity.key`, `oauth_jwt_secret`), cifrada por
defecto.** Regla que atraviesa todo el ADR: **nunca cambiar nada sin una copia verificada**.

## 1. Doctor: qué hay hoy y qué debe haber

**Hoy (medido):**
- `repair` solo acepta dos objetivos: `guild` (reinicia una guild) y `storage` (**VACUUM solo de Silva**)
  (`crates/tylluan-kernel/src/doctor/mod.rs:308-335`).
- `check_storage` solo mira Silva (`health_check`, `node_count`, `stats`): las otras bases **no se comprueban**
  (`doctor/mod.rs:215-238`).
- El «benchmark» es un bucle de `String::push('x')` 10.000 veces y un `HashMap` 10.000 inserts: no toca ninguna ruta
  real del producto (`doctor/mod.rs:336-350`).
- **No hay ninguna comprobación de FTS** (`git grep -n "fts" origin/main -- crates/tylluan-kernel/src/doctor/mod.rs`
  → 0), aunque existe una reparación FTS explícita, con copia previa, verificación y rollback
  (`crates/tylluan-cli/src/repair_fts.rs:1-14`).

**Qué debe diagnosticar** (todo con comando y salida, no semáforos):

| Área | Qué se comprueba | Hoy |
|---|---|---|
| Integridad de bases | `PRAGMA integrity_check` + `quick_check` en las **15** bases del inventario canónico (`crates/tylluan-cli/src/backup.rs:40-58`) | solo Silva |
| FTS | `nodes` vs `nodes_fts_docsize`: huérfanos y sin indexar (`FtsCounts`, `repair_fts.rs:24-30`) | nada |
| Guilds / venv | que el intérprete sea el del venv y **no** el del sistema (`venv_final_path`), y que el árbol de guilds que carga el kernel sea el que el instalador deja (`guilds_root`) | nada |
| Modelos | que existan los modelos que pide el catálogo (`maintenance.rs`), sin URLs de chat hardcodeadas | nada |
| Puertos | 47004 libre/ocupado por **este** kernel, y si el PID del `.pid` coincide | parcial (`/health`) |
| Config desviada | `tylluan.toml` contra los defaults peligrosos: `vector_dimensions = 1024`, `host` + `dev_mode` juntos, `[p2p] enabled` | nada |

**Qué repara solo:** **nada que toque datos**. Regla heredada de `repair_fts` (T-DSH-171): copia → verificar copia →
reparar → verificar reparación → **rollback si la verificación falla**; y siempre por orden explícita del operador,
nunca por arranque automático (`repair_fts.rs:11-14`).

**Qué NUNCA repara solo** (lista cerrada): borrar o rotar `identity.key` / `oauth_jwt_secret`; tocar `peers` o permisos
de federación; reescribir o deduplicar nodos de memoria y embeddings; migrar esquema; cambiar `vector_dimensions`;
reiniciar producción; «arreglar» un FTS sin copia.

## 2. Copia y restauración verificada

**Hoy:** `tylluan-cli backup` copia en caliente con `VACUUM INTO` (consistente también con WAL), cubre **15 bases** con
manifiesto (`backup.rs:1-20`, `:40-58`, `:62-75`), y `--with-secrets` añade `identity.key` y `oauth_jwt_secret`
**copiados tal cual, en claro y solo si el operador lo pide** (`backup.rs:14-16`, `:59`, `:373`). `restore` exige que
el kernel de ese `DATA_DIR` esté parado, acepta `--target-dir` y antes de tocar nada guarda
`pre_restore_<timestamp>/` (`backup.rs:17-20`).

**Falta para cumplir el requisito:**
1. **Cifrado por defecto de los secretos** (decisión de José; hoy es claro y opt-in). Modelo propuesto, sin generarlo
   aquí: `secrets.enc` con **XChaCha20-Poly1305** y clave derivada por **Argon2id** de una contraseña que teclea el
   operador al crear la copia; salt y nonce en la cabecera del fichero; la contraseña **no** se guarda junto a la copia.
   La clave de la copia y la de SQLCipher son **independientes**: perder la contraseña = copia sin secretos
   recuperables (se dice en el manifiesto, no se descubre al restaurar).
2. **Restauración verificada de verdad**: restaurar en carpeta temporal (`--target-dir` ya existe) y **comparar cuentas**:
   nodos de Silva, edges, `nodes`/`nodes_fts_docsize` (huérfanos, sin indexar), filas por tabla de cada base y
   `sha256` por entrada del manifiesto. Si una sola cuenta no cuadra, la restauración es **FALLIDA** — no «restaurada».
3. **Checksum por fichero en el manifiesto** (hoy guarda `size`, `backup.rs:68-75`): sin hash no se puede afirmar que
   la copia es la que se hizo.

## 3. Actualizar sin perder memoria

**Hoy:** `tylluan update --install` descarga el archivo, exige `checksums.txt` del release y verifica SHA-256 antes de
extraer (`crates/tylluan-cli/src/main.rs:909-999`, `verify_sha256` en `:1721`); instala desde un directorio de staging
(`:1002-1017`) y termina diciendo «Restart the kernel» (`:1019-1021`). En **Windows** aparta el binario anterior a
`<nombre>.old-<pid>` (`main.rs:1841-1856`); en **Unix** hace `rename` sobre el destino y el anterior **se pierde**
(`main.rs:1829-1837`). **No** toma copia antes, **no** comprueba salud después, **no** compara memoria y **no** tiene
marcha atrás.

**Orden obligatorio propuesto:**
1. **Copia verificada** (§2, con secretos cifrados) y **restauración de prueba en temporal**. Si falla cualquiera de
   las dos → ABORTA la actualización.
2. `update --check` y descarga con checksum (ya existe).
3. Cambio de binario **conservando el anterior** también en Unix (`.old` o `binario.prev`), y registrar versión anterior.
4. Arranque + `/health` conforme al contrato (`schemas/health_response.json`): `status = ok` y `version` nueva.
5. **Cuentas de memoria antes/después**: nodos + edges + FTS; si difieren → **marcha atrás automática**: parar, reponer
   el binario anterior y restaurar la copia verificada.
6. Rastro en `data/update.log`: versión origen/destino, checksums, cuentas antes/después y resultado.

## 4. Pantalla de Ajustes (3 botones, español sencillo)

- **«Comprobar mi Tylluan»** → doctor en solo lectura: qué está mal, qué se puede arreglar y qué **no** se toca solo.
- **«Hacer copia segura»** → pide contraseña si va con secretos, dice dónde quedó y **cuándo se verificó**.
- **«Actualizar»** → versión actual → nueva, exige copia verificada y ofrece «volver atrás».

Ningún botón dice «OK» sin fecha y sin cuentas. Maquetación: DSH, tras la auditoría T4082.

## 5. Preguntas para José (son suyas)

1. **Custodia de la contraseña de la copia**: memorizada, gestor de contraseñas, o segundo soporte físico.
2. ¿Se cifran **también las 15 bases** (hoy SQLCipher es opt-in con `--features encryption`) o solo los secretos?
3. Frecuencia y retención de copias (¿diaria? ¿cuántas se guardan?).
4. ¿Actualización solo manual con estos 3 botones, o automática con copia previa?

## No verificado

- **No he ejecutado** `backup`/`restore` en esta sesión ni en Linux: todo lo anterior sobre su comportamiento sale de
  leer el código citado, no de una prueba mía.
- `--with-secrets` en Windows con ACLs: **sin probar**.
- No existe hoy inventario de filas por tabla, así que las «cuentas» de §2 y §3 hay que definirlas al implementar.
- `repair_fts` tiene tests de su autor; **no** lo he probado en vivo.
