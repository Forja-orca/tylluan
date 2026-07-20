# Federación Tylluan — Dos instancias reales (nativo ↔ Docker)

> Creado 2026-07-11 durante el ciclo de reflexión: se detectó que este setup de
> prueba de dos instancias reales (nativo + `docker-compose.secondary.yml`)
> nunca se había armado en Tylluan. El código de federación (Noise XK, DHT,
> gossip, sync push/pull) siempre estuvo intacto — lo que faltaba era la
> infraestructura reproducible para probarlo entre dos procesos reales en vez
> de solo en tests unitarios.

## Arquitectura

```
  Windows (nativo)              Docker Desktop
  ─────────────────             ─────────────────
  tylluan-nexus  :4000  ←──────→  tylluan-nexus  :4040
  dev_mode=true (sin auth)         (via docker-compose.secondary.yml)
  data/tylluan.db                data-docker-secondary/tylluan.db
  tylluan.toml                    tylluan.docker-secondary.toml
```

- Bases de datos completamente independientes — no se comparte SQLite.
- La federación sincroniza únicamente nodos marcados como `shareable = true`.
- El nativo corre en `dev_mode = true` (sin token) — solo para pruebas locales.
  Nunca usar `dev_mode = true` con `host = "0.0.0.0"` (ver invariante de seguridad).

## Setup

### Paso 1 — Build de la imagen Docker (10-15 min la primera vez)

```powershell
cd E:\tylluan
docker compose -f docker-compose.secondary.yml build
```

### Paso 2 — Arrancar la instancia secundaria

```powershell
docker compose -f docker-compose.secondary.yml up -d
```

### Paso 3 — Verificar health de ambos nodos

```powershell
curl http://127.0.0.1:4000/health   # nativo
curl http://127.0.0.1:4040/health   # docker secundario
```

Esperar hasta 3-5 min la primera vez (`start_period` del healthcheck + carga de modelos).

### Paso 4 — Registrar el secundario como peer del nativo

El secundario ya trae pre-configurado al nativo como peer (`tylluan.docker-secondary.toml`).
Falta registrar el sentido inverso, vía API del nativo (dev_mode=true, sin auth requerida):

```powershell
curl -X POST http://127.0.0.1:4000/api/v1/federation/peers `
  -H "Content-Type: application/json" `
  -d '{\"name\":\"docker-secondary\",\"url\":\"http://127.0.0.1:4040\",\"auth_token\":\"secondary-dev-token-change-me\"}'
```

## Prueba real de sincronización

```powershell
# 1. Crear un nodo de prueba en el nativo y marcarlo shareable
curl -X POST http://127.0.0.1:4000/api/v1/memory/remember -H "Content-Type: application/json" `
  -d '{\"content\":\"federation smoke test node\",\"type\":\"fact\",\"metadata\":{\"shareable\":true}}'

# 2. Habilitar sharing y empujar
curl -X POST http://127.0.0.1:4000/api/v1/federation/sharing/enable
curl -X POST http://127.0.0.1:4000/api/v1/federation/sync -H "Content-Type: application/json" -d '{\"peer\":\"docker-secondary\"}'

# 3. Verificar en el secundario que el nodo llegó
curl http://127.0.0.1:4040/api/v1/memory/recall -H "Content-Type: application/json" -d '{\"query\":\"federation smoke test\"}'
```

Si el paso 3 devuelve el nodo creado en el paso 1, la federación real entre dos
procesos independientes (no loopback de un solo proceso) queda verificada
empíricamente, no solo por tests unitarios.

## Limpieza

```powershell
docker compose -f docker-compose.secondary.yml down
Remove-Item -Recurse -Force data-docker-secondary
```
