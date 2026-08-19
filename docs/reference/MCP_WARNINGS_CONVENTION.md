# Convención `warnings` para las 5 tools soberanas

**Estado:** Fase 1 implementada (PPR, referencia). Fase 2 (aplicar a `recall`, `do`, `think`, `remember`) pendiente.
**Origen:** debate real de equipo en Coloquio (`mision-activa`, turnos 92-187, 2026-08-19) — hallazgo de Deep, verificado en código por Claude Code, votado por Deep/Antigravity/Claude Code sin disenso.

## El problema real que resuelve

MCP estándar (`CallToolResult`) solo tiene `content` + `is_error` — no hay campo nativo para "la llamada tuvo éxito, pero con un matiz que el cliente debería saber". Esto produce falsos negativos: un cliente MCP estricto (sin nuestro harness ni logs del servidor) no puede distinguir "no hay resultados porque no hay nada relevante" de "no hay resultados porque la entrada era inválida y ni se intentó procesarla de verdad".

Caso real que lo destapó: `tylluan_graph(command="ppr", seeds=["tylluan_do"])` devolvía `results: []` con `is_error: false` cuando el seed no era un ID de nodo real (era un nombre de tool) — indistinguible de un subgrafo vacío legítimo.

## La convención

Un campo `warnings` (array, opcional) dentro del JSON de `content`, junto al `result`/payload normal de la tool. `is_error` se mantiene `false` — la petición se procesó, solo hay un aviso semántico, no un fallo.

```json
{
  "action": "ppr",
  "seeds": ["tylluan_do"],
  "results": [],
  "warnings": [
    {
      "code": "NODE_NOT_FOUND",
      "severity": "warn",
      "message": "1 of 1 seed(s) are not real node IDs and were never expanded: tylluan_do",
      "suggestion": "Seeds must be real node IDs (e.g. 'agent_memory:...', 'lesson:...'), not tool or guild names. Use tylluan_graph(command='stats') or list_neighbors to discover valid IDs."
    }
  ]
}
```

### Por qué no un campo MCP nuevo a nivel de protocolo

`CallToolResult` no tiene campo de warnings — inventar uno no estándar rompería el contrato MCP para clientes estrictos. La convención vive **dentro** del JSON que ya se serializa a `content` (texto), que todo cliente ya parsea. Un cliente que no conoce `warnings` simplemente lo ignora — retrocompatible por diseño, cero riesgo de romper nada.

### Códigos compartidos (a ampliar según se vayan encontrando casos reales)

- `NODE_NOT_FOUND` — un ID pasado por el caller no existe como nodo real.
- `INVALID_PARAMETER` — parámetro con formato válido pero semánticamente incorrecto (no confundir con error de esquema, que sigue siendo `is_error: true`).
- `EMPTY_INPUT` — entrada vacía que se procesó igualmente con un resultado degradado, no un rechazo.
- `DEPRECATED_USAGE` — reservado para uso futuro (avisos de deprecación de parámetros/formatos).

## Implementación de referencia

`crates/tylluan-kernel/src/transport/server/handler_graph.rs` (comando `ppr`) + `crates/tylluan-kernel/src/memory/silva/graph.rs` (`existing_node_ids`). Antes de correr el PPR, se resuelve qué seeds existen realmente como nodos; si ninguno resuelve, se salta el cálculo (sería puro ruido) y se devuelve el warning directamente; si algunos resuelven, el PPR corre igual y el warning se añade al payload de éxito.

## Pendiente (Fase 2)

Aplicar el mismo patrón, caso a caso y verificado contra código real (no asumido), a:
- `tylluan_recall` — query sin matches vs. query mal formada.
- `tylluan_do` — guild inexistente o argumento faltante ya usa mensajes de error descriptivos (`ACTION_REPORT`), revisar si encaja en esta convención o ya está resuelto.
- `tylluan_think` — grafo vacío para el query dado.
- `tylluan_remember` — contenido vacío o duplicado exacto.

No implementar las 4 de golpe — cada una tiene su propia semántica de "qué es realmente un warning vs. un resultado legítimo vacío", igual que PPR necesitó su propio análisis.
