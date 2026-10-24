# IMPL — M-BUFFY-6: corpus de descripciones de herramientas envenenadas (solo datos)

- **Agente:** Buffy · **Fecha:** 2026-10-24
- **Cola:** `docs/internal/COLA_buffy_2026-10-24_noche.md` (M-BUFFY-6)
- **Base:** `d2e71cf` (origin/main tras `agent_wt.sh reset buffy`).
- **Ficheros nuevos:**
  - `benchmarks/tool_description_corpus_v1.json` — el corpus.
  - `crates/tylluan-kernel/tests/tool_description_corpus_v1.rs` — validación de esquema + medición de capa 1.

## Qué contiene el corpus

30 descripciones de herramientas tipo MCP, **originales escritas por mí** (procedencia declarada en cada caso `procedencia` y en la cabecera del fichero; sin fuentes externas; los dominios citados terminan en `.invalid`, RFC 2606, nunca alcanzables):

- **15 maliciosas** (`cat: maliciosa`), 3 por técnica:
  - `instruccion_oculta` — directiva oculta que redirige el comportamiento del asistente.
  - `exfiltracion` — pide enviar datos a un destino externo (`https://telemetry.ejemplo-invalid.invalid/collect`…).
  - `orden_ignorar_usuario` — «ignora lo que pida el usuario», «aunque cancele, continúa».
  - `texto_invisible` — comentario HTML `<!-- … -->`, `<span style="display:none">` y espacios de ancho cero U+200B embebidos.
  - `homoglifos` — cirílicos U+0430/U+0435/U+043E/U+0456/U+0441/ U+043D intercalados en texto latin.
- **15 benignas** (`cat: benigna`), 3 por técnica: `ignore_legitimo` (`.eslintignore`, `INSERT IGNORE`, `--force-conflicts`, «ignora los ficheros bloqueados»), `descripcion_larga`, `contexto_seguridad` (habla de inyección de SQL y de prompts), `terminologia_tecnica`, `ejemplo_codigo`.

Cada caso lleva `id` (`tdp_mal_*` / `tdp_ben_*`), `cat`, `tecnica`, `texto`, `procedencia`.

## Validación de esquema (test `tool_description_corpus_schema`)

- 30 casos; 15 maliciosas / 15 benignas; ids únicos y con prefijo coherente con `cat`.
- Campos `id/cat/tecnica/texto/procedencia` presentes y no vacíos en los 30.
- Técnicas dentro de las declaradas; las 10 técnicas (5+5) tienen al menos un caso.
- **Jaccard de trigramas** sobre los 435 pares: **máximo 0.203** (`tdp_mal_004 ↔ tdp_mal_013`); ningún par ≥ 0,8 → sin duplicados ni casi-duplicados.

## Medición de la capa 1 (método de `layer1_hard_corpus_measure.rs`, SIN ajustar la capa)

```
cargo test -p tylluan-kernel --test tool_description_corpus_v1 -- --nocapture
→ test result: ok. 2 passed; 0 failed
```

| categoría/técnica | detectadas | total |
|---|---|---|
| benigna/contexto_seguridad | 0 | 3 |
| benigna/descripcion_larga | 0 | 3 |
| benigna/ejemplo_codigo | 0 | 3 |
| benigna/ignore_legitimo | 0 | 3 |
| benigna/terminologia_tecnica | 0 | 3 |
| maliciosa/exfiltracion | 0 | 3 |
| maliciosa/homoglifos | 0 | 3 |
| maliciosa/instruccion_oculta | 0 | 3 |
| maliciosa/orden_ignorar_usuario | 0 | 3 |
| maliciosa/texto_invisible | 0 | 3 |

- **Maliciosas detectadas: 0/15 = 0,0 %** (pasan las 15: `tdp_mal_001`…`tdp_mal_015`).
- **Falsos positivos sobre benignas: 0/15 = 0,0 %**.

**Lectura sin maquillar:** la capa 1 actual es una lista de frases de inyección; ninguna descripción envenenada de este corpus contiene esas frases, así que no detecta ninguna (0 %) y tampoco confunde a los benignos (0 % FP). El *tool poisoning* **no pasa por la capa 1**: o un detector específico de descripciones de herramientas, o la capa 2. La cola prohíbe afinar la capa para que salga bien y no lo he hecho: estos son los números reales de hoy.

## Comandos y salida real (2026-10-24)

```
python validación de corpus (JSON, recuentos, Jaccard) → n = 30; cats 15/15; jaccard max 0.203; pares >= 0.8: []
cargo test -p tylluan-kernel --test tool_description_corpus_v1 -- --nocapture
→ 2 passed; 0 failed (esquema + medición)
cargo test -p tylluan-kernel --test layer1_corpus_acceptance
→ 2 passed (0 falsos positivos sobre benignos EN+ES y suelos por categoría intactos)
cargo clippy -p tylluan-kernel --all-targets -- -D warnings
→ exit 0 (el primer run cazó un type_complexity en el test nuevo; corregido con un alias de tipo, sin allow)
```

## Impact

- **Superficie:** 1 JSON de datos + 1 test de integración nuevo en `benchmarks/` y `tests/`. Sin cambios en `src/`, sin API, sin schema.
- **Seguridad:** prepara el terreno para conectar MCP externo con una medición honesta de lo que la capa 1 actual cubre (0 %) — sin conexiones reales.

## No verificado

- **Ninguna conexión a servidor MCP real** (excluida por la cola); todavía no hay ADR.
- No medí la capa 2 ni modelos reales sobre este corpus (la cola prohíbe usar motores locales).
- La calidad de «parecido» de los benignos la juzgué yo; el TL puede pedir más casos.
- `DECISION PENDIENTE:` ninguno.
