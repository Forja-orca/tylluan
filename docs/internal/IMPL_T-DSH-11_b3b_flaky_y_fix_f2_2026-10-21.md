# T-DSH-11 — B3b (¿flaky?) y fix mínimo de F2 (banner en `tylluan_think`)

- **Agente:** dsh
- **Fecha:** 2026-10-21
- **Base:** `9d584cc` == origin/main (push 82, T-DSH-10 integrada tal cual; el test de la matriz es el integrado).
- **Encargo (turno 1468):** (1) correr 5 veces el test con `--ignored` contra origin/main y entregar tabla caso × corrida para **B3b** (el TL no lo reprodujo en 2 corridas: flaky, orden o carrera con el auto-bootstrap); (2) fix mínimo de **F2** (banner en `tylluan_think` vía `trust::mark_text`) como diff sin commitear con test que pase A5, solo `handler_think.rs`.

## (1) B3b: tabla caso × corrida

Comando (mismo binario de test integrado, 5 corridas consecutivas sobre `9d584cc`):

```text
rustup run 1.99.1 cargo test -p tylluan-kernel \
  --test trust_banners_adversarial -- --ignored --nocapture   # x5
```

| caso | run 1 | run 2 | run 3 | run 4 | run 5 |
|---|---|---|---|---|---|
| A4 @inbox legacy | QUEBRADO | QUEBRADO | QUEBRADO | QUEBRADO | QUEBRADO |
| A5 think sin banner | QUEBRADO | QUEBRADO | QUEBRADO | QUEBRADO | QUEBRADO |
| B2 externo auto-registrado | QUEBRADO | QUEBRADO | QUEBRADO | QUEBRADO | QUEBRADO |
| **B3b `Deep` tras identidad `Deep`** | **QUEBRADO** (banner=false) | **QUEBRADO** | **QUEBRADO** | **QUEBRADO** | **QUEBRADO** |
| C2 herencia retroactiva | QUEBRADO | QUEBRADO | QUEBRADO | QUEBRADO | QUEBRADO |
| C1 / A1 / A2 / A3 / B1 / B3 / B4 | OK | OK | OK | OK | OK |

Cada corrida: `5 quebrados de 12`, exit 101; las 7 filas de control, estables.

**Diagnóstico.** En esta máquina B3b es determinista (5/5), pero **no por lógica: por carrera**. La secuencia del caso es:

1. llamada con `agent_id="deep"` → el auto-bootstrap de identidad (`handlers.rs` líneas 55-68) lanza un `tokio::spawn` que crea `agent:deep`;
2. `mailbox_send` desde `"Deep"` → `sender_is_local = get_identity("Deep").is_some()` = false → marcado (B3 OK);
3. llamada con `agent_id="Deep"` → otro spawn crea `agent:Deep`;
4. `mailbox_send` desde `"Deep"` → si `agent:Deep` **ya existe**, local (banner=false: B3b QUEBRADO); si el spawn **aún no ha corrido**, externo (banner=true: no se reproduce).

El test espera 400 ms tras la llamada 3, que en esta máquina basta; en las 2 corridas del TL no bastó (o el spawn no llegó antes del envío) → por eso no lo reprodujo. **Prueba adicional:** tras aplicar el fix de F2 (rebuild del binario, otro perfil de tiempos) la misma matriz dio **B3b = OK** con A5 también OK (`3 quebrados de 12`): el veredicto de B3b cambió sin tocar nada del buzón — confirma que es una carrera, no una propiedad estable.

**Conclusión del punto (1):** B3b es **flaky por diseño del test y por diseño del sistema**: la localidad depende de que un `tokio::spawn` haya completado. El vector de seguridad determinista es **B2** (registro explícito de identidad → local, 5/5 y reproducible sin carrera). Recomendación: la localidad no puede depender de una carrera; o se decide con roster/binding (determinista) o la identidad se crea de forma síncrona antes de decidir. Anotado como hallazgo, no se cambia el test: el test documenta el comportamiento observable.

## (2) Fix mínimo de F2: banner en `tylluan_think`

Diff sin commitear, limitado a **`crates/tylluan-kernel/src/transport/server/handler_think.rs`** (el test va en fichero aparte):

- Nuevo helper privado `trust_text(node, text)`: si `security::trust::node_is_untrusted(node)`, antepone `trust::mark_text(text, origin_of(metadata, "memoria marcada"))`; si no, devuelve el texto igual.
- Aplicado a las **cuatro** emisiones de texto de nodo de la síntesis: experiencia previa (líneas ~88), conocimiento SilvaDB (líneas ~108-114), expansión Chain-of-Thought (~136) y resumen de clúster GraphRAG (~213).
- Sin cambios de formato del resto de la respuesta, sin claves nuevas, sin tocar el gate ni el ranking. `related_mail` (payload de buzón, no nodo Silva) queda fuera del fix mínimo: su marca vive en `BlackboardMessage.untrusted`, no en metadata — anotado abajo como no cubierto.

**Verificación:**

| check | resultado |
|---|---|
| Test focalizado nuevo `crates/tylluan-kernel/tests/trust_think_banner.rs` (no ignorado) | **1 passed** |
| Matriz `--ignored` post-fix | A5 → **OK** (`3 quebrados de 12`: quedan A4, B2, C2) |
| `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` | **exit 0** |
| `cargo test -p tylluan-kernel --lib` | **1141 passed; 0 failed; 4 ignored** (recuento intacto) |

El test focalizado comprueba: el nodo marcado sale con banner **delante** del contenido; el nodo limpio no lo lleva; el banner aparece exactamente una vez. Es la especificación ejecutable de A5 (y falla sin el fix).

## Impact

- **Cambio de código en un solo archivo** (`handler_think.rs`, helper + 4 llamadas): no toca el gate, el ranking, el formato de la respuesta ni otros handlers. `## Impact` (ADR-013): sin cambios de comportamiento fuera de añadir el banner a contenido ya marcado; ningún nodo limpio cambia.
- **Test nuevo de integración** (`trust_think_banner.rs`) fuera del recuento de lib tests (1141 intacto); la matriz adversarial sigue `#[ignore]` y no bloquea CI.
- Cero escrituras en producción: estado de test en memoria, audit en temporal, kernel vivo no tocado.
- Sin secretos: contenido sintético.

## No verificado

- **Por qué exactamente las 2 corridas del TL no reprodujeron B3b**: hipótesis de carrera con el spawn (respaldada por el cambio de veredicto tras el rebuild), pero no instrumenté el orden de los spawns con logs.
- **`related_mail` en `think`**: el payload de un mensaje de buzón no se marca con el banner (no es nodo Silva). Queda fuera del fix mínimo; si se quiere cubrir, debe leerse `BlackboardMessage.untrusted` al proyectar la actividad reciente.
- **A4/B2/C2**: siguen abiertos (como reprodujo el TL); este diff no los toca.
- El test focalizado usa el router real pero no un kernel en vivo (regla de flota: no se toca producción).
