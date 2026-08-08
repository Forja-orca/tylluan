# Spike: Inference Mesh — enrutamiento de modelos enteros por capability

**Fecha**: 2026-08-08
**Relacionado**: `docs/architecture/PROPOSAL_distributed_inference_credit_mesh.md`
**Test real**: `crates/tylluan-link/tests/inference_mesh_spike.rs` — `cargo test -p tylluan-link --test inference_mesh_spike` → **3/3 passed, 0.00s** (DST determinista, sin procesos reales).

## Qué se probó

Reutilizando el mismo patrón DST que ya usa `dispatch_dst.rs` (peers inyectados en memoria contra el `DispatchRouter` real, sin red ni procesos):

1. **`spike_whole_model_capability_routes_to_gpu_peer`** — un nodo sin GPU pide `inference:llama-3-8b-q4`; un peer con GPU que anuncia esa capability exacta gana el enrutamiento. **Confirmado: cero código nuevo necesario en el router** — `route(guild, ...)` acepta cualquier string, incluida una capability de inferencia, exactamente igual que trata `"vision"` o `"bash"` hoy.
2. **`spike_router_distinguishes_between_models`** — dos peers sirviendo modelos distintos (`llama-3-8b-q4` vs `gemma-2b-q4`); el router enruta al que sirve el modelo pedido, no al peer "más fuerte" en general. Confirmado.
3. **`spike_finding_router_is_trust_blind_by_design`** — **hallazgo real, no asumido**: `DispatchRouter` no conoce `FederationPeer.approved`. Solo ve lo que hay en `CapabilityRegistry`. Un peer nunca aprobado, si sus capabilities llegaran a la registry por cualquier vía, se enrutaría igual que uno de confianza. El test lo demuestra pasando con ese comportamiento documentado explícitamente, no oculto.

## Conclusión del spike

La variante "modelos enteros por capability" (estilo exo) es viable con la infraestructura actual **para el enrutamiento**, tal como proponía el documento. Pero el documento asumía que "confianza binaria vía `FederationPeer.approved`" ya estaba conectada al flujo de enrutamiento — **no lo está**. Son dos capas separadas hoy:

- `CapabilityRegistry` / `DispatchRouter` → decide *a quién* enrutar según capacidad y rendimiento.
- `PeerDb.approved` (federation) → decide *en quién confiar* para sincronizar memoria.

No hay ningún punto de integración entre ambas todavía. Antes de cualquier implementación real, hace falta decidir **dónde** se aplica el filtro de confianza: lo más seguro es en el punto de ingestión del gossip de capacidades (`registry.ingest(...)`) — solo ingerir capabilities de peers con `approved == true` — no dentro de `DispatchRouter::route()`, que debe seguir siendo agnóstico de identidad/confianza (separación de responsabilidades limpia).

## Siguiente paso real (no este spike)

Diseñar el punto de integración concreto: quién llama a `registry.ingest()` con datos de capacidad de un peer remoto hoy (probablemente el gossip handler de `federation/mod.rs` o el HTTP handler de mesh), y añadir ahí — no en el router — el chequeo `PeerDb::load_all().iter().find(|p| p.name == node_id).map(|p| p.approved).unwrap_or(false)` antes de aceptar sus capabilities. Eso sí requeriría tocar código de producción; este spike deliberadamente no lo hizo.
