# Checklist Antigravity: dashboard y diagramas públicos (2026-10-18)

**Autor:** Claude Code (TL). **Método:** medido sobre `E:\tylluan` en `f01cd9f`/`8b7c85c`; cada punto trae su evidencia y cómo verificarlo. Alcance: frontend (`dashboard/`, `docs-site/`) y diagramas (`docs/assets/`). Todo lo que requiera tocar el kernel se asigna aparte (ver D1).

## Estado medido hoy

| Comprobación | Resultado |
|---|---|
| `npx tsc --noEmit` | exit 0 |
| `npx vitest run` | 7 ficheros, 43 tests, todos verdes |
| `npx eslint src` | 0 errores, **339 avisos** |
| Tamaño `dashboard/dist` | 3,3 MB; último build 2026-10-16 |
| Texto diminuto (`text-[8px]`/`[9px]`/`[10px]`) | **582 usos** |
| Atributos `aria-*`/`role=` en `src/` | 15 usos frente a 54 ficheros con `<button>` |
| Rutas REST del kernel sin consumidor en el frontend | ~70 de 251 (ruido incluido: ficheros estáticos y literales de test) |

## A. Dashboard: huecos funcionales (prioridad alta)

- [ ] **D1. Decision Service sin ninguna vista.** `/api/v1/decide` existe, pero el dashboard solo menciona "decision" en `SchedulerConfusionPanel`. No hay panel de modos por contrato, estado del proveedor (Decima, breaker abierto/cerrado), ni resumen del `decision_shadow_log`. **Bloqueo:** el kernel NO expone endpoint de lectura del shadow log (solo `read_shadow_log` interno). Paso 1 (Deep, aparte): `GET /api/v1/decide/shadow?limit=` clase operador, solo lectura. Paso 2 (tú): panel con acuerdo/desacuerdo por contrato, tabla de últimas filas, aviso "sin verdad humana: no es precisión".
- [ ] **D2. Contradicciones (P2) sin vista.** `/api/v1/silva/contradictions` no tiene consumidor. Hay 368+ nodos `conflicted`. Panel: lista paginada por grupo, texto de ambos lados, y marca clara de que la detección actual es estructural (91% de falsos positivos en la muestra de Buffy: `benchmarks/human_truth_conflict_pairs_v1.json`).
- [ ] **D3. Seguridad y auditoría sin vista.** Sin consumidor: `/api/v1/security/events`, `/api/v1/audit/logs|verify|latency`. Un tablero de operador debe mostrar eventos de seguridad recientes y el resultado de `audit/verify`.
- [ ] **D4. Otros endpoints huérfanos a evaluar (decidir: cablear o documentar como solo-API):** `capability-registry`, `journal` (checkin/recover), `mesh/peers`, `federation/nodes`, `routing/anchors`, `eval/run|results`, `stigmergy/traces`, `system/signals`, `inference/providers`. Entrega una tabla endpoint → decisión (cablear/solo-API/borrar) con una frase de motivo. No cablear a ciegas.
- [ ] **D5. Salud: contador de tools.** `/health?verbose=true` expone `components.sovereign_tools {ok,count}` (debe ser 6). Mostrarlo en el panel de salud y marcar en rojo si `count != 6` o `ok=false`.
- [ ] **D6. Canvas del Coloquio no puede llamar a `tylluan_decide`** (lista de permitidos del dashboard). Decisión del TL pendiente; documentar el motivo en pantalla si se bloquea.

## B. Dashboard: calidad y accesibilidad (prioridad media)

- [ ] **Q1. Texto ilegible:** 582 usos de 8–10 px. Subir el mínimo a 11–12 px en datos y etiquetas; revisar contraste de los grises `slate-500` sobre `#0B0F17`. Criterio: contraste ≥ 4.5:1 en texto de datos.
- [ ] **Q2. Accesibilidad:** 15 `aria-*` para 54 ficheros con botones. Nombre accesible en botones de solo icono, `role`/`aria-live` en estados (online/offline/degraded), foco visible y navegación por teclado en pestañas.
- [ ] **Q3. Color como único canal de estado:** añadir icono o texto (patrón "pill" con palabra), no solo verde/rojo/ámbar.
- [ ] **Q4. 339 avisos de eslint:** clasificarlos por regla, arreglar los de corrección (hooks, `any`) y silenciar con motivo los demás. No dejar el contador sin tendencia: objetivo documentado ≤ 150.
- [ ] **Q5. Colores fijos fuera de tokens:** `DESIGN_AUDIT.md` (2026-07-30) lista 7 fondos literales (`#0B0F17`, `#06080d`, `#0d1017`…). Pasarlos a variables CSS y probar un segundo tema o al menos un contraste coherente.
- [ ] **Q6. Estados vacíos/error/carga:** cada panel necesita los tres. Comprobar con el kernel parado (debe mostrar error con acción de reintento, no pantalla en blanco). Existe `WelcomeEmptyState`: reutilizarlo.
- [ ] **Q7. Móvil:** José usa el dashboard en remoto desde el teléfono. Revisar anchos < 400 px en Overview, Coloquio y Sistema (sin scroll horizontal de página).
- [ ] **Q8. `dist` desactualizado frente al código:** reconstruir (`npm run build`) y comprobar que el kernel lo sirve. Regla del repo: no tocar `tylluan-mcp.bat` sin que José confirme el arranque.

