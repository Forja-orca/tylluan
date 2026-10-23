# Recapitulación del 22 y 23 de octubre de 2026

Elaborada por el TL (claude-code) a petición de José. Fuente: `git log` de `main` (22 y 23 oct), `loop_state.md`, informes `IMPL_*` y notas `H-*`. Cada cifra va con el comando que la dio; lo no medido se marca «no verificado». Los informes de los agentes llevan fecha de su propio reloj (`2026-10-24`), por eso aparecen nombres de fichero con esa fecha.

## 1. Ayer (22 oct): seguridad de memoria y capacidades

- **Defensa contra memoria envenenada (ASI06):** la capa 1 normaliza el texto antes de comparar y cubre español (D-31); las cinco vías de escritura sin defensa marcan con la capa 1 y lanzan el juez (D-32). DSH midió falsos positivos (T-DSH-31, T-DSH-36, T-DSH-37).
- **Capabilities:** parser multilínea (D-35a), `exec_tools` declarativo (D-46, T-DSH-46) y bloqueo del salto de la lista de permitidos de `mcp_bridge`.
- **Juez:** manda `model` y los rechazos del backend se ven en `/health` (D-33).
- **ADR-022** (aislamiento de ejecución) corregido tras la revisión independiente de DSH; hallazgos de Hermes H-4 a H-9 (modelos para el juez, coste de sandboxes en Windows, Landlock).
- **README:** guardián de afirmaciones, 13 corregidas y 3 cifras sin fuente retiradas.

## 1b. Hilos de ayer que la primera versión de este recap no recogía

Releídos los mensajes de José del 22 de octubre en el registro de la sesión:

- **Guardián permanente del README (22 oct, 10:58):** José pidió un subagente barato (Haiku) que, entre ciclos, mantenga el README público limpio, al día, con diagramas actualizados y **cotejado con la realidad del proyecto en cada momento**. Lo que existe hoy es una pasada única (commit `329f3e3`: 13 afirmaciones corregidas, 3 cifras sin fuente retiradas); **no existe todavía un guardián recurrente ni un gate determinista** de afirmaciones del README. Es un pendiente real (paso 16 del plan).
- **Inferencia de «cliente único y configurable» (22 oct, 21:05):** José considera sobreingeniería la forma de probar modelos dentro de Tylluan (decisión, generación y embeddings) y trajo la idea de desacoplar la inferencia por HTTP (LiteLLM, OpenRouter, protocolo OpenAI). El TL la acotó: la decisión de dónde corre la inferencia es del usuario, local sigue siendo una dirección más, la localidad debe alimentar la política y se quita la gestión del ciclo de vida de modelos, no los modelos locales. Pendiente de José: enmienda corta del ADR-020, nube opcional para el juez, tope de gasto. La investigación H-13 a H-20 es la evidencia.
- **Hermes con los mismos permisos que el harness del TL (22 oct, 23:39):** José quiere que Hermes trabaje como un agente de código, sin miedo a configurarlo inseguro y sin que cada actualización sea un dolor. Entregados: informe de configuración, propuesta de configuración segura en `hermes_propuesta/` (sin aplicar) y H-12/H-20. Pendiente de José: aplicarla en un perfil nuevo, `pre_update_backup: quick` y limpiar dos stashes.
- **Modelo d1-3B y LM Studio (22 oct, 21:58 y 22:05):** estudiar el modelo, usar el LM Studio cargado y no parar el banco aunque haya poca CPU. El banco completo se perdió en la caída del sistema; relanzarlo solo con luz verde.
- **Origen de D-38 (23 oct, 00:26):** José no pudo conectar ningún agente porque el kernel había arrancado en otro puerto; de ahí que D-38 prohíba el puerto aleatorio.
- **Contexto de fondo que no cambia (memoria del proyecto):** Tylluan como la mitad local, persistente y federada que falta al modelo; la sociedad de modelos pequeños es el sistema nervioso interno que protege la memoria; fortaleza contra el malo, nunca jaula para el agente legítimo; caso de diseño del médico con una Raspberry Pi sin conexión que de noche se enlaza con el servidor del hospital. El README nuevo cubre la primera impresión; **no** cuenta todavía este «hacia dónde va».

## 2. Hoy (23 oct): arranque, credenciales y semana de vulnerabilidades

