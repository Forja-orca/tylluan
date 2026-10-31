# Mapa vivo del proyecto Tylluan

Documento de estado para no perder nada. Lo actualiza el TL al final de cada ciclo y lo contrasta cada pocos ciclos un subagente de solo lectura (Haiku). Cada afirmación se puede comprobar con el comando o el fichero citado; lo no comprobado se marca «no verificado». **Última actualización:** 24-oct-2026, ciclo de las 03:xx (reloj de la máquina; los informes de los agentes llevan fecha 2026-10-24).

## 0. Imagen global por bloques (leer ESTO primero; actualizada 2026-10-26)

Regla de atención: ningún bloque recibe la mayor parte del esfuerzo del TL más de dos ciclos seguidos mientras otro bloque esté en rojo. Reparto real de los últimos 30 días: `python scripts/project_balance.py 30` (el 26-oct: 791 commits; seguridad 9 %, memoria 12 %, herramientas 6 %, dashboard 5 %, instalación y release 2 %, cara pública 20 %, tests y CI 14 %, proceso de flota 61 %; un commit cuenta en cada bloque que toca).

| Bloque | Estado verificado | Qué falta | Quién |
|--------|-------------------|-----------|-------|
| Seguridad (frente de cierre) | Guard Origin/Host en MCP, SSE y REST mutante, 415 sin JSON, fail-closed en `delete_node`, avisos en errores de SQLite, bloqueo D-58 arreglado, carrera de WAL arreglada. Verificado en vivo en el kernel `31b965c` | ADR-024 (A4, D1, B1, A1, C3), C1 y `dev_mode=false`, `trust.rs:34`, release v0.17.2. Congelada: nada nuevo salvo hallazgo crítico | Buffy y Deep; decide José |
| Memoria y recuperación (para qué existe Tylluan) | 1374 tests; recall con BM25, vector y grafo; LongMemEval-S v0.12.0 con BGE-M3 (82 % R@5), no repetido con el modelo actual | T-DSH-93: `save_embedding` ignorado en `tylluan_remember`; evaluar nuestra propia memoria (40 preguntas); registro de pérdidas (H-36) | DSH, Hermes, Antigravity, TL |
| Herramientas / guilds | 45 guilds; necesitan Python 3.12 y FastMCP aparte | Bug `TypeError` en `comfy_ui.py:1169`; paso a Python 3.12.15 (decide el TL) | DSH, TL |
| Federación y malla | Red controlada y federación segura (permisos deny-by-default). La red global quedó descartada (José, 26-oct) | Solo mantenimiento | — |
| Dashboard | Existe en `dashboard/`, 151 tests; los binarios publicados NO lo incluyen (se compilan sin `bundled-dashboard`) | Decidir si se publica embebido | José |
| Cara pública | README nuevo (181 líneas EN/ES, 2 diagramas, medición propia) en la rama local `readme-pro-2026-10-25`, SIN empujar; revisado por DSH, Antigravity y Deep | Ronda 2 (Hermes H-35, Buffy M-BUFFY-11), chino (Deep D-61), tu visto bueno | TL, equipo; decide José |
| Instalación y release | v0.17.1 es la última; `main` lleva arreglos sin publicar | `tylluan-cli install` lanza un kernel; `install.sh` ofrece macOS Intel sin binario; el token se crea en el cwd (D-62) | Deep |
| Tests y CI | 1374 tests; flaky arreglados (WAL, semáforo del juez, tiempos); CI verde en `5d21cc7` | 67 `#[ignore]` sin auditar y repetir la suite ×10 (M-BUFFY-12); `pnpm test` del dashboard falló una vez bajo carga | Buffy |
| Proceso de flota | Dos frentes desde el 26-oct: seguridad = Buffy + Deep; producto = Antigravity + DSH + Hermes + TL | Bucle de dogfooding: medir fricción propia, arreglar lo peor, remedir | TL |

## 1. Hechos que caducan (comprobar siempre antes de citar)

