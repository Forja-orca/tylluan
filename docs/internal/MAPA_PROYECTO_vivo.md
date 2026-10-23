# Mapa vivo del proyecto Tylluan

Documento de estado para no perder nada. Lo actualiza el TL al final de cada ciclo y lo contrasta cada pocos ciclos un subagente de solo lectura (Haiku). Cada afirmación se puede comprobar con el comando o el fichero citado; lo no comprobado se marca «no verificado». **Última actualización:** 24-oct-2026, ciclo de las 03:xx (reloj de la máquina; los informes de los agentes llevan fecha 2026-10-24).

## 1. Estado de un vistazo

| Qué | Valor | Cómo se comprueba |
|-----|-------|-------------------|
| `origin/main` | ver `git log -1 origin/main` | `git fetch && git log -1 --format=%h origin/main` |
| Producción (kernel en marcha) | commit `955179f`, sin D-39a/a2, D-38, D-39b, T-DSH-54/55, D-29/D-30, D-40/D-41/D-36 | `curl http://127.0.0.1:47004/health` |
| Tests | 1316 (1229 kernel + 75 link + 12 fsrs) | `bash scripts/check_test_count.sh` |
| Cadena de suministro | `cargo deny` ok, `npm audit` 0, certifi y pip al día en `.venv` y guilds | `bash scripts/check_supply_chain.sh` |
| Reinicio de producción | pendiente: lo lanza José; el cambio de Python de los guilds (3.12.15) va en ese reinicio | recap, paso 5b y 7 |

## 2. Equipo y colas

| Agente | Hace ahora | Siguiente | Cómo se le habla |
|--------|------------|-----------|------------------|
| Deep | cola nueva D-42 a D-45 (puerto con `--config`, `/api/v1/skills`, `device=auto`, mensaje del primer arranque, contradicciones) | relevo de José | `docs/internal/COLA_deep_2026-10-24.md` |
| Buffy | anunciar su trabajo pendiente, M-BUFFY-5 y M-BUFFY-6 (corpus de herramientas envenenadas) | relevo de José | `docs/internal/COLA_buffy_2026-10-24_noche.md` |
| DSH | T-DSH-57 entregada; espera tarea | proponer T-DSH-58 | Coloquio |
| Hermes | H-21 entregada; espera tarea | proponer H-22 | Coloquio |
| Antigravity y Codex | sin trabajo asignado (worktrees atrasados 165 y 298 commits) | decisión de José | — |
| TL | integra, empuja, verifica el CI | ver §5 | — |

## 3. Registro de ideas e investigaciones (para no perder ninguna)

| # | Idea o investigación | Origen | Estado | Dónde está el material |
|---|----------------------|--------|--------|------------------------|
| I-01 | Tylluan como concentrador de MCP: instalar servidores MCP en cualquier sitio o usar los oficiales por web, conectarlos a Tylluan y que los agentes solo vean sus seis herramientas | José, 23-oct | idea con análisis; falta ADR (lo aprueba José) | el kernel ya tiene `[[external_mcp]]`, la API `/api/v1/mcp/external` y `handler_do/external_mcp.rs`; **hallazgo:** al llamar a un MCP externo copia el texto en todos los campos de argumentos, no pasa argumentos tipados |
| I-02 | Servidor MCP global de Whisper en `E:\MCP-SERVERS` | José, 23-oct | **hecho y probado** con un vídeo de 7 min 6 s; falta guardar la transcripción completa y conectarlo a Tylluan cuando haya concentrador | `E:\MCP-SERVERS\whisper\README.md` |
| I-03 | Guardián recurrente del README (gate determinista + pasada de un subagente barato) | José, 22-oct | **sin hacer** (hubo una pasada única, commit `329f3e3`) | recap, paso 16 |
| I-04 | README nuevo: «stack de IA local, portable, universal vía MCP y federado», no agente | José, 23-oct | borrador en la rama local `readme-draft`; espera el visto bueno de José | `git log readme-draft` |
| I-05 | Cliente de inferencia único y configurable (ADR-020), juez en nube opcional, tope de gasto | José, 22-oct | investigación H-13 a H-20 hecha; enmienda del ADR por redactar | `docs/reference/research/2026-10-24_H1*` y `H2*` |
| I-06 | Hermes con los permisos de un agente de código sin miedo a configurarlo mal | José, 22-oct | propuesta `hermes_propuesta/` sin aplicar | `C:\Users\FoRJa\.claude\projects\E--ForjaMCPo3\` (informe y propuesta) |
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
| J-03 | README: aprobar o corregir `readme-draft` | fusionar tras tu visto bueno y con el guardián del README |
| J-04 | ADR-023 (MCP como concentrador): ¿lo redacto? | sí, empezando por paso directo con argumentos tipados |
| J-05 | Enmienda del ADR-020 (cliente de inferencia, nube opcional, tope de gasto) | redactar y aprobar por partes |
| J-06 | Alcance de Whisper en Tylluan (nativo o vía el MCP) | vía el MCP primero |
| J-07 | `mark_uploads_untrusted`, `cron_cancel` en modo estricto, exención de loopback en el consentimiento OAuth, `capabilities_enforce`, rol por defecto con `dev_mode`, reconstrucción del índice de texto de producción, licencia y empaquetado de LFM1.0, ocho decisiones del ADR-022 | ver `loop_state.md`; no las tomo yo |
| J-08 | Aplicar `hermes_propuesta/` en un perfil nuevo, `pre_update_backup: quick`, descartar dos stashes | aplicar en perfil nuevo |

## 5. Siguientes pasos del TL

1. Comprobar el CI del push en curso y asignar tareas a DSH y Hermes.
2. Que cada agente haga `reset` y `migrate` (método nuevo); después `gc` en modo informe.
3. Ventana de reinicio cuando José lo diga.
4. Guardián del README (I-03) y ADR-023 (I-01) si José lo aprueba.

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
