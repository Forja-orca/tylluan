# Tasks Checklist — Zero-Downtime Hot-Reload Swap (ARCHIVED)

> **⚠️ PARTIALLY IMPLEMENTED.** The kernel side (dynamic port fallback, `data/active_port.json`, shutdown of the previous kernel) exists in `crates/tylluan-kernel/src/transport/http/mod.rs`; the `tylluan-proxy` crate does not exist. See HOT_RELOAD_SWAP.md.

Checklist histórica del diseño. Estado real, comprobado contra el código el 2026-10-14:

- [ ] Crear el crate `crates/tylluan-proxy` y definir sus dependencias de Hyper 1.0 y Tokio en `Cargo.toml`. — **no existe** (no hay `crates/tylluan-proxy`).
- [ ] Desarrollar la lógica de proxy inverso con soporte de WebSockets y túnel de sockets TCP en `crates/tylluan-proxy/src/main.rs`. — **no existe** (no hay `crates/tylluan-proxy`).
- [ ] Registrar el crate `crates/tylluan-proxy` en la propiedad `members` del `Cargo.toml` raíz de la solución. — **no existe** (no hay `crates/tylluan-proxy`).
- [x] Actualizar la función de arranque HTTP en `crates/tylluan-kernel/src/transport/http/mod.rs` para permitir puertos dinámicos y persistir el puerto asignado en `data/active_port.json`. — implementado.
- [x] Añadir llamada de apagado ordenado del kernel previo en `crates/tylluan-kernel/src/main.rs` antes de abrir las bases SQLite, evitando bloqueos de archivos. — implementado, pero en `transport/http/mod.rs` tras enlazar el puerto (no en `main.rs` antes de abrir SQLite).
- [x] Generar documentación técnica de la arquitectura en `docs/guides/HOT_RELOAD_SWAP.md`. — es `HOT_RELOAD_SWAP.md`.
- [ ] Registrar el cierre de misión y traza en los canales `mision-activa` y `trazas-tareas` de Coloquio. — no verificable desde el repositorio.