| Qué | Cómo se comprueba |
|-----|-------------------|
| `origin/main` | `git fetch && git log -1 --format=%h origin/main` |
| Producción en marcha | `curl http://127.0.0.1:47004/health` (26-oct: `31b965c`) |
| Tests | `bash scripts/check_test_count.sh` (26-oct: 1374 = 1287 kernel + 75 link + 12 fsrs) |
| Cadena de suministro | `bash scripts/check_supply_chain.sh` |
| Reparto del esfuerzo | `python scripts/project_balance.py 30` |

## 2. Equipo y colas (26-oct)

| Agente | Hace ahora | Cómo se le habla |
|--------|------------|------------------|
| Buffy | M-BUFFY-11 (red-team del README) y M-BUFFY-12 (suite ×10, 67 ignorados, soak) | colas en `E:/tylluan-patches/cola/` y la rama del README; las relaya José |
| Deep | D-60 hecho, D-61 (chino sobre la rama actual), D-62 (instalación y release) | ídem |
| DSH | T-DSH-94 (que la memoria no falle en silencio) | Coloquio |
| Hermes | H-35 (ejecutar los comandos del README), H-36 (registro de pérdidas) | Coloquio |
| Antigravity | AG-1 (viaje de un agente nuevo contra el kernel vivo) | Coloquio |
| TL | integra y empuja; protocolo de dogfooding; este mapa | — |

## 3. Registro de ideas e investigaciones (para no perder ninguna)

| # | Idea o investigación | Origen | Estado | Dónde está el material |
|---|----------------------|--------|--------|------------------------|
| I-01 | Tylluan como concentrador de MCP: instalar servidores MCP en cualquier sitio o usar los oficiales por web, conectarlos a Tylluan y que los agentes solo vean sus seis herramientas | José, 23-oct | idea con análisis; falta ADR (lo aprueba José) | el kernel ya tiene `[[external_mcp]]`, la API `/api/v1/mcp/external` y `handler_do/external_mcp.rs`; **hallazgo:** al llamar a un MCP externo copia el texto en todos los campos de argumentos, no pasa argumentos tipados |
| I-02 | Servidor MCP global de Whisper en `E:\MCP-SERVERS` | José, 23-oct | **hecho y probado** con un vídeo de 7 min 6 s; falta guardar la transcripción completa y conectarlo a Tylluan cuando haya concentrador | `E:\MCP-SERVERS\whisper\README.md` |
| I-03 | Guardián recurrente del README (gate determinista + pasada de un subagente barato) | José, 22-oct | **sin hacer** (hubo una pasada única, commit `329f3e3`) | recap, paso 16 |
| I-04 | README nuevo: «stack de IA local, portable, universal vía MCP y federado», no agente | José, 23-oct | borrador en la rama local `readme-draft`; espera el visto bueno de José | `git log readme-draft` |
| I-05 | Cliente de inferencia único y configurable (ADR-020), juez en nube opcional, tope de gasto | José, 22-oct | investigación H-13 a H-20 hecha; enmienda del ADR por redactar | `docs/reference/research/2026-10-24_H1*` y `H2*` |
| I-06 | Hermes con los permisos de un agente de código sin miedo a configurarlo mal | José, 22-oct | propuesta `hermes_propuesta/` sin aplicar | espacio privado del TL (informe y propuesta) |
| I-07 | LM Studio como servidor de decisión: falta el adaptador de `/v1/decisions` | TL, 22-oct | sin tarea | H-13, H-14 |
| I-08 | Transcripción de audio nativa en Tylluan, sin ComfyUI | José, 23-oct | alcance sin decidir | recap, paso 10 |
| I-09 | Semana de vulnerabilidades: Python 3.12.15/3.14.8 (urgente en los guilds), Node, Java 21.0.14, Bun 1.4.4, Windows 25H2 | José, 23-oct | informe hecho; faltan las actualizaciones en ventana | `docs/internal/SEGURIDAD_semana_vulnerabilidades_2026-10-24.md` |
| I-10 | Worktrees baratos y desechables | José, 23-oct | **hecho**: `scripts/agent_wt.sh` y protocolo; falta que cada agente haga su `reset` y `migrate` | `docs/internal/PROTOCOLO_worktrees_agentes.md` |
| I-11 | Bug: `--port` se pierde si se pasa `--config`; `device=auto` colgó un arranque más de 20 min | DSH, T-DSH-57 | en la cola de Deep (D-42, D-43) | `IMPL_T-DSH-57_*` §4 |
| I-12 | Mensaje claro en el primer arranque (huella en frío de 15 a 27 s) | DSH, T-DSH-57 | en la cola de Deep (D-44) | `IMPL_T-DSH-57_*` §4.4 |
| I-13 | Reconciliar la base de datos del kernel 0.17.0 que escribió en `~\.tylluan` entre las 00:48 y las 02:55 | auditoría Haiku | sin dueño; no verificado | `loop_state.md` |
| I-14 | Auditor Haiku periódico y este mapa | José, 23-oct | **en marcha** (primer informe recibido y volcado aquí) | memoria del TL |