| Tema | Qué se hizo | Evidencia |
|------|-------------|-----------|
| Credenciales de guilds | D-34: las guilds ya no reciben el token del operador (credencial efímera por guild, `env_clear` con lista blanca) | commit `b846545`; guardia permanente T-DSH-49 |
| Arranque lento | Causa raíz: `RepoMap::build` se esperaba antes de abrir el puerto (945 990 ms en el arranque real). D-39a y D-39a2 lo bajan a 1,235 s medidos sobre el árbol real | `repo_map_real_measure` (`--ignored`) y `IMPL_T-DSH-52` |
| Resto del arranque | Mapa de lecturas previas al bind (T-DSH-51, T-DSH-53); caché de la huella sha256 de 1,34 GB (T-DSH-54); sesiones ONNX en paralelo (T-DSH-55) | 29 y 1207 tests lib; ahorros reales **no medidos** |
| Arranque asíncrono | D-39b: el mapa se construye tras abrir el puerto y `/health` pasa por `starting`; D-38: puerto ocupado reintenta y falla claro (nunca puerto aleatorio) | 1214 tests lib, clippy `-D warnings` exit 0 |
| Persistencia | D-29: `consents.json` corrupto se conserva; D-30: los planes pendientes sobreviven al reinicio (caducan a las 24 h) | 4 pruebas de aceptación |
| Investigación ADR-020 | Hermes H-10 a H-20: dialectos de inferencia, `/v1/decisions`, localidad de endpoint, timeouts, topes de gasto, credenciales | citas verificadas por el TL contra las fuentes |
| Vulnerabilidades | Informe `SEGURIDAD_semana_vulnerabilidades_2026-10-24.md`: `tokio` 1.54.1, certifi, pip, overrides de tiptap, ignore obsoleto de `deny.toml`; `check_supply_chain.sh` | `cargo deny check advisories` = ok; `npm audit` = 0 |
| Integración | Pushes 127 a 134 (último `85ce705`) | CI verde hasta `d6b53cd`; en `85ce705` falla un meta-test (ver §6) |

## 3. Decisiones y encargos de José

- **«Hasta el mantenimiento de la workstation no podemos migrar de versión de Windows.»** El paso a 25H2 queda para esa ventana.
- **Reiniciar Tylluan** cuando Deep y los demás acaben sus tareas; avisarle cuando todos estén listos.
- **Semana de vulnerabilidades:** investigar todas, ver cómo afectan (de Windows a Tylluan) y parchear.
- **Posicionamiento:** «Tylluan nunca se creó para ser un agente»; es casi un stack de IA local, portable, universal vía MCP y con federación. Guardado en memoria.
- **README:** usar la presentación `Tylluan_Sustrato_Cognitivo_Soberano.pdf` como base; el texto debe poder cambiar en el futuro.

## 4. Lo que salió del brainstorming del README

- La frase anterior listaba ingredientes (memoria, grafo, herramientas) que cualquier proyecto de memoria reclama, y «tool execution» sonaba a *tool calling*.
- Problema que se entiende en cinco segundos: *las herramientas de IA no se recuerdan entre sí*; Tylluan es la memoria y las herramientas compartidas, en tu máquina.
- Lo realmente distinto: un binario sin nube con cifras medidas; memoria **y acciones** protegidas; independiente del modelo y del cliente.
- Cautelas que el texto debe respetar: cifras de recuperación de una medición antigua (BGE-M3), federación probada solo en red local, `tylluan_decide` aún sin modelo, endurecimiento de red fase 2 pendiente. «Governed» suena a jaula (principio *fortaleza, no jaula*): preferir «protegido» o «verificable».
- Del PDF se descartaron «seguridad inquebrantable», «cero latencia» y las cifras del perfil `server` sin su aviso.
- Estado: nuevo README en la rama local `readme-draft` (commit `a98d409`), **sin empujar**, a la espera de que José lo vea. El README en chino sigue pendiente de revisión nativa.

## 5. Estado de producción

- El kernel en marcha sigue en `955179f`: no tiene D-39a/a2, D-38, D-39b, T-DSH-54/55, D-29/D-30 ni las correcciones de `tokio`/certifi del Python empaquetado (esto último sí está aplicado a los ficheros en disco, no al proceso).
- Antes de reiniciar: que Deep, DSH, Buffy y Hermes paren; build release; copiar `data/silva.db`; avisar en el Coloquio; José lanza el lanzador; comprobar que el commit de `/health` coincide con `origin/main`.
- Aviso para el reinicio: `/health` puede mostrar `starting` el primer segundo; con D-38 un puerto ocupado ya no arranca en otro.

## 6. Pendiente en el momento de escribir esto

