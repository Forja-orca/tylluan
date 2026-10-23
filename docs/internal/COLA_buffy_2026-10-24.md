# COLA DE BUFFY — 2026-10-24 (escrita por el TL mientras José descansa)

**Para quién:** Buffy. Trabajas solo en `E:\tylluan-wt-buffy` (`git fetch` y `git checkout --detach origin/main`). **No tocas `src/`** y **NO lanzas ninguna petición contra motores locales reales** (LM Studio :1234, Strata :8080, FreeLLMAPI :31415, Ollama): hay agentes usándolos y la máquina va justa. Todo se desarrolla y prueba contra un **servidor falso en `127.0.0.1:0`** (puerto que elige el SO). Entregas un diff **sin commitear** + informe `IMPL_*` con `## Impact` y «no verificado». Mensaje final: `[COLA buffy] <tarea> terminada: <ruta del informe>`.

## M-BUFFY-3 (REORIENTADA) — El banco del juez como herramienta, no como proyecto de máquina
Contexto: José ya tiene servidores locales (LM Studio con varios modelos, entre ellos `d1-3b` de LiquidAI, un modelo de decisión que contesta con probabilidades en un solo pase). Probar un modelo es cambiar `--base-url` y `--model`. Hay un prototipo en `E:\tylluan-patches\bench\bench_decision_judge.py` (léelo; es tuyo para mejorarlo). Una sonda de 8 textos dio 8/8 con la formulación `choice`; el banco completo se perdió en una caída porque solo guardaba al final.

Entrega `benchmarks/judge_bench/` con:
1. `run.py --base-url URL --model ID --dialect systemone|chat --out resultado.jsonl [--limit N] [--sleep-ms 200] [--resume]`.
   - **Guardado incremental**: una línea JSONL por texto, con flush inmediato, y `--resume` que salta lo ya hecho.
   - Secuencial, nunca concurrente.
   - Dialecto `systemone`: `POST {url}/v1/systemone` con `questions` (`choice` y `noul`).
   - Dialecto `chat`: `/v1/chat/completions` con el prompt y la gramática reales del juez (léelos en `coherence_gate.rs`; cita ruta:línea).
2. Corpus: `benchmarks/injection_corpus_v1.json` (40 EN), `injection_corpus_es_v1.json` (80 ES), `benign_corpus_v1.json` (40), `benign_corpus_es_v1.json` (300).
3. `report.py`: detección y falsos positivos por umbral (0,3 / 0,5 / 0,7 / 0,9), **por categoría y por técnica de ofuscación**, intervalo de confianza de Wilson al 95 %, p50/p95 de latencia, y una línea «CPU / pocos recursos» si se declara con `--note`.
4. Pruebas con un servidor falso en `127.0.0.1:0` que cubran: éxito, 400, timeout, JSON roto, corte a media corrida y `--resume`.

**Criterio:** `python -m pytest benchmarks/judge_bench -q` en verde; ningún puerto fijo; ninguna petición a servidores reales; el informe lleva la tabla de métricas del servidor falso (no inventes números de modelos reales).

## M-BUFFY-4 — Más corpus difícil (solo datos, sin motores)
Amplía los corpus con casos **escritos por ti y originales** (sin copiar de fuentes con licencia dudosa; declara la procedencia como en los ficheros actuales):
- (a) 40 inyecciones en español de alta dificultad: parafraseadas, partidas en dos frases, dentro de bloques de código o Markdown, con homoglifos, dentro de un JSON, o con un rol falso de «sistema».
- (b) 60 **negativos difíciles**: texto técnico sobre inyección de prompts, código con `ignore` / `override`, actas que dicen «ignorar las instrucciones antiguas del manual», documentación de seguridad.

Cada caso lleva `id`, `cat`, `tecnica`, `texto`.
**Criterio:** JSON válido; sin duplicados exactos ni casi-duplicados (Jaccard de trigramas < 0,8 contra el resto); un test que valide el esquema; informe con recuento por categoría. Mide también la capa 1 actual con `matches_injection_pattern` a través del test de integración existente (`layer1_fp_hunt_repo.rs` como modelo), sin kernel: debería dejar pasar al menos el 30 % de (a) y marcar menos del 10 % de (b). Reporta los números reales.

## Si te atascas
Un paso que no avanza en 1 h: anótalo en el informe y pasa al siguiente. No esperes decisiones por chat: escribe `DECISION PENDIENTE: ...` en el informe.
