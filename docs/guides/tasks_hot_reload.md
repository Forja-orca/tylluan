# Tasks Checklist — Zero-Downtime Hot-Reload Swap

Checklist de las tareas completadas para la integración del proxy de recarga en caliente:

- [x] Crear el crate `crates/tylluan-proxy` y definir sus dependencias de Hyper 1.0 y Tokio en `Cargo.toml`.
- [x] Desarrollar la lógica de proxy inverso con soporte de WebSockets y túnel de sockets TCP en `crates/tylluan-proxy/src/main.rs`.
- [x] Registrar el crate `crates/tylluan-proxy` en la propiedad `members` del `Cargo.toml` raíz de la solución.
- [x] Actualizar la función de arranque HTTP en `crates/tylluan-kernel/src/transport/http/mod.rs` para permitir puertos dinámicos y persistir el puerto asignado en `data/active_port.json`.
- [x] Añadir llamada de apagado ordenado del kernel previo en `crates/tylluan-kernel/src/main.rs` antes de abrir las bases SQLite, evitando bloqueos de archivos.
- [x] Generar documentación técnica de la arquitectura en `docs/guides/HOT_RELOAD_SWAP.md`.
- [x] Registrar el cierre de misión y traza en los canales `mision-activa` y `trazas-tareas` de Coloquio.
