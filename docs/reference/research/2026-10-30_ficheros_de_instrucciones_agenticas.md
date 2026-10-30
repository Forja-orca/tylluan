# Cómo deben ser los ficheros de instrucciones (CLAUDE.md / AGENTS.md), globales y de Tylluan

Fecha: 2026-10-30 · Autor: TL · Estado: propuesta, sin aplicar · Pedido de José mientras esperaba.

## Fuentes leídas hoy
- Anthropic, "How Claude remembers your project": https://code.claude.com/docs/en/memory (guardada en disco; citas literales).
- agents.md: https://agents.md (el extractor devolvió un resumen; esas frases NO son literales y van marcadas).

## Lo que dicen las fuentes
- "Claude treats them as context, not enforced configuration. To block an action regardless of what Claude decides, use a PreToolUse hook instead."
- "target under 200 lines per CLAUDE.md file. Longer files consume more context and reduce adherence."
- "Imports help you organize a long file but don't reduce its context cost, because imported files load together with the CLAUDE.md that imports them."
- "if two instructions contradict each other, Claude may pick one arbitrarily."
- "By default, Claude reads AGENTS.md only when you have no CLAUDE.md in your working directory or above it." (con las dos, lee solo CLAUDE.md; la vía compartida es un CLAUDE.md que importe AGENTS.md con `@AGENTS.md`).
- "Block-level HTML comments (`<!-- maintainer notes -->`) in CLAUDE.md files are stripped before the content is injected into Claude's context."
- "If an entry is a multi-step procedure or only matters for one part of the codebase, move it to a skill or a path-scoped rule."
- No lee: `AGENTS.local.md`, `AGENTS.override.md` ni nada bajo `.agents/` (Antigravity sí lee `.agents/AGENTS.md`).
- agents.md (resumen, no literal): el AGENTS.md más cercano al fichero editado manda; la instrucción explícita del usuario manda sobre todos.
- Existe `/doctor prompt-audit` para detectar referencias muertas y contradicciones.

## Medido hoy (comando: `wc -c`, `wc -l`, `ls`)
| Fichero | Tamaño | Objetivo oficial |
|---|---|---|
| `E:\tylluan\CLAUDE.md` | 337 líneas, 28.983 B | < 200 líneas |
| `E:\tylluan\AGENTS.md` | 251 líneas, 21.396 B | < 200 líneas |
| `E:\ForjaMCPo3\CLAUDE.md` (carpeta de trabajo de esta sesión) | 15.125 B, habla de Forja, 5 tools y puerto 3030 | debe reflejar Tylluan: 6 tools (CONTRACT-01), puerto 47004 |
| Reglas globales siempre cargadas (10 de `ecc/common` + `ecc/README.md` + `review.md`) | 24.831 B | el README no es una regla |
| `~/.claude/CLAUDE.md` global | no existe | recomendado: corto, solo preferencias personales |
| Índice de memoria `MEMORY.md` | 29,5 KB, tope 24,4 KB, 22 líneas cortadas | cabe entero |

## Hallazgos (finding → dónde → arreglo)
1. Ficheros del repo de ~2× el tamaño recomendado → `CLAUDE.md` líneas 98-123 son historia (autonomía 2026-09, GraphRAG resuelto) → mover a `STATUS.md`/`docs/`; dejar el fichero en reglas vigentes.
2. Dos ficheros casi iguales a mantener (`CLAUDE.md` y `AGENTS.md`) con un gate que vigila el bloque común → fuente única `AGENTS.md` (≤ 200 líneas); `CLAUDE.md` = `@AGENTS.md` + lo propio de Claude/TL.
3. Reglas "duras" escritas como texto (nunca `--no-verify`, nunca arrancar prod por Bash, nunca borrar los logos) → convertirlas en hooks PreToolUse y permisos `deny`; el texto sirve de explicación, no de garantía.
4. Instrucciones por zona cargadas siempre → `.claude/rules/` con `paths:` (kernel Rust, dashboard, docs y sus puertas).
5. `ecc/README.md` (5.476 B) se carga como regla en cada sesión → sacarlo de `~/.claude/rules/`.
6. Contradicción real: `E:\ForjaMCPo3\CLAUDE.md` afirma 5 tools y puerto 3030; Tylluan tiene 6 y puerto 47004 → corregir o acotar para que no confunda a un agente que arranque ahí.
7. Sin `~/.claude/CLAUDE.md` global: las normas de José (comprobar antes de afirmar, nunca arrancar procesos por Bash, categorías solo-humano, español) viven repartidas entre memoria y repos → una página corta global.
8. Índice de memoria excedido: 22 entradas no se cargan → compactar entradas a una línea y mover detalle a ficheros.

## Estructura propuesta
- Global `~/.claude/CLAUDE.md` (≤ 60 líneas): idioma, norma de comprobar, seguridad de procesos, categorías solo-humano, cómo informar.
- Global `~/.claude/rules/`: solo reglas cortas y específicas; `review.md` se queda.
- Tylluan `AGENTS.md` (≤ 200): invariantes, comandos de build/test/gates, protocolo de flota; sin historia.
- Tylluan `CLAUDE.md` (≤ 80): `@AGENTS.md`, rol del TL, ciclo, lo propio de Claude Code.
- `.claude/rules/*.md` con `paths:` para kernel, dashboard, docs.
- Hooks para lo inviolable.

## No verificado
- No he ejecutado `/doctor prompt-audit` sobre los ficheros.
- No sé qué parte del texto actual cargan hoy los agentes de otros runtimes (Antigravity, Codex, Hermes).
- Las frases de agents.md son del resumen del extractor, no literales.
- No medí el impacto real en adherencia al acortar; la cifra oficial es una recomendación del proveedor.
