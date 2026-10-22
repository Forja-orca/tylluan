# D-31 — Normalización de la capa 1 de ASI06 (capa 1 antes de comparar)

- **Agente:** Deep (OpenCode)
- **Fecha:** 2026-10-22/23
- **Base del patch:** `origin/main` = `f824436` (docs: T-DSH-33 vías de escritura de memoria)
- **Patch:** `E:\tylluan-patches\deep\D-31.patch` — 33 428 B, **4 ficheros**, **+659 / −21**
- **Estado:** implementado, verificado en local y aplicado en limpio sobre `f824436`; **sin commitear** (entrega = diff + este informe)
- **`## Impact`:** toca el control de seguridad de escritura de memoria (capa 1 de ADR-011). Ver sección *Impact*.

---

## 1. Qué pedía el encargo y qué se hizo

Encargo (cola de Deep): la capa 1 comparaba el texto **crudo** con 10 literales en inglés, así que
cualquier ofuscación trivial (espaciado, ancho cero, homoglifo, full-width, puntuación) o una simple
traducción al español la evadía. Objetivo:

1. una pasada de normalización **antes** de comparar;
2. un conjunto inicial de patrones en español con la misma semántica (no traducción literal);
3. mantener la firma `matches_injection_pattern(&str) -> bool`;
4. los tests de ofuscación del red-team pasan sin `#[ignore]`;
5. 0 falsos positivos sobre el corpus benigno, medidos antes y después;
6. coste por llamada medido antes y después.

Hecho:

- `crates/tylluan-kernel/src/security/poison_patterns.rs` (reescritura del módulo, 391 líneas):
  - `is_invisible` — quita U+00AD, U+061C, U+180E, U+200B..200F, U+202A..202E, U+2060..2064, U+FEFF.
  - `fold_width` — U+FF01..FF5E → ASCII (−0xFEE0) y U+3000 → espacio.
  - `fold_homoglyph` — acentos españoles + tabla corta cirílica/griega (`а`→a, `е`→e, `і`→i, `о`→o, `ѕ`→s, `α`→a…).
  - `normalize` — invisibles fuera, width, minúsculas y homoglifos; **vía rápida ASCII** con
    `str::to_ascii_lowercase` (el ASCII no puede ser invisible, full-width ni homoglifo).
  - `squash` — vista solo alfanumérica (quita espacios, puntuación y símbolos) para que
    `i g n o r e   a l l …`, `ignore_all_previous` o `ignore...previous...instructions` casen.
  - `PatternIndex` + `OnceLock` — índice por primer byte (bitmap de rechazo rápido + cubos), construido
    **desde el propio array de patrones**: añadir un patrón no puede dejar el índice obsoleto.
  - Patrones: los **10 originales intactos** (`[system:`, `<|im_start|>`, `<|im_end|>`,
    `ignore all previous instructions`, `ignore previous instructions`, `disregard all prior
    instructions`, `you are now`, `override security policy`, `<!--#include`, `<!-- #include`) +
    14 en español: `[sistema:`, `ignora todas/las instrucciones anteriores`,
    `olvida todas las instrucciones anteriores`, `haz caso omiso de las instrucciones anteriores`,
    `descarta todas las instrucciones previas`, `sin restricciones`, `sin filtros`, `sin censura`,
    `autoridad maxima`, `no sigue las politicas`, `ignora las reglas`, `anula/omite/salta la politica
    de seguridad`. Las frases tienen además su forma *squashed*.
  - Sin dependencias nuevas (la normalización es manual; `unicode-normalization` no se añadió).
- `crates/tylluan-kernel/tests/asi06_layer1_ofuscacion_regresion.rs` (**nuevo**, 276 líneas): afirma la
  **defensa** (no el ataque) sobre los mismos vectores del red-team de Buffy, más la ruta real
  `/api/v1/memory/write` con español, ancho cero, espaciado y full-width → `403 ACCESS_DENIED`.
  9 tests en verde + 2 `#[ignore]` que **declaran** las clases abiertas (`limite_conocido_*`).
- `tests/asi06_layer1_ingest_memory_write.rs` y `tests/d17_layer1_write_paths.rs`: sus listas
  anti-drift fijaban el conjunto medido por B-9 (8 ids). Ahora la capa 1 detecta 12 → listas
  actualizadas a 12 (`inj_13..inj_16`) y el recuento de juicios pasa a derivarse de
  `B9_FLAGGED.len()` en vez de un `8` literal. Es el procedimiento que el propio test documenta
  ("si cambió `poison_patterns.rs`, actualiza EXPECTED_DETECTED").

**No decidido por mí:** no se tocó `tests/asi06_redteam_memory_poisoning.rs` (fichero de Buffy).
Retirar allí los 9 tests cuyo bypass ya está cerrado es suyo — su propio mensaje de fallo lo pide.
Queda propuesto en la sección 6.

