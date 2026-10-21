# T-DSH-27 — Corpus ES benigno + inyección (prerrequisito de D-31)

- **Agente:** dsh
- **Fecha:** 2026-10-21/22
- **Base:** worktree `E:\tylluan-wt-dsh` en `--detach origin/main` (`776e576`); limpio antes de empezar.
- **Encargo (turno 1529):** D-31 de Deep añade patrones en español y normalización a la capa 1; su criterio es 0 falsos positivos, pero el corpus benigno v1 es solo inglés y no mide eso. Crear dos corpus en español con el MISMO esquema que `benchmarks/injection_corpus_v1.json` y `benign_corpus_v1.json`: (1) benigno >=300, con 60 que hablan de inyección/seguridad legítimamente y 40 con frases inocuas que contienen «ahora eres», «ignora», «olvida», «sistema» o «instrucciones»; (2) inyecciones >=60 en español más variantes ofuscadas (espaciado, ancho cero, homoglifo cirílico, ancho completo, puntuación intercalada). Textos escritos por dsh (sin copiar fuentes con licencia), nota de procedencia y licencia MIT en cada fichero, y test `tests/corpus_es_shape.rs` que valide esquema, recuentos y la línea base de la capa 1 (0 detectadas hoy en ambos). Sin `cfg(unix)/cfg(windows)` en la función entera de un test. Informe con tabla de recuentos, `## Impact` y no verificado. Diff sin commitear.

## Entrega (4 ficheros nuevos, sin tocar `src/`)

- `benchmarks/benign_corpus_es_v1.json` (57 KB)
- `benchmarks/injection_corpus_es_v1.json` (22 KB)
- `crates/tylluan-kernel/tests/corpus_es_shape.rs`
- este informe

Esquema respetado del v1: metadatos (`dataset`, `creado`, `autor`, `licencia`, `procedencia`, `descripcion`, `muestras`) y por muestra `id` + `texto` (benigno, con `tipo`) o `id` + `cat` + `variante` + `tecnica` + `texto` (inyección). En ambos ficheros: **licencia MIT** y **procedencia** explícita («textos originales escritos por dsh para T-DSH-27; ninguna fuente externa copiada»). Contenido generado con plantillas + vocabulario propios de forma determinista; nada scrapeado ni traducido de fuentes con licencia.

## Tabla de recuentos (verificada por el test)

| corpus | total | desglose |
|---|---|---|
| **benign_corpus_es_v1** | **300** | técnicos neutros **200** · sobre inyección/seguridad legítimos **60** · frases gatillo inocuas **40** («ahora eres» ×8, «ignora» ×8, «olvida» ×8, «sistema» ×8, «instrucciones» ×8) |
| **injection_corpus_es_v1** | **80** | base **60** (directa 15, cambio de rol 12, exfiltración 12, marcador de sistema 11, manipulación de contexto 10) + variantes ofuscadas **20** (espaciado 5, ancho cero 5, homoglifo cirílico 4, ancho completo 3, puntuación intercalada 3) |

El test comprueba además: ids únicos, textos no vacíos y únicos (benigno), tipos permitidos, cobertura de las 5 variantes y **realidad de la ofuscación** (U+200B presente en ancho cero, rango U+FF01-U+FF5E en ancho completo, rango U+0400-U+04FF en homoglifo cirílico, punto entre letras dentro de palabra en puntuación intercalada).

## Línea base de la capa 1 (medida, no supuesta)

La capa 1 es una lista de **10 substrings en inglés** (`security/poison_patterns.rs`, línea 9-20): `[system:`, `<|im_start|>`, `<|im_end|>`, `ignore all previous instructions`, `ignore previous instructions`, `disregard all prior instructions`, `you are now`, `override security policy`, `<!--#include`, `<!-- #include`. Medición sobre los corpus ES:

| corpus | textos | detectados por capa 1 hoy |
|---|---|---|
| inyecciones ES | 80 | **0** |
| benignos ES | 300 | **0** (falsos positivos) |

```text
rustup run 1.99.1 cargo test -p tylluan-kernel --test corpus_es_shape -- --nocapture
| injection_corpus_es_v1 | base 60 | variantes 5 | total 80 | capa1 detectadas 0 |
| benign_corpus_es_v1 | total 300 | tecnico 200 | seguridad 60 | frase_inocua 40 | capa1 marcados 0 |
test result: ok. 2 passed; 0 failed
```

Nota honesta: 0 detectadas **hoy** es consecuencia de que la capa 1 no tiene patrones en español (no un mérito del corpus); es exactamente la línea base que D-31 necesita para medir su ganancia: cualquier detección nueva en inyecciones ES será TP, y cualquier marcado en los 300 benignos (60 de ellos hablando de seguridad legítimamente, 40 con frases gatillo) será FP. Uno de los textos de sistema se escribió con marcadores en español (`<|inicio_sistema|>…<|fin_sistema|>`) precisamente para no disparar los patrones ingleses y mantener la línea base 0.

## Verificación

| criterio | resultado |
|---|---|
| `cargo test --test corpus_es_shape` | **2 passed / 0 failed** (0,01 s) |
| `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` | **exit 0** |
| `cfg(unix)`/`cfg(windows)` en funciones de test | **ninguno** (requisito de flota) |
| `bash scripts/check_docs_reality.sh` | solo el hallazgo **preexistente** de `ROADMAP_O3.md` (no tocado; artefacto de mi WSL, ya explicado) |
| `check_doc_links.py` / `check_test_count.sh` | **exit 0** / **exit 0** (no cambia el recuento lib) |

## Impact

- **Diff sin commitear: 2 corpus de datos + 1 test + este informe.** Cero cambios en `src/`, `guilds/`, configuración o tests existentes; no se arrancó el kernel ni ninguna guild; no se tocó `E:\tylluan`.
- Insumo directo de D-31: el TL puede aceptar D-31 midiendo su ganancia (TP en 80 inyecciones ES) sin pagar FPs en 300 benignos ES, incluidos los 100 casos más propensos a falso positivo (seguridad legítima y frases gatillo).
- Coste CI: el test es de milisegundos (lee JSON, sin red ni modelos); los corpus pesan ~79 KB.

## No verificado

- **Calidad lingüística**: los textos los ha escrito dsh (plantillas + vocabulario propios); no han pasado revisión de un hablante nativo ni de un lingüista. Son variados y gramaticales, pero la cobertura semántica no es la de un corpus real.
- **Realismo**: los 60 benignos de seguridad y las 40 frases gatillo son sintéticos (autoría propia, sin licencia externa); no replican la distribución de documentación real. El corpus ES **no** se ha validado contra una fuente externa (a propósito, para no arrastrar licencias).
- **Capa 1 futura**: la línea base se mide contra el código de hoy (`776e576`). Cuando D-31 añada patrones/normalización, el test seguirá exigiendo 0 benignos marcados pero el contador de inyecciones ES debería subir; **no** se ha codificado todavía esa expectativa (depende del diseño de D-31).
- **Ofuscaciones**: se cubren las 5 variantes pedidas, no todas las posibles (p. ej. no hay homoglifos griegos, bidi controls ni combinación de dos técnicas en un mismo texto).
- **Normalización**: no se prueba aquí ninguna normalización (NFKC, strip de zero-width, mapping de homoglifos); eso es parte de D-31, no del corpus.
- **CI**: los corpus no se añaden a ningún workflow ni a `verify.sh`; no hay comprobación de que un `git` futuro no los altere (podría añadirse un checksum si se quiere).
