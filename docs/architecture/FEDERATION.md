# Federación Tylluan — Dos instancias locales

## Arquitectura

```
  Windows (nativo)              Docker Desktop
  ─────────────────             ─────────────────
  tylluan  :3030  ←──────→  tylluan  :3040
  tylluan-proxy  :3030            (via docker-compose.secondary.yml)
  data/tylluan.db                 data-docker/tylluan.db
  tylluan.toml                    tylluan.docker.toml
  .tylluan-token                  .tylluan-token-docker
```

- Las bases de datos son **completamente independientes** — no se comparte SQLite.
- Los modelos (`models/`) se montan **en solo lectura** desde el nativo.
- La federación sincroniza únicamente nodos marcados como `shareable = true`.

## Shared secret de federación

El token compartido entre instancias está en `tylluan.docker.toml`:

```
token = "YOUR_SHARED_SECRET_HERE"
```

Usa este mismo valor al registrar la instancia Docker en el nativo (ver paso 4).

---

## Setup completo (ejecutar una sola vez)

### Paso 1 — Build de la imagen Docker

```powershell
docker compose -f docker-compose.secondary.yml build
```

Primera vez: ~10-15 min (compila Rust en release + instala Python deps).

### Paso 2 — Arrancar instancia Docker

```powershell
docker compose -f docker-compose.secondary.yml up -d
```

### Paso 3 — Verificar health

```powershell
curl http://127.0.0.1:3040/health
```

Esperar hasta 3 minutos (start_period del healthcheck).

### Paso 4 — Registrar Docker como peer en el NATIVO (única modificación al nativo, vía API)

```powershell
curl -X POST http://127.0.0.1:3030/api/v1/federation/peers `
  -H "Content-Type: application/json" `
  -d '{"name":"docker-secondary","url":"http://127.0.0.1:3040","token":"YOUR_SHARED_SECRET_HERE"}'
```

Esto **no modifica `tylluan.toml`** — el kernel lo persiste en memoria y en el toml automáticamente.

---

## Operaciones de federación

### Habilitar sharing en el nativo y empujar a Docker

```powershell
# Habilitar sharing (marca nodos elegibles como shareable)
curl -X POST http://127.0.0.1:3030/api/v1/federation/sharing/enable

# Verificar cuántos nodos están listos para compartir
curl http://127.0.0.1:3030/api/v1/federation/sharing/status

# Push al Docker
curl -X POST http://127.0.0.1:3030/api/v1/federation/sync
```

### Habilitar sharing en Docker y empujar al nativo

```powershell
curl -X POST http://127.0.0.1:3040/api/v1/federation/sharing/enable
curl -X POST http://127.0.0.1:3040/api/v1/federation/sync
```

### Ver peers registrados

```powershell
curl http://127.0.0.1:3030/api/v1/federation/peers
curl http://127.0.0.1:3040/api/v1/federation/peers
```

---

## Parar/arrancar la instancia Docker

```powershell
# Parar (sin borrar datos)
docker compose -f docker-compose.secondary.yml down

# Arrancar de nuevo
docker compose -f docker-compose.secondary.yml up -d

# Ver logs
docker logs tylluan-kernel-secondary -f
```

---

## Limitaciones conocidas

- **Sync unidireccional**: cada instancia debe hacer push por separado para sync bidireccional.
- **Sin replay protection**: no hay timestamp en el body cifrado — solo para uso local.
- **Token = clave auth + cifrado**: el shared secret sirve para ambos. Aceptable en LAN.