## 4. Decisiones que esperan a José

| # | Decisión | Recomendación del TL |
|---|----------|----------------------|
| J-01 | Ventana de mantenimiento de herramientas (`winget`, Python, Node) y reinicio de producción | con los agentes parados; yo dejo el build, el respaldo de `silva.db` y el Python 3.12.15 preparados |
| J-02 | Paso a Windows 25H2 (paquete de habilitación, un reinicio) | en el mantenimiento de la workstation |
| J-03 | README: aprobar el borrador nuevo (rama `readme-pro-2026-10-25`) | tras la ronda 2 del equipo |
| J-04 | ADR-023 (MCP como concentrador): ¿lo redacto? | sí, empezando por paso directo con argumentos tipados |
| J-05 | Enmienda del ADR-020 (cliente de inferencia, nube opcional, tope de gasto) | redactar y aprobar por partes |
| J-06 | Alcance de Whisper en Tylluan (nativo o vía el MCP) | vía el MCP primero |
| J-07 | `mark_uploads_untrusted`, `cron_cancel` en modo estricto, exención de loopback en el consentimiento OAuth, `capabilities_enforce`, rol por defecto con `dev_mode`, reconstrucción del índice de texto de producción, licencia y empaquetado de LFM1.0, ocho decisiones del ADR-022 | ver `loop_state.md`; no las tomo yo |
| J-08 | Aplicar `hermes_propuesta/` en un perfil nuevo, `pre_update_backup: quick`, descartar dos stashes | aplicar en perfil nuevo |

## 5. Siguientes pasos del TL

1. Recibir AG-1, T-DSH-93 (hecho), H-36 y T-DSH-94; fijar las tres peores fricciones de la semana y arreglarlas.
2. Ronda 2 del README y tu visto bueno; después empujar la rama.
3. Con tu aprobación de ADR-024, que Deep implemente y Buffy pruebe; luego la release v0.17.2.
4. Protocolo de dogfooding y prueba de continuidad (¿se puede reanudar solo con Tylluan?).
5. Actualizar ESTE mapa al final de cada ciclo y cuando algo cambie de bloque.

## 6. Riesgos conocidos

| Riesgo | Qué hay hecho |
|--------|---------------|
| Espacio en `E:` | 28,5 a 99,4 GB libres tras limpiar compilaciones antiguas; el método nuevo evita la acumulación |
| Producción sin los arreglos de seguridad y arranque | reinicio pendiente |
| Python de los guilds con CVE sin parchear | plan en el reinicio |
| Documentos que se contradicen | auditoría Haiku periódica; D-45 en la cola de Deep |

## 7. Cómo se mantiene

- El TL lo actualiza al final de cada ciclo con lo que cambió.
- Cada tres ciclos aproximadamente, un subagente de solo lectura lo contrasta con `git`, el Coloquio y los worktrees, y el TL verifica lo que diga antes de actuar.
- Toda idea o investigación nueva de la sesión entra en §3 el mismo día.
