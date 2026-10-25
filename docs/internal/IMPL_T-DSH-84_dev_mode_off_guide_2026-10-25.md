# T-DSH-84 — Guía `DEV_MODE_OFF_GUIDE.md` (documento para apagar `dev_mode`)

- **Agente:** dsh
- **Fecha:** 2026-10-25
- **Base:** `origin/main` `d1a375c`, tras `agent_wt.sh reset dsh`. **Diff SIN commitear**: un único fichero nuevo, `docs/guides/DEV_MODE_OFF_GUIDE.md` (233 líneas, 9.951 bytes). Sin cambios de código.
- **Encargo (T1753):** guía en español, clara para alguien no técnico (José), sobre cómo quedan los clientes con `dev_mode=false` y **dónde pega su token** cada uno; secciones (a)-(g); usar solo hechos de T-DSH-82 y de la tabla verificada por el TL en un kernel de prueba; sin `:NNNN` sueltos (escribir `<puerto>`); sin valores de token.

## 1. Contenido de la guía

`docs/guides/DEV_MODE_OFF_GUIDE.md`, en español y orientada a ejecución manual:

| Sección | Qué incluye |
|---|---|
| 0. Qué cambia (y qué no) | Tabla `dev_mode=true` vs `false`: todo exige token; `/health` y estáticos siguen abiertos; `/api/v1/bash` desaparece (403 aun con token); `?token=` no vale para operador; guilds internas y watchers no cambian. Cómo saber el modo en el arranque. |
| 1. El token maestro | Qué es; vive en `.tylluan-token` en la raíz (o `TYLLUAN_TOKEN`, que tiene prioridad); el kernel genera y guarda uno si falta con `dev_mode=false`; **token por instancia**; cómo copiarlo sin tecnicismos; **nunca** escribirlo en documentos/chats; siempre `<TU_TOKEN>`. |
| 2. Claude Code | Fichero `~/.claude.json`, entrada `mcpServers.tylluan`; ejemplo con `"headers": {"Authorization": "Bearer <TU_TOKEN>"}`; reinicio y comprobación; alternativa SSE con `?token=` (solo clase agente). |
| 3. Dashboard | Carga sin token (estático); diálogo **"Autenticación Requerida"**; campo Bearer Token, botón "Pegar portapapeles", se guarda en `localStorage`; botón "Configurar Bearer Token" si no salta solo; los paneles de operador usan el mismo token maestro. |
| 4. Claude Desktop / `mcp-remote` | ✅ cabecera Bearer con `--header "Authorization:${TYLLUAN_TOKEN}"` + `env` en `claude_desktop_config.json`; ❌ OAuth: con `dev_mode=false` `/oauth/authorize` muestra la pantalla de consentimiento pero el flujo OAuth de escritorio no está terminado → no conecta. `?token=` vale en `/messages`, no en operador. |
| 5. Scripts y CLI | `$env:TYLLUAN_TOKEN` o `.tylluan-token` para los scripts de Coloquio; `tylluan sandbox --token <TU_TOKEN> …` (o `TYLLUAN_TOKEN`), `tylluan stop --token <TU_TOKEN>`; aviso de historial del shell. |
| 6. Agentes del equipo | REST exige `Authorization: Bearer`; los scripts del repo ya leen variable/fichero; un agente no pega el token en Coloquio. |
| 7. Tabla síntoma → causa → arreglo | 401 (sin token / token de otra instancia), dashboard 401 (diálogo sin rellenar), 403 con `?token=` (usar cabecera), 403 de `/api/v1/bash` (deshabilitado, usar `tylluan_do`), Claude Desktop atascado en consentimiento (OAuth no terminado → cabecera), `stop` sin token, script 401 (entorno/cwd), token de otra instancia. |
| 8. Comprobación rápida | `/health` sin token; REST con Bearer → 200; operador con cabecera → 200 y con `?token=` → 403 esperado; `/messages` con `?token=` OK; `/api/v1/bash` con cabecera → 403 esperado; dashboard con token. |
| 9. Referencias | Enlace a `CLIENT_INTEGRATION_GUIDE.md` (mismo directorio) y a los informes internos de dsh T-DSH-82/83 por nombre, sin rutas que no existan aún en `main`. |