## C. Diagramas públicos (prioridad alta: es lo que ve un visitante)

- [ ] **G1. `docs/assets/architecture.png` está obsoleto** (28-ago): dice `:4000`, "5 Sovereign Tools", "49 Python Guilds" y no tiene el Decision Service. No lo referencia el README (usa el SVG), pero está en el repo y se puede enlazar desde fuera. Regenerarlo desde el SVG actual o borrarlo.
- [ ] **G2. Cifras incoherentes entre diagramas:** SVG dice "45 Python Guilds", el interactivo "46 Python guilds", el PNG "49". Derivar el número de una sola fuente (`/health?verbose=true` o el registro de guilds) y usarlo en los tres. No inventar: citar el comando que lo dio.
- [ ] **G3. Elementos "[Roadmap]" en el SVG** (Retrieval Gate, Postcard Serde, Sparse Vectors, Coloquio Watch): comprobar en `ROADMAP.md`/`STATUS.md` cuáles siguen pendientes y quitar o actualizar los entregados. Un diagrama público no debe prometer lo ya hecho ni ocultar lo no hecho.
- [ ] **G4. `architecture_interactive.spec.json`:** el subtítulo cita `main 8c9287b` (commit antiguo) y `Decision (rules)` no refleja el proveedor de modelo opt-in. Actualizar revisión y rótulo; el rótulo debe decir "reglas + modelo opcional en sombra".
- [ ] **G5. Flechas que cruzan texto en el PNG** (la flecha del cliente atraviesa "tylluan-nexus (:4000)" y "Vector & Graph Engines"): al regenerar, validar con `node bin/archify.mjs validate architecture <spec> --quality showcase --json` (skill archify) y `visual-check` en 1440×900 y móvil.
- [ ] **G6. Incorporar el mapa de puntos neuronales** (`docs/assets/puntos_neuronales_tylluan.html`, ya publicado como Artifact privado) a la documentación pública SOLO si José lo aprueba; hoy está en español y marca propuestas como tales. No lo enlaces desde el README sin permiso.
- [ ] **G7. `docs-site/`:** la página dice "9 subsistemas" con lista fija escrita a mano. Verificar cada nombre contra el código y que el sitio no repita cifras ya desmentidas (puertos, nº de tools, nº de guilds).

## D. Reglas de trabajo (vinculantes)

1. Tu worktree `E:\tylluan-wt-antigravity`, sincronizado con `origin/main`. **Diff sin commitear.** Solo Claude commitea y empuja.
2. Cada ítem cerrado = evidencia: comando + salida (exit code preservado). "Hecho" sin salida no cuenta.
3. Primero los ítems **D1–D3, G1–G2, Q1–Q2**. Entrega por lotes pequeños (un lote = un informe `docs/internal/IMPL_*` con `## Impact`).
4. Nunca arrancar procesos fuera del patrón `test-run-*` con puerto propio; nunca contra el 47004 de producción. Pide a José que lance cualquier `.bat`.
5. Cuota limitada: si te quedas sin cuota a mitad, deja en el informe qué ítems quedaron a medias, con rutas y estado exacto.
6. Verifica las afirmaciones de este checklist antes de actuar; si alguna es falsa, dilo y corrígela en tu informe.

## Límites de este análisis

No abrí el dashboard en un navegador (sin capturas ni medición de contraste real); las cifras de texto diminuto y `aria` son recuentos por búsqueda de texto. La lista de endpoints huérfanos sale de comparar rutas literales: puede incluir falsos positivos (rutas construidas con plantillas).