- CI rojo en `85ce705`: el meta-test `test_verify_semantics.sh` (T6, cero scripts huérfanos) detecta `check_supply_chain.sh` sin cablear. Arreglo en curso: cablearlo en `verify.sh` como solo-informe y declararlo en la lista de solo-informe del meta-test.
- Entregas de Deep sin verificar: D-40, D-41 y D-36. DSH: T-DSH-57 (medición con un kernel `test-run-*`). Hermes: H-21.

## 7. Plan de pasos futuros

| # | Paso | Quién | Depende de | Hecho cuando | ¿Decide José? |
|---|------|-------|------------|--------------|----------------|
| 1 | Arreglar el CI de `85ce705` (cablear `check_supply_chain.sh`) | TL | nada | `bash scripts/test_verify_semantics.sh` sale con 0 y el CI de ese hash queda en verde | no |
| 2 | Verificar e integrar D-40, D-41 y D-36 de Deep | TL | 1 | `cargo test --lib` y clippy verdes con los parches aplicados en orden | no |
| 3 | Medir el arranque con un kernel de prueba aislado (T-DSH-57) | DSH | 1 | tabla de tiempos hasta `/health`, `ok` y `repo_map ok`, sin tocar producción | no |
| 4 | H-21: cómo actualizar de forma soportada los Node embebidos y certifi de Hermes | Hermes | nada | nota con citas del repo de Hermes | no |
| 5 | Ventana de mantenimiento de herramientas: agentes parados, `winget upgrade` de Node, Python 3.14.7, Git, Chrome/Edge y Docker; Bun 1.4.4; Java 21.0.14 cuando exista | TL | 2, 3, 4 | versiones reinstaladas y `scripts/check_supply_chain.sh` en OK | **sí: elegir la ventana** |
| 6 | Windows 24H2 a 25H2 con el paquete de habilitación (un reinicio) | José | mantenimiento de la workstation | compilación 25H2 y parches recibiendo | **sí** |
| 7 | Reinicio de producción: build release, copia de `silva.db`, aviso, lanzamiento, `/health` igual a `origin/main` | TL y José | 1 a 5 | commit de `/health` igual al de `origin/main`; `starting` y puerto verificados | **sí: autorizar y lanzar** |
| 8 | Validación tras el reinicio: arranque real medido, mapa del repo, plan de vuelta atrás | TL | 7 | tiempos medidos y comparados con T-DSH-52/55 | no |
| 9 | README: revisar `readme-draft`, fusionar y empujar; traducción china con revisión nativa | TL y José | visto bueno de José | README publicado, enlaces comprobados (`check_doc_links.py`) | **sí** |
| 10 | Transcripción local de audio dentro de Tylluan (hoy depende de ComfyUI con Whisper); transcribir el vídeo del cuaderno | por asignar | decisión de José | `audio_transcribe` funciona sin ComfyUI o se transcribe con una herramienta aparte | **sí: alcance** |
| 11 | Enmienda del ADR-020 (cliente de inferencia configurable, juez en nube opcional, tope de gasto) con la evidencia H-13 a H-20 | TL redacta | José | ADR en revisión | **sí: aprobar** |
| 12 | Banco del juez y corpus difíciles de Buffy; el banco completo de d1-3b solo con luz verde | Buffy | José lo releva | informes `IMPL_M-BUFFY-*` verificados | **sí** |
| 13 | Reconstruir la imagen Docker sobre la base Debian actualizada | TL | 7 | `Docker Smoke` en verde | no |
| 14 | Renovar el cron de ciclos del TL antes de su caducidad de siete días | TL | nada | cron recreado | no |
| 15 | Decisiones abiertas de José: `mark_uploads_untrusted`, `cron_cancel` en modo estricto, licencia y empaquetado de LFM1.0 (d1-3B), `pre_update_backup` de Hermes, perfil `tylluan-dev` de Hermes, `[silva] sync_interval_ms`, reconectar `/mcp`, `dev_mode=false` más adelante, las ocho decisiones del ADR-022 | José | nada | cada una decidida y anotada | **sí** |
| 16 | Guardián recurrente del README: gate determinista de afirmaciones (puertos, versiones, recuentos, lista de herramientas contra el código) y pasada de un subagente Haiku entre ciclos | TL | 9 | `scripts/` con el gate cableado en `verify.sh` y el README nuevo pasándolo | no |

## 8. No verificado

- Ahorro real de arranque de T-DSH-54, T-DSH-55, D-39b: requiere un arranque medido (paso 3).
- Si Windows Update ofrece 25H2 en esta máquina; si Microsoft distrust-ó las intermedias de eMudhra (solo fuentes de terceros).
- Que el README nuevo se entienda a primera vista: pendiente de la prueba con 3 a 5 lectores externos.
