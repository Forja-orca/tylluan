# MEGATAREA NOCTURNA DE DEEP — 2026-10-22: arnés «con / sin Tylluan» (Q1 y Q3)

**Para quién:** Deep. Trabajas solo en `E:\tylluan-wt-deep` (`git fetch` y `git checkout --detach origin/main` al empezar). **No es una tarea de `src/`**: entregas un arnés reproducible en `benchmarks/longmemeval/` y un informe con números. La cola de cada día (`COLA_deep_2026-10-22.md`: D-27, D-22b, D-22c, D-28, D-29) va **antes** si estás libre; esta megatarea es lo que haces con las horas largas.

## Por qué
La pregunta 3 del documento `docs/reference/research/2026-10-21_preguntas_correctas_camino_a_v1.md` dice que nunca hemos medido si Tylluan ayuda. LongMemEval (Wu et al., ICLR 2025, https://arxiv.org/abs/2410.10813) mide cinco capacidades de memoria: extraer información, razonar entre sesiones, razonar sobre el tiempo, actualizar conocimiento y abstenerse. Esta noche medimos la parte **de recuperación**, sin LLM: es determinista, barata y no depende de un juez.

## Qué entregas
1. **`benchmarks/longmemeval/` con `run.py` (o `run.sh`) y un `README.md` corto**: descarga el conjunto, lo carga en un kernel de prueba, lanza las consultas y calcula métricas. Un solo comando reproduce todo.
2. **Resultados en JSON** (`benchmarks/longmemeval/results/<fecha>_<perfil>.json`) y un informe `docs/internal/IMPL_M-DEEP-1_longmemeval_deep_2026-10.md` con `## Impact` y «no verificado».

## Pasos
1. **Licencia primero.** Lee la tarjeta del conjunto de datos y copia la línea de licencia literal en el informe. Si no permite uso y redistribución para investigación, **para**, no descargues nada más y reporta. No subas el conjunto al repo: el script lo descarga a `E:\tylluan-targets\deep\longmemeval\` (fuera del árbol).
2. **Kernel de prueba aislado:** compila `cargo build --release -p tylluan-kernel` con `CARGO_TARGET_DIR=E:\tylluan-targets\deep` y lanza el binario desde `E:\tylluan-targets\deep\test-run-lme\` con **puerto propio** (elige uno libre, anótalo) y `TYLLUAN_DATA_DIR` propio, `dev_mode=true` solo porque es un kernel de prueba. **Nunca el cwd ni el puerto de producción.** Páralo **solo por el PID que arrancaste**, verificando que su `ExecutablePath` está bajo tu carpeta; nunca `taskkill` ni `Stop-Process` por nombre.
3. **Carga:** variante pequeña (`longmemeval_s`) primero; escribe cada sesión como memorias por la API de memoria (anota si usas `memory/write` o `ingest` y por qué). Respeta que la capa 1 puede rechazar o marcar contenido: **cuéntalo** (rechazos y marcas por tipo), no lo desactives.
4. **Consulta y métrica:** por cada pregunta, `tylluan_recall` con la pregunta y mide **Recall@5 y Recall@10 a nivel de sesión** y, si el conjunto trae turnos de evidencia, a nivel de turno; desglosa por **tipo de pregunta** (las cinco capacidades) y reporta también las preguntas de abstención aparte (aquí la métrica es no devolver nada confiado).
5. **Tres perfiles comparados** sobre las mismas preguntas: solo léxico (`portable`), `clinic` y `server` (reranker incluido si el perfil lo activa). Tabla perfil × capacidad × Recall@5/10 y latencia mediana y p95 por consulta.
6. **Con y sin:** el «sin» es el techo trivial de un agente sin memoria (sin sesiones no hay evidencia: Recall = 0) y el de **contexto completo** (todas las sesiones en el prompt: Recall = 1 pero coste en tokens = suma de sesiones). Calcula el coste en tokens medio por pregunta de «contexto completo» frente a «top-5 de Tylluan» (tokeniza con el tokenizador del modelo de embeddings, dilo).
7. **Muestra para auditoría manual (José evalúa a mano, no con LLM):** exporta `benchmarks/longmemeval/results/muestra_manual_50.md` con 50 preguntas elegidas con semilla fija (10 por capacidad): pregunta, respuesta esperada, los 5 fragmentos devueltos y si contienen la evidencia. Sin juicio automático.
8. **Estadística:** intervalos de confianza por bootstrap (1.000 remuestreos, semilla fija) para cada Recall; si comparas dos perfiles, prueba pareada (McNemar exacto sobre acierto@5) y di p.

## Criterios de aceptación (comandos)
- `python benchmarks/longmemeval/run.py --subset s --profile server` imprime la tabla y escribe el JSON sin intervención.
- Dos ejecuciones consecutivas dan **exactamente** las mismas métricas (el sistema es determinista; si no, documenta la fuente de varianza).
- El informe incluye: licencia literal del conjunto, versión (commit) de Tylluan usada, modelo de embeddings y perfil, máquina (CPU, RAM), tabla por capacidad con intervalos, y el coste en tokens.
- Nada en `src/`; `git status` solo muestra `benchmarks/longmemeval/` y el informe; ningún fichero > 1 MB en el diff.

## Si te atascas
- Descarga bloqueada o conjunto no usable: usa el subconjunto «oracle» o genera un conjunto sintético reproducible con 200 preguntas (documéntalo como tal, no como LongMemEval) y reporta el motivo.
- Más de 2 h sin avanzar en un paso: anótalo en el informe y pasa al siguiente.
- Espacio: el disco de datos tiene margen limitado; borra tus datos de prueba al terminar y no superes 10 GB.

## Reglas
Diff **sin commitear**; el TL commitea. Mensaje al terminar: `[COLA deep] M-DEEP-1 terminado: <ruta del informe>`. Reglas de siempre: ningún `#[test]` con `cfg(unix)`/`cfg(windows)` en la función entera; no tocar `dev_mode` ni tokens de producción; no `--no-verify`; kernels de prueba solo en carpeta propia con puerto y datos propios.