Hechos usados, por origen: los de T-DSH-82 (estructura de clientes, rutas operator-class, dashboard, guilds internas, token en `.tylluan-token`, `setup-hint`) y **la tabla que verificó el TL en un kernel de prueba con `dev_mode=false`** (sin token 401; Bearer 200 en MCP/REST/operador; `?token=` vale en `/messages` pero 403 en operador; `/api/v1/bash` 403 aun con token; el token de un kernel no vale en otro; `/oauth/authorize` muestra pantalla de consentimiento).

## 2. Cumplimiento de criterios

| Criterio | Resultado |
|---|---|
| `python scripts/check_doc_links.py` | **exit 0** (222 enlaces relativos en 412 Markdown, todos resuelven) |
| Sin `:NNNN` suelto | **0** coincidencias de `:\d{4,5}`; **0** de `47004`; toda URL usa `http://127.0.0.1:<puerto>` |
| `bash scripts/check_docs_reality.sh` sin errores nuevos | exit 1 solo por el artefacto WSL conocido (`ROADMAP_O3.md`, línea 42); ninguna cita del fichero nuevo |
| Nada de valores de token | **0** cadenas alfanuméricas de 32+ caracteres; solo `<TU_TOKEN>` y tokens ficticios en texto de UI |
| Recuento de tests | `scripts/check_test_count.sh` exit 0 (1364 = 1277 + 75 + 12) |

## Impact

- Deja lista una guía operativa para el apagado de `dev_mode` que José puede seguir sin ayuda: **qué cambia, dónde está el token, dónde se pega en cada cliente** (Claude Code, dashboard, Claude Desktop/`mcp-remote`, scripts, CLI, agentes) y **qué hacer ante cada error típico** (401, 403 por `?token=`, 403 de `/api/v1/bash`).
- Es documentación pura: no toca código, configuración, token ni kernel; no incluye credenciales (solo `<TU_TOKEN>`), y enlaza a la guía de integración existente en vez de duplicarla.
- Recoge explícitamente el límite de OAuth de escritorio (consentimiento visible pero flujo incompleto) para que nadie pierda tiempo intentándolo: con `dev_mode=false` la vía recomendada es la cabecera Bearer.

## No verificado

- **No arranqué ningún kernel** (ni con `dev_mode=false`): la guía se apoya en T-DSH-82 y en **la tabla que verificó el TL en su kernel de prueba**; no re-ejecuté esos 401/403/200 ni el consentimiento OAuth. Los pasos "haz esto y comprueba" son instrucciones, no una prueba en vivo.
- **Textos de UI del dashboard** (título "Autenticación Requerida", campo, botón "Pegar portapapeles", "Configurar Bearer Token", `localStorage`) están tomados del código (`dashboard/src/components/ui/AuthModal.tsx` y `App.tsx` / `packages/tylluan-ui-core`), no de una sesión real con `dev_mode=false`.
- **`mcp-remote` con `--header` + `env`**: el ejemplo sigue la guía de integración del repo (`CLIENT_INTEGRATION_GUIDE.md`) y la tabla del TL; no lo probé con Claude Desktop real.
- **CLI** `tylluan sandbox --token` / `tylluan stop --token`: documentado a partir de T-DSH-83 (implementación propia) y de `--help` ya verificado; no se probó contra un kernel con auth.
- **Rutas operator-class**: la lista de paneles que exigen token maestro viene de T-DSH-82 (código); no se comprobó cada panel en vivo.
- Gates: solo el fallo conocido del artefacto WSL; `check_test_count` ya en verde. Sin commit ni push.
