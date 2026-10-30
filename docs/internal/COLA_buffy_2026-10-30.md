# COLA para Buffy — 2026-10-30 (pruebas y verificación; entrega por relevo del operador)

Lee primero `AGENTS.md`, `STATUS.md`, `REVIEW.md` y `docs/internal/WORK_PROTOCOL.md`. Trabaja solo en tu worktree. Entrega: diff sin commitear más un informe `IMPL_B-9_…` con `## Impact` y apartado "No verificado". Cero datos personales.

Entorno de compilación (obligatorio): `CARGO_PROFILE_DEV_DEBUG=line-tables-only`, `CARGO_INCREMENTAL=0`, `-j 8` y un `CARGO_TARGET_DIR` compartido fuera del checkout (el que te indique el operador). El target antiguo `tylluan-target-buffy` está pendiente de tu confirmación de migración antes de borrarlo: dime si ya no lo usas.

Kernel de prueba solo con puerto y directorio de datos propios (`test-run-*`); nunca el de producción.

## B-9 — Pilar Recordar: la memoria recién guardada debe recuperarse (prioridad nº 1 de la flota)
- Contexto medido el 2026-10-30: se guardó con `tylluan_remember` un resumen de ~3.000 caracteres y el sistema lo **fusionó** (similitud 89%) en un nodo parecido (`handler_remember.rs`, bloque DCR, líneas 213 a 319, umbral 0,87). Después, una consulta casi calcada de su contenido **no lo devolvió** entre los 5 primeros. Causa probable en el código: el DCR compara contra TODOS los nodos (`search_vector(emb, 3)`) sin filtrar tipo ni agente, y añade el texto nuevo al final del viejo (`merge_near_duplicate_content`). Hipótesis NO verificada: el vector del nodo fusionado no representa el texto añadido.
- Hacer, en este orden:
  1. Test de integración que **falle hoy**: con un kernel de prueba, guarda dos textos largos (≥2.000 caracteres) sobre temas distintos pero con vocabulario de proyecto compartido y comprueba (a) que cada uno queda como nodo propio y (b) que una paráfrasis de cada uno lo devuelve en el top 5 de `tylluan_recall`.
  2. Mide para qué longitudes y similitudes se activa la fusión y registra la tabla `longitud -> similitud -> fusionó (sí/no) -> recuperable (sí/no)` con 20 pares.
  3. Propón, sin aplicarlo, el arreglo mínimo (por ejemplo: no fusionar con nodos de otro tipo o procedencia, ni cuando el texto entrante es largo; o crear nodo nuevo enlazado). El operador decide.
- Aceptación: el test del paso 1 fallando hoy con su salida pegada; tabla del paso 2 con los comandos; clippy `-D warnings` = 0.
- Fuera de alcance: cambiar el umbral o el comportamiento de producción sin aprobación.

## B-10 — Simulacro de restauración ("la oscuridad")
- Hacer: con `tylluan-cli backup --dir <carpeta temporal propia>` sobre un kernel de PRUEBA (no el de producción), restaura en otra carpeta con `tylluan-cli restore`, arranca un kernel de prueba con esa carpeta y comprueba que 10 recuerdos escritos antes de la copia se recuperan con `tylluan_recall`. Anota cuánto tarda y cuánto ocupa la copia.
- Aceptación: tabla paso -> comando -> resultado con salida real; qué falló y por qué. Sin tocar el `data/` de producción.

## Si te atascas
Para y publica en Coloquio qué te falta.
