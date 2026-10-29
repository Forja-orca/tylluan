# Plan de cierre a producción (2026-10-29)

> Lo escribe el TL por petición de José: nada de lo que está a medias se queda olvidado. Cada meta tiene dueño, criterio medible y su estado verificado el 2026-10-29. Se revisa en cada ciclo del TL.

| Meta | Qué cierra | Dueños | Criterio de cierre (comando y salida esperada) | Estado verificado |
|---|---|---|---|---|
| G1 Seguridad: `dev_mode=false` | B-1/B-2, subida de Vision, estado vacío del panel, prueba conjunta | Deep (D-68/69/70 hecho), Antigravity (AG-13 hecho, AG-14), Buffy (B-5), decide José | kernel de prueba con `dev_mode=false`: 0 rutas del panel rechazan un token válido y un JWT sin mapeo recibe 403 | hecho: D-68/69/70 y AG-13; falta AG-14 y B-5 |
| G2 Release siguiente | todo lo posterior a v0.17.2 | TL | etiqueta; `checksums.txt` y atestación en la release; puerta de versión en verde | publicable cuando G1 cierre; `main` lleva muchos commits más que producción |
| G3 Cara pública | README nuevo (EN/ES), chino, visto bueno | TL, Hermes, Deep (D-61), decide José | la rama `readme-pro-2026-10-25` rebasada sobre `main`, puertas de docs en verde | sin integrar en `main` |
| G4 Tests y CI | auditar los 35 `#[ignore]` (33 tras D-69), suite repetida 10 veces, test del dashboard bajo carga | Buffy (B-6, B-7), DSH | tabla test -> motivo -> acción; 0 tests intermitentes sin explicar | 33 atributos `#[ignore]` reales (`grep -rn --include=*.rs -E "^[[:space:]]*#[[]ignore" crates/ | wc -l`) |
| G5 Memoria y recuperación | cobertura de embeddings, contadores del filtro de huella, validar `coherence_semantic_scope` | DSH (T-DSH-139), Hermes (H-52, H-53) | A/B de no inferioridad (Recall@5, 40 preguntas) antes de aplicar nada | simulación en curso |
| G6 Runtime (ADR-025, aprobado) | registro de tareas, vista de solo lectura `/api/v1/runtime/tasks`, reindexer como primera tarea | DSH (T-DSH-140) | clippy `-D warnings` 0; tests nuevos; sin herramienta MCP nueva (CONTRACT-01) | apagado hecho (T-DSH-130/132); registro y vista sin hacer |
| G7 Dashboard | literales en español, capturas vivas, panel embebido en las releases | Antigravity (AG-14, AG-15), decide José lo embebido | capturas contra un kernel real; `tsc` 0; `vitest` en verde | AG-13 hecho |
| G8 Higiene y privacidad | capa de log de auditoría, inventario de memoria en el repo público, informes antiguos sin integrar | Hermes (H-54, H-55), TL | tabla fichero -> tipo de dato -> propuesta; nada se mueve sin permiso de José | informes pendientes en worktrees de DSH y Antigravity |
| G9 Producción | reiniciar con todo lo integrado | José lanza, TL prepara | `/health` devuelve el hash de `main` | producción en `d4f2537` |
| G10 Servidor verde | medir y reducir el consumo sin perder calidad | DSH (T-DSH-141), Qwen (investigación), TL | RAM y CPU en reposo y con carga medidos en un kernel de prueba; cualquier recorte pasa A/B de no inferioridad antes de aplicarse | producción: 5 227 MB de RAM, 90 hilos (`Get-Process tylluan-nexus`); sin vatímetro |

## Reglas que no cambian
Solo el TL commitea y empuja; un push por ciclo; silencio de carga durante cada push; Deep y Buffy trabajan por relevo de José con su archivo de cola; todo cambio de recuperación pasa por A/B antes de aplicarse.

## No verificado
- Cuántos informes antiguos de los worktrees merecen entrar en `main` (se cuentan: DSH 11, Antigravity 9, Buffy 1; no se ha leído su contenido).
- El estado actual de los bloques del mapa del proyecto: el fichero de bloques tiene más de 64 h sin actualizar.
