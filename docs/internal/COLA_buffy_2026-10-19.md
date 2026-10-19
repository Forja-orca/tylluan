# Cola de trabajo de Buffy (segundo plano) — 2026-10-19

**Cómo usarla.** José te dirá en tu chat que la leas. Trabaja en orden, sin esperar al TL (no lees Coloquio hasta que José lo pida). Reglas fijas: `git fetch` y `git checkout --detach origin/main` en `E:\tylluan-wt-buffy`; **diff sin commitear**; no toques el kernel de producción; kernels de prueba solo en `test-run-*` con puerto y `DATA_DIR` propios.

**Al terminar cada ítem:** informe `docs/internal/IMPL_<item>_buffy_2026-10.md` con `## Impact` y "no verificado", y un mensaje en Coloquio `[COLA buffy] <item> terminado: <ruta>`. Al final: `[COLA buffy] COLA COMPLETA`. Si un ítem se atasca más de 2 h, anota el bloqueo y pasa al siguiente.

## B-1 — H1: higiene del ROADMAP
Tabla `casilla -> estado declarado -> estado real -> evidencia (archivo:línea, hash o comando)` para las secciones de ADR-019/020/021, OAuth, dashboard y producción de `docs/roadmap/ROADMAP_O3.md`. No edites el ROADMAP (lo hace el TL). El 2026-10-19 se actualizaron casillas con sus commits; verifica también esas.

## B-2 — H2: parsers de URI y host
Busca en `crates/tylluan-kernel/src` otros parsers hechos con `split(':')`, `split('@')`, `starts_with("http")` o `contains("localhost")` (misma clase que el fallo de OAuth `redirect_uri` con userinfo). Tabla `sitio -> riesgo -> entrada que lo explota -> arreglo propuesto`. No arregles; lista.

## B-3 — Evaluación del recall con juicio manual (el ítem grande)
Objetivo: saber si `tylluan_recall` sirve de verdad. **Sin LLM como juez**: tú etiquetas a mano, con el criterio fijado antes de empezar y escrito en el informe.
1. Haz una copia segura de las bases con la API de backup de SQLite en modo lectura (`sqlite3.connect('file:...?mode=ro', uri=True)` + `.backup`); nunca copies el fichero en caliente. Trabaja solo sobre la copia.
2. Arranca un kernel de prueba en `test-run-recall` con `DATA_DIR` apuntando a la copia, puerto aleatorio y `[p2p] enabled = false`.
3. Extrae 60 preguntas reales de agentes de Coloquio (turnos que preguntan cómo, dónde o qué), saneadas (sin claves, rutas absolutas ni correos).
4. Para cada una, lanza el recall por REST y etiqueta los 5 primeros resultados: relevante / parcial / irrelevante.
5. Informe: hit@1, hit@5, MRR, % de consultas con 0 resultados, latencia p50/p95, y 10 fallos ilustrativos con la causa probable. Guarda el conjunto etiquetado en `benchmarks/recall_manual_v1.json` (con fichas de origen y criterio).

## B-4 — Corpus de inyección contra CoherenceGate y la puerta de escritura
40 muestras de inyección de prompt de fuentes públicas que puedas verificar (cita la URL de cada una y comprueba que dice lo que afirmas) y 40 textos benignos técnicos. Mide detección y falsos positivos de CoherenceGate y de la puerta de escritura (`tylluan_remember`). Informe con tabla; no cambies código. Si algo se cuela, descríbelo con la entrada exacta.

## B-5 — Activar la suite adversarial de OAuth F2
Solo cuando Deep termine las fases A y B (aparecerá `[COLA deep] D-3 terminado`): quita los `#[ignore]` de `tests/oauth_f2_adversarial_spec.rs` que correspondan, ejecútalos y reporta cuáles pasan y cuáles no.

**Exclusiones:** no tocar `src/` del kernel; no bajar umbrales de los gates; no `--no-verify`.
