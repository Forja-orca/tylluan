# Verificación del TL de la auditoría integral de Codex (snapshot 4db12a6, 2026-10-28)

> La auditoría original es un informe HTML de Codex, estático, sin ejecutar nada, en la máquina de José. Aquí consta lo que el TL comprobó contra el código y el estado de cada punto. Las referencias de línea van como "línea N" por el gate de puertos de `check_docs_reality.sh`.

## Tabla de verificación

| ID | Hallazgo | Verificado por el TL | Dónde (comando) | Acción y responsable |
|---|---|---|---|---|
| AUD-01 | `tylluan update` pide `checksums.txt` como asset y el workflow solo publica los tar.gz | **Confirmado** | `grep -n "checksums" .github/workflows/release.yml`: el manifiesto se creaba dentro de cada paquete (línea 109) y `files` solo listaba los tar.gz (línea 165) | **Corregido** por el TL: el job `package-draft` genera `checksums.txt` sobre los tar.gz y lo adjunta. Pendiente de probar en la próxima release (no hay forma de ejecutarlo antes) |
| AUD-02 | El servidor HTTP no recibe señal de cierre y `reap_idle` solo recoge guilds inactivos | **Confirmado** | `axum::serve(...).await` sin `with_graceful_shutdown` en `transport/http/mod.rs`, línea 502; `registry.reap_idle()` en `main.rs`, línea 2223; el filtro excluye `always_on` y guilds activos | T-DSH-125 (medir y arreglar). Explica en parte la parada lenta observada el 2026-10-28 |
| AUD-03 | Vision sube ficheros con `fetch` directo, sin bearer | **Confirmado** | `dashboard/src/components/VisionTab.tsx`, línea 119: `fetch('/api/v1/ingest/upload', { method: 'POST', body: form })` | AG-13 (Antigravity). Hoy no falla porque producción corre con `dev_mode = true`; fallaría al cerrarlo |
| AUD-04 | BWC cuenta registros, no participantes; mayoría sobre los votos presentes | **Confirmado** | `api_contracts.rs`, líneas 395-399 (`deliveries.len() >= team.len()`) y 425-432 (`approves > total / 2`) | T-DSH-124 (DSH) |
| AUD-05 | El API responde éxito aunque falle la escritura durable | **Confirmado** | `grep -n "let _ = persist_contract" api_contracts.rs`: 5 sitios | T-DSH-124 (DSH) |
| AUD-06 | Intents completos en el log persistente | **Confirmado** | `handler_do/mod.rs`, líneas 188-191 (`intent = %intent`); `grep -c "intent='" logs/kernel.log`: 219 | H-47 (Hermes) |
| AUD-07 | `expose_guild_tools=true` expone más de seis tools | **Cierto como diseño, sin efecto hoy** | Producción tiene `expose_guild_tools = false` y `tools/list` en vivo devuelve exactamente 6 | Decisión de José: enmienda formal de CONTRACT-01 o retirar el flag |
| AUD-08 | Cambiar de motor de embeddings mezcla espacios vectoriales | **Confirmado** | `embedding_fingerprint_filter` por defecto `false` (`config.rs`, línea 2335); la BD viva mezcla `mxbai-embed-large-v2-onnx` 4079, `bge-m3-v2-onnx` 1083, `distill-v1` 781 | Medir el efecto de activarlo antes de decidir (H-42/H-43 ya miden el backlog); decide José |
| LEAD-01 | BWC: el `agent_id` del payload no se liga al bearer | No probado | es la decisión X1 de José (binding token a agent_id) | Prueba HTTP en kernel aislado, tras decidir X1 |
| LEAD-02 | OAuth sin mapeo hereda `default_role = admin` | **Confirmado en la config, moot hoy** | `tylluan.toml` de producción: `dev_mode = true` (sin auth) y `default_role = "admin"` | Ya es la decisión J-2 (C1 + `dev_mode=false` + quitar el rol por defecto) |

## Lo que la auditoría hizo bien

- Comprobó que el aviso histórico del gate P2P ya estaba cerrado y no lo repitió como hallazgo (el patrón de fallo de auditorías anteriores).
- Separó lo confirmado de los leads sin petición real, aplicando la regla de `REVIEW.md` sobre hallazgos de autenticación.
- Citó `REVIEW.md` y la tabla de rutas de lectura: el orden de lectura llegó a un agente externo.

## No verificado

- Que los cuatro primeros puntos de la tabla se comporten así en runtime: la verificación del TL es de lectura de código y de la configuración viva, no de ejecución.
- AUD-01: el workflow no se pudo ejecutar; solo se validó el YAML.
