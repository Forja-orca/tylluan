## Goal
- ~~Implement complexity scoring cascade (M20)~~ ✅ **CERRADO**
- ~~Implement scheduler guild (async timer service)~~ ✅ **CERRADO**
- ~~Curar y asentar documentación~~ ✅

## Done (this session)
- **M20 Complexity Cascade**: `complexity.rs` + proactive routing (`routing.rs:40-47`) + reactive cascade (`mod.rs:599-628`). Guardado por `registry.has_guild("coordinator")`.
- **Coordinator en lazy_guilds** (`main.rs:651`): registrado en registry al startup sin depender de `registry.json`.
- **Scheduler guild** (`guilds/core/scheduler.py`), always-on:
  - SQLite persistence (`data/scheduler.db`) — survives kernel restarts
  - Background thread (30s poll) checks due schedules
  - Fired schedules → `POST /api/v1/coloquio/channels/{channel}/post` con `@agent_id`
  - Tools: `schedule(intent, agent_id, delay_minutes, channel)`, `cancel_schedule(id)`, `list_pending(agent_id)`
  - Zero external deps (stdlib: sqlite3, threading, urllib, uuid)
- **Documentación curada**:
  - README.md: version v0.12.0→v0.13.0, guilds 47+→42, tests 363→349
  - STATUS.md: M18-P3, M20, scheduler añadidos; test counts corregidos
  - `catalog.rs:590`: KNOWN_GUILDS actualizada
  - `tylluan.toml`: scheduler en always_on
  - `tylluan.example.toml`: scheduler en always_on
- **Seguridad**: 0 secretos trackeados, 0 `0.0.0.0`+`dev_mode` en prod, 0 `dbg!`/`todo!` en producción
- Tests: 286/286 PASS

## Next Steps
- M20-B: lazy semantic complexity vía OnceLock (low priority)
- ADR-009 para scheduler guild + M20 architecture
