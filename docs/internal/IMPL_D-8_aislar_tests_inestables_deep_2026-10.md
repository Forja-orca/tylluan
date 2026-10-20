# D-8 — Aislar los tests que hacen inestable la suite en paralelo (Deep, 2026-10-20)

**Estado: no verificado** por terceros. Diff **sin commitear** en
`E:\tylluan-wt-deep`, base `origin/main` = `cad1477`.
Parche: `E:\tylluan-patches\deep\D-8.patch` (3 archivos, sin cambios de recuento:
no añade ni quita tests).

## Criterio de aceptación, cumplido y pegado

Cinco pasadas consecutivas de `cargo test -p tylluan-kernel --lib` EN PARALELO,
misma máquina, con el kernel de producción corriendo:

```
RUN 1 | EXIT 0 | test result: ok. 1108 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 30.36s
RUN 2 | EXIT 0 | test result: ok. 1108 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 31.34s
RUN 3 | EXIT 0 | test result: ok. 1108 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 29.82s
RUN 4 | EXIT 0 | test result: ok. 1108 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 30.29s
RUN 5 | EXIT 0 | test result: ok. 1108 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 50.53s
```

Y cuatro más bajo carga real (dos suites completas concurrentes, que es más
duro que una sola pasada: las de arriba tardan ~30 s, estas ~51 s porque se
pelean por CPU):

```
STRESS 1 | A | ok. 1108 passed; 0 failed; 4 ignored | 51.50s
STRESS 1 | B | ok. 1108 passed; 0 failed; 4 ignored | 49.65s
STRESS 2 | A | ok. 1108 passed; 0 failed; 4 ignored | 52.71s
STRESS 2 | B | ok. 1108 passed; 0 failed; 4 ignored | 51.48s
```

Antes de este cambio, cuatro pasadas normales produjeron cuatro fallos distintos
en esta misma máquina (los nombres están en el informe de D-5).

## Qué se cambió y por qué (causa raíz, no "esperar más")

### 1. `inference_budget::condvar_waiter_does_not_poll_while_parked`

El test asumía que, 50 ms después de que el hilo waiter pusiera su bandera, ya
estaba aparcado en el `Condvar`. Pero la bandera (`entered`) se pone **antes**
de llamar a `acquire_sync`, así que bajo carga el hilo puede estar desprogramado
justo ahí: el contador vale 1 y el `assert_eq!(…, 2)` revienta. No era el
mecanismo el que fallaba, era la premisa temporal del test.

Ahora:

- se espera (con deadline de 5 s) a que el contador llegue a 2, es decir, a que
  **ambos** caminos rápidos se hayan intentado — cuando está en 2 y no crece, el
  waiter está aparcado, sin depender de cuánto tarde el scheduler;
- se observa una ventana de 100 ms: el contador no puede crecer más de 1
  (una reactivación espuria puede reintentar una vez; un busy-wait de 5 ms
  añadiría ~20);
- la aserción posterior a `give()` pasa de `== 2` a "como mucho un reintento más
  que al final de la ventana", que es lo que de verdad hace el camino de
  despertar.

### 2. `handler_do::audit::audit_store_seam_write_and_read_land_on_same_file`

La propiedad que el test debe fijar es **"el escritor y el lector resuelven al
mismo fichero"**, no "esa base de datos tiene exactamente una fila". Mientras
`TYLLUAN_AUDIT_DB` está puesto, la variable es de proceso: cualquier otro test
del binario que registre una entrada de auditoría escribe en ese mismo SQLite.
En la pasada que falló, la consulta devolvió 3 filas (la nuestra + dos de tests
concurrentes) y el `assert_eq!(rows.len(), 1)` la tumbó. La guarda que ya había
solo excluye a un segundo *usuario del seam*, no a esos escritores.

Ahora se comprueba que **la fila escrita está presente** para el lector. La
propiedad del seam sigue verificada; el recuento de filas ya no se usa como
prueba de algo que el test no controla.

Nota honesta sobre lo que esto **no** hace: el test sigue usando `set_var`
(variable de proceso) porque el seam por entorno es una funcionalidad real de
despliegue y merece su test. Aislarlo del todo pediría inyectar la ruta por
estado interno solo para tests, y entonces se dejaría de verificar el seam real.
El efecto secundario que sí queda es que los tests siguen escribiendo en
`./data/audit.db` del directorio de trabajo (aunque el seam apunte a temporal);
no afecta a este test, pero es la siguiente cosa a limpiar si alguien quiere un
`data/` inmaculado tras correr la suite.

### 3. Los dos tests de lecciones

Ambos verificaban un efecto escrito por una **tarea spawned** tras un `sleep`
fijo (150 ms y 300 ms). Bajo la suite completa ese plazo no se cumple: el
despacho previo puede quedarse esperando el mutex de ONNX que otros tests tienen
tomado. Sustituidos por sondeo con deadline de 10 s (cada 20-25 ms). No relaja
la aserción: el caso normal sigue resolviendo en la primera o segunda vuelta;
solo deja de suspender por hambre de CPU.

El tercer test de lecciones (`test_lesson_penalty_on_failure`) ya sondeaba, pero
con deadline de 2 s — también insuficiente bajo carga; subido a 10 s, mismo
motivo.

## Verificación

```
cargo test -p tylluan-kernel --lib lesson                 → 11 passed
cargo test -p tylluan-kernel --lib audit_store_seam       → 1 passed
cargo test -p tylluan-kernel --lib condvar_waiter         → 1 passed
5 × cargo test -p tylluan-kernel --lib (paralelo)         → 5/5 verdes (pegadas arriba)
2 × dos suites concurrentes                               → 4/4 verdes (pegadas arriba)
cargo clippy -p tylluan-kernel --all-targets -- -D warnings → EXIT 0
```

## Impact

- **¿Depreda a otro agente?** No: solo toca tests, ningún path de producción.
- **¿Depreda a un usuario?** No: cero cambios en runtime. La única pérdida es
  precisión de aserción en el test del condvar (de "exactamente 2" a "no crece
  mientras está aparcado"), que es precisamente lo que se quería medir.
- **¿Coste?** Los tres tests de lecciones pueden tardar hasta 10 s en el peor
  caso; en la práctica < 100 ms. La suite completa no se alarga (30 s medidos).
- **¿Se puede verificar sin mí?** Sí: los cinco comandos de arriba, en esta
  máquina y con el kernel de producción encendido.
