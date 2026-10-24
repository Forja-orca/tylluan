# IMPL — M-BUFFY-5: invertir los tests de red-team que fijaban el comportamiento vulnerable

- **Agente:** Buffy · **Fecha:** 2026-10-24
- **Cola:** `docs/internal/COLA_buffy_2026-10-24_noche.md` (M-BUFFY-5)
- **Fichero tocado:** `crates/tylluan-kernel/tests/asi06_redteam_memory_poisoning.rs` — solo tests; `src/` intacto.
- **Base:** `d2e71cf` (origin/main tras `agent_wt.sh reset buffy`), con D-31 y D-32 ya integrados.

## Qué medí antes de tocar nada

Corrí la suite tal cual estaba, en su modo demo:

```
cargo test -p tylluan-kernel --test asi06_redteam_memory_poisoning -- --ignored --nocapture
→ test result: FAILED. 2 passed; 9 failed
```

Es decir: **9 de los 11 tests que afirmaban «este bypass funciona» FALLABAN** — el hueco ya estaba cerrado en el código actual (D-31 normalización + patrones ES; D-32 en las vías de escritura). Los 2 que pasaban documentaban huecos que siguen abiertos: `bypass_05` (leetspeak) y `bypass_07` (paráfrasis).

## Tabla antes/después (cada test, qué cubre y su vía)

| Test (nombre actual) | Nombre anterior | Cubre (variante) | Vía | Antes | Después |
|---|---|---|---|---|---|
| `capa1_detecta_espanol` | `bypass_01_la_lista_de_patrones_no_tiene_espanol` | instrucción en español | unitario → vías 1+2 | `#[ignore]`, `assert!(!matches(BASE_ES))` | **verde**, `assert!(matches(BASE_ES))` |
| `bypass_02_espaciado_intra_palabra` | igual | v1 espaciado | unitario → vías 1+2 | `#[ignore]`, evasión afirmada | **verde**, detección afirmada |
| `bypass_03_homoglifo_cirilico` | igual | v3 homoglifo cirílico | unitario → vías 1+2 | `#[ignore]`, evasión afirmada | **verde**, detección afirmada |
| `bypass_04_fullwidth` | igual | v4 full-width | unitario → vías 1+2 | `#[ignore]`, evasión afirmada | **verde**, detección afirmada |
| `bypass_05_leetspeak` | igual | v5 leetspeak | unitario | `#[ignore]` (pasaba) | **`#[ignore]`** — residual ABIERTO, razón actualizada con fecha de medición |
| `bypass_06_puntuacion_intercalada` | igual | v7 puntuación | unitario → vías 1+2 | `#[ignore]`, evasión afirmada | **verde**, detección afirmada |
| `bypass_07_parafasis_no_esta_en_la_lista` | igual | paráfrasis (clase) | unitario | `#[ignore]` (pasaba) | **`#[ignore]`** — residual ABIERTO, razón actualizada |
| `bypass_08_matriz_de_repeticiones_k` | igual | k = 1..10, EN/ES | unitario → vías 1+2 | `#[ignore]`, «el español NUNCA se detecta» | **verde**, español detectado en todo k; la columna de paráfrasis solo se imprime, no se afirma |
| `memory_write_rechaza_espanol` | `bypass_09_memory_write_acepta_espanol_sin_banner` | español end-to-end | **vía 1** `/api/v1/memory/write` | `#[ignore]`, esperaba 200 sin marca | **verde**, espera **403 ACCESS_DENIED + sin nodo escrito** |
| `memory_write_rechaza_ancho_cero` | `bypass_10_ofuscacion_e_victoria_end_to_end` | U+200B end-to-end | **vía 1** `/api/v1/memory/write` | `#[ignore]`, esperaba 200 sin marca | **verde**, espera **403 + sin nodo escrito** (el control literal ya esperaba 403) |
| `bypass_11_matriz_completa_de_variantes` | igual | matriz 6 familias no-control | unitario | `#[ignore]`, `n_bypass >= 5` | **`#[ignore]`** — aserción ajustada al estado medido: solo `v5_leetspeak` evade (1/6); razón escrita |

Renombré los 3 tests cuyo nombre afirmaba la vulnerabilidad (01, 09, 10); los demás nombres describen la técnica y se mantienen.

## Comandos y salida real (2026-10-24)

```
# suite normal (después de la revisión)
cargo test -p tylluan-kernel --test asi06_redteam_memory_poisoning
→ test result: ok. 8 passed; 0 failed; 3 ignored; finished in 1.51s

# residuo documentado
cargo test -p tylluan-kernel --test asi06_redteam_memory_poisoning -- --ignored
→ test result: ok. 3 passed; 0 failed; 0 ignored; finished in 0.01s

# gate de CI
cargo clippy -p tylluan-kernel --all-targets -- -D warnings
→ exit 0 (CLIPPY=0)
```

Los 8 tests en verde afirman que el ataque se bloquea: **ningún test pasa en verde afirmando una vulnerabilidad**. Los 3 `#[ignore]` llevan la razón escrita en el propio atributo con la fecha de medición; si el residuo se cierra, su aserción indica que se inviertan.

## Impact

- **Superficie:** un único fichero de tests de integración. Sin cambios en `src/`, sin API, sin schema, sin comportamiento de producción.
- **Seguridad:** convierte la suite en red de regresión real de D-31/D-32 sobre las vías 1 y 2, y deja documentado por código el residuo medido (leetspeak, paráfrasis).

## No verificado

- Vías 3-6 (`ingest/upload`, coloquio, grafo `tylluan_graph`, salida de guild): inventariadas en la cabecera, **no atacadas ni modificadas**.
- No ejecuté la suite lib completa (1245) ni `scripts/verify.sh`: este cambio no toca `src/`; el TL los corre en el pre-push.
- La capa 2 (juez LLM) no entra en estos tests.
- `DECISION PENDIENTE:` ninguno.
