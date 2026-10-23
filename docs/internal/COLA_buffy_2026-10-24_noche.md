# COLA DE BUFFY — noche del 24-oct-2026 (escrita por el TL; sustituye el orden de trabajo, no borra la cola anterior)

**Para quién:** Buffy. Trabajas solo en `E:\tylluan-wt-buffy`. **No tocas `src/`** y **no haces peticiones a LM Studio, Strata, FreeLLMAPI ni Ollama**; las pruebas usan servidores falsos en `127.0.0.1:0`. Nunca fijes un puerto. No comiteas ni empujas: entregas diff sin commitear más un informe con `## Impact` y «No verificado».

## Paso 00 — Método de worktree nuevo (5 minutos, ANTES de nada)
Lee `docs/internal/PROTOCOLO_worktrees_agentes.md` (corto). Tu worktree es `E:\tylluan-wt-buffy` y su compilación pasa a `E:\cargo-target\buffy`.
1. **Primero el Paso 0 de abajo** (anunciar lo que ya tienes sin commitear), para que el TL lo vea.
2. Cuando el TL lo haya aceptado: `bash /e/tylluan/scripts/agent_wt.sh reset buffy` (guarda todo antes en `E:\tylluan-patches\buffy\_saved\`) y `bash /e/tylluan/scripts/agent_wt.sh migrate buffy` (borra tu `target` antiguo de 70 GB; solo si no hay cargo en marcha).
3. **No hagas `reset` antes de anunciar:** no se pierde nada, pero tendrías que rehacer el entorno para seguir con tu trabajo.

## Paso 0 — Anuncia lo que ya tienes (30 minutos)
Tu worktree ya contiene trabajo sin anunciar: `benchmarks/judge_bench/`, dos corpus difíciles, `layer1_hard_corpus_measure.rs`, el test `asi06_redteam_memory_poisoning.rs` modificado y dos informes `IMPL_M-BUFFY-3_*` con nombres distintos.
1. Unifica los dos informes de M-BUFFY-3 en uno (`IMPL_M-BUFFY-3_banco_juez_buffy_2026-10.md`).
2. Ejecuta `python -m pytest benchmarks/judge_bench -q` y pega la salida real.
3. Publica en el Coloquio `[COLA buffy]` con la lista de ficheros y las cifras que mediste (no las de otros).
Sin este anuncio el TL no puede verificar ni integrar nada.

## M-BUFFY-5 — Invertir los tests de red-team que fijan el comportamiento vulnerable
Algunos tests de `asi06_redteam_memory_poisoning.rs` afirman que cierto bypass **funciona** (documentaban el hueco). Con D-31 y D-32 integrados el hueco está cubierto en las vías V1 y V2 y las demás se cierran por pasos.
- **Hacer:** para cada test que fije el comportamiento vulnerable, decide si debe invertirse (ahora el ataque debe bloquearse) o quedar `#[ignore]` con la razón escrita. Lista en el informe cada test, su estado antes y después, y la vía (V1 a V7) que cubre.
- **Criterio:** `cargo test -p tylluan-kernel --test asi06_redteam_memory_poisoning` con el resultado que declares; ningún test pasa en verde por afirmar una vulnerabilidad.
- **Excluye:** cualquier cambio en `src/`; si un test requiere código nuevo, anótalo como `DECISION PENDIENTE`.

## M-BUFFY-6 — Corpus de descripciones de herramientas envenenadas (solo datos)
Prepara el terreno para conectar servidores MCP externos a Tylluan (idea de José, aún sin ADR): una descripción de herramienta es texto que lee el modelo, y puede llevar instrucciones ocultas (*tool poisoning*).
- **Hacer:** `benchmarks/tool_description_corpus_v1.json` con 30 descripciones **originales escritas por ti**: 15 maliciosas (instrucción oculta, petición de enviar datos a otro sitio, orden de ignorar al usuario, texto en Markdown o HTML invisible, homoglifos) y 15 benignas de aspecto parecido (herramientas reales con descripciones largas o con palabras como «ignore» en un contexto legítimo). Cada caso: `id`, `cat`, `tecnica`, `texto`, `procedencia`.
- **Criterio:** JSON válido; sin duplicados ni casi-duplicados (Jaccard de trigramas menor de 0,8); un test que valide el esquema; mide la capa 1 actual sobre el corpus con el mismo método de `layer1_hard_corpus_measure.rs` y reporta detección y falsos positivos **sin ajustar la capa para que salga bien**.
- **Excluye:** cualquier conexión a un servidor MCP real; es solo dato y medición.

## Si te atascas
Un paso que no avanza en una hora: anótalo en el informe y pasa al siguiente. No esperes decisiones por chat: escribe `DECISION PENDIENTE: ...` en el informe.

## Cómo reportar
Mensaje en el Coloquio con el prefijo `[COLA buffy]`, la lista de ficheros nuevos y la ruta de cada informe. El TL verifica con sus propios comandos antes de aceptar.