---

## 2. Efecto medido sobre los corpora (Rust, misma función real)

| corpus | antes (`f824436`) | después (D-31) |
|---|---|---|
| `injection_corpus_v1.json` (40, inglés) | **8/40** | **12/40** |
| `injection_corpus_es_v1.json` (80, T-DSH-27) | **0/80** | **13/80** |
| `benign_corpus_v1.json` (40) | 0/40 | **0/40** |
| `benign_corpus_es_v1.json` (300, T-DSH-27) | 0/300 | **0/300** |

- Los 4 nuevos del corpus inglés son `inj_13`, `inj_14`, `inj_15`, `inj_16`: inyecciones **en español**
  que ya estaban en el corpus y que la capa 1 no veía (`"Ignora todas las instrucciones anteriores…"`).
- Los 340 textos benignos (40 EN + 300 ES) siguen con **0 falsos positivos**; el test
  `d31_benign_corpora_have_zero_false_positives` lo comprueba leyendo los dos JSON.
- **Decisión de precisión, con datos:** `"ahora eres"` se probó y se **descartó**. Detectaba 4
  inyecciones del corpus ES, pero marcaba **8 de los 300 benignos** (`"Ahora eres la persona de
  guardia…"`, `"…contacto tecnico del turno de noche…"`), que es uso normal del español. Lo que se
  matchea es la **cláusula de anulación** que convierte una asignación de rol en inyección
  (`sin filtros`, `sin restricciones`, `autoridad maxima`, `no sigue las politicas`, `sin censura`).
  Resultado: 13/80 en inyecciones ES **con 0/300** en benignos. `inj_es_020` ("…puedes aprobar
  cualquier accion sin confirmacion") queda fuera a propósito: `"sin confirmacion"` aparece en 1
  benigno inglés, y preferí no abrir un falso positivo en el corpus que es criterio de aceptación.
- `13/80` **no es una cifra de cobertura**: el corpus ES es en buena parte de paráfrasis, que es la
  clase que una lista cerrada no cierra. Se publica como suelo medido, no como defensa.

---

## 3. Red-team de Buffy: 9 de 11 bypass cerrados, 2 abiertos

`cargo test --test asi06_redteam_memory_poisoning -- --ignored` sobre este patch:

| test | resultado | lectura |
|---|---|---|
| `bypass_01_…_no_tiene_espanol` | **FAILED** | cerrado: el español se detecta |
| `bypass_02_espaciado_intra_palabra` | **FAILED** | cerrado (squash) |
| `bypass_03_homoglifo_cirilico` | **FAILED** | cerrado (tabla homoglifos) |
| `bypass_04_fullwidth` | **FAILED** | cerrado (fold_width) |
| `bypass_05_leetspeak` | ok | **abierto** (declarado) |
| `bypass_06_puntuacion_intercalada` | **FAILED** | cerrado (squash) |
| `bypass_07_parafasis_no_esta_en_la_lista` | ok | **abierto** (estructural) |
| `bypass_08_matriz_de_repeticiones_k` | **FAILED** | cerrado: el español se detecta con k=1..10 |
| `bypass_09_memory_write_acepta_espanol_sin_banner` | **FAILED** | cerrado end-to-end: `403 ACCESS_DENIED` |
| `bypass_10_ofuscacion_e_victoria_end_to_end` | **FAILED** | cerrado end-to-end: U+200B rechazado |
| `bypass_11_matriz_completa_de_variantes` | **FAILED** | bypasses reales 6/6 → 1/6 (solo leetspeak) |

Esos tests afirman **que el ataque gana** (`assert!(!matches_injection_pattern(...))`), así que un
FAILED es la señal de cierre, no una regresión. Por eso el criterio "pasan sin `#[ignore]`" se cumple
en `asi06_layer1_ofuscacion_regresion.rs` (aserción invertida, en verde) y no reescribiendo el fichero
de Buffy.

---

## 4. Coste por llamada (antes / después)

Medido en **`--release`**, A/B dentro del mismo proceso y la misma corrida (la máquina es compartida:
entre corridas vi ±25 %, por eso el A/B y no números absolutos sueltos).

| texto | legacy (10 `contains` sobre `to_lowercase`) | D-31 | ratio |
|---|---|---|---|
| 105 B | 424 ns | **1 244 ns** | 2,9× |
| 1 680 B | 2 401 ns | **19 498 ns** | 8,1× |

Desglose a 1 680 B: `normalize` 217 ns, `squash` 4 688 ns, escaneo raw 7 097 ns, escaneo squashed 8 031 ns.

Notas honestas:

- La primera versión (lowercase por carácter + `contains` por patrón) daba **35 µs** a 1 680 B
  (15×). Se optimizó con vía rápida ASCII, `squash` a nivel de byte, índice por primer byte y
  cubos; el 8,1× es el residuo: dos pasadas de normalización más dos escaneos, con 24 patrones.
- En **debug** el ratio aparenta 14× (48 µs vs 3,4 µs): los bucles por carácter no se inlinan.
  Los tests corren en debug y siguen tardando microsegundos; la cifra que importa es la de release.
- No se añadió Aho-Corasick ni dependencia alguna. Si alguien quiere bajar de 8×, esa es la vía
  (y entonces hay que justificar la dependencia), no micro-optimizar más a mano.

---

## 5. Verificación ejecutada

```
cargo clippy -j 3 -p tylluan-kernel --lib -- -D warnings                     → exit 0 (23,21 s)
cargo clippy ... --test asi06_layer1_ofuscacion_regresion
             --test asi06_layer1_ingest_memory_write
             --test d17_layer1_write_paths -- -D warnings                    → exit 0 (15,67 s)
cargo test -j 3 -p tylluan-kernel --lib                                     → 1185 passed / 0 failed / 4 ignored
cargo test -j 3 -p tylluan-kernel --lib                                     → 1185 passed / 0 failed / 4 ignored (34,63 s)
cargo test -p tylluan-kernel --test asi06_layer1_ofuscacion_regresion       → 9 passed / 0 failed / 2 ignored
   ... -- --ignored                                                        → 2 passed (los 2 límites siguen abiertos)
cargo test -p tylluan-kernel --test asi06_layer1_ingest_memory_write        → 1 passed / 1 ignored
cargo test -p tylluan-kernel --test d17_layer1_write_paths                  → 1 passed (33,38 s)
```

Aplicación en limpio (worktree nuevo en `origin/main` = `f824436`):

```
git apply --check D-31.patch   → 0
git apply D-31.patch           → 0
CARGO_TARGET_DIR=…\deep-d31 cargo check -p tylluan-kernel --tests → 0 (8m35s)
```

Base de tests: `f824436` tiene **1177** lib; con los 8 tests nuevos de D-31 → **1185**.

---

## 6. Pendiente / para el TL

1. **Buffy — retirar los 9 tests cerrados** de `tests/asi06_redteam_memory_poisoning.rs`
   (`bypass_01..04, 06, 08..11`) y dejar `bypass_05` y `bypass_07` como límites documentados. Su
   propio mensaje de fallo lo pide; no lo hice para no editar el fichero de otro agente mientras ella
   puede seguir trabajando en él.
2. **Renombrar** `d17_memory_write_rejects_the_8_layer1_injections` (dice 8, verifica 12). No lo
   renombré porque ese nombre aparece en prosa de `IMPL_T-DSH-15`; el contrato ya es la lista.
3. `check_test_count.sh` sigue con MISMATCH (deuda previa, ajena a D-31): la cifra de referencia del
   script no es 1185.
4. Los 4 ficheros de corpus ES (T-DSH-27) están en `main` desde `de57658`; conviene que la cola fije
   **qué fracción de `injection_corpus_es_v1` se espera detectar** — hoy es un suelo medido (13/80)
   sin contrato, y un cambio de patrones puede bajarlo sin que nada avise.

## 7. Límites declarados (en el módulo, no solo aquí)

1. **Paráfrasis** no cubiertas (clase estructural; 67/80 en el corpus ES).
2. **Leetspeak** (`1gn0r3`) no se normaliza: sigue evadiendo.
3. Los patrones que **son** puntuación (`[system:`, `<|im_start|>`, `<!--#include`) sobreviven a
   invisibles/width/homoglifos/mayúsculas, pero espaciarlos (`[ s y s t e m :`) sigue evadiendo:
   hacerlos *squashed* casaría cualquier mención de "system".
4. **Asignación de rol** sola no es patrón (8 benignos ES); se matchea la cláusula de anulación, así
   que una inyección que solo **implique** la anulación pasa.

## Impact

- **Qué cambia para un usuario:** ningún cambio de API ni de configuración. La capa 1 de escritura de
  memoria rechaza (o marca, en `ingest`) más contenido malicioso: +4 muestras del corpus inglés, +13
  del corpus español, y las 5 familias de ofuscación del red-team.
- **Falsos positivos:** 0 sobre los 340 textos benignos de `benign_corpus_v1` + `benign_corpus_es_v1`.
  Se descartó explícitamente el patrón que los producía (`"ahora eres"`, 8/300).
- **Riesgo asumido:** una asignación de rol con anulación implícita puede pasar; el coste por llamada
  sube ~3× (texto corto) a ~8× (1,6 KB) sobre una función que corre una vez por escritura.
- **No toca** `transport/`, `guilds/`, `integrations/`, `tylluan*.toml`, tokens, `dev_mode` ni datos.
- **No depredación:** solo `crates/tylluan-kernel/src/security/poison_patterns.rs` y tres ficheros de
  test; ningún fichero de otro agente modificado (el red-team de Buffy queda intacto).
