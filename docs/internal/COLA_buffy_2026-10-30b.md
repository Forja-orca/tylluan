# COLA para Buffy — 2026-10-30 (segunda tanda; entrega por relevo del operador)

Mismas reglas que la cola anterior: tu worktree, entorno ligero de compilación con un `CARGO_TARGET_DIR` compartido, kernel de prueba con puerto y datos propios, diff sin commitear más informe `IMPL_B-11_…` con `## Impact` y "No verificado". Cero datos personales.

Contexto: tu B-9 y B-10 han sido decisivos. Causas confirmadas: `update_node_content` no actualiza `nodes_fts` (arreglo en manos de DSH), la política de fusión DCR colapsa notas con la misma plantilla (A/B de Hermes), y la copia de seguridad cubre 5 de 12 bases y el restore solo vuelve "in situ" (arreglo en manos de Hermes). Tus tests que fallan hoy son el contrato que deben cumplir esos arreglos: **déjalos como están** (no los marques `#[ignore]`).

## B-11 — Guardián de cobertura de la copia de seguridad (falla hoy, a propósito)
- Hacer: un test (o script `scripts/` comprobable por CI) que compare las bases `*.db` del directorio de datos de un kernel de prueba con las que contiene el manifiesto de `tylluan-cli backup`, y falle con la lista de las que faltan. Una excepción solo puede entrar en una lista explícita y comentada (por ejemplo, caches regenerables). Hoy debe fallar por las 7 bases que mediste.
- Añade el caso de la identidad: el backup por defecto no incluye `identity.key` ni el secreto de OAuth; el test debe documentar el comportamiento esperado: **incluirlos solo con un indicador explícito** (`--with-secrets`) y avisando de que el destino debe ser de confianza.
- Aceptación: salida real del test fallando con la lista; `clippy -p tylluan-kernel --tests -D warnings` = 0; sin tocar el comportamiento de producción.

## B-12 — Restore a otra carpeta y con otro puerto (contrato)
- Hacer: test de integración que exija que `tylluan-cli restore` acepte un directorio de destino y un puerto propios, y que no se niegue por haber OTRO kernel (el de producción) vivo. Debe fallar hoy con tu salida pegada.
- Aceptación: igual que B-11.

## Pregunta abierta
Tu build coincidió con otro `cargo` en `E:\tylluan-target-buffy`. Dime quién crees que lo usa (nombre del proceso o agente). No se borra nada de ahí hasta que se aclare.

## Si te atascas
Para y publica en Coloquio qué te falta.
