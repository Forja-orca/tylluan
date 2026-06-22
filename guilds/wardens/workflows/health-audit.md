# 🛡️ Workflow: Auditoría de Salud Completa
Guild: `wardens`  
Version: 1.0.0

---

## Cuándo Usar Este Workflow

- Cada 24 horas (rutina)
- Después de un incidente
- Antes de un release
- Cuando el dashboard muestra anomalías

---

## Checklist de Auditoría

### 1. Estado del Kernel (2 min)
```bash
# ¿El kernel está respondiendo?
curl -s http://127.0.0.1:3030/health | jq .

# Uptime y métricas básicas
curl -s http://127.0.0.1:3030/api/v1/health/detailed | jq .

# Últimos errores en logs
tail -50 logs/kernel.log | grep -E "ERROR|panic|CRITICAL"
```

**Criterio de éxito**: `status: "healthy"` en /health, 0 panics en logs

---

### 2. Estado de los Guilds (3 min)
```bash
# ¿Cuántos guilds están activos?
curl -s http://127.0.0.1:3030/api/v1/guilds | jq '[.guilds[] | select(.running == true)] | length'

# ¿Hay guilds en crash loop?
curl -s http://127.0.0.1:3030/api/v1/guilds | jq '[.guilds[] | select(.restarts_5m > 3)]'
```

Via `tylluan_do`:
```
tylluan_do("audita el estado de todos los guilds")
# → audit.py ejecuta inventario completo
```

**Criterio de éxito**: Todos los `always_on` guilds activos, ninguno con restarts > 3

---

### 3. Métricas del Sistema (2 min)
```
tylluan_do("muestra métricas del sistema: CPU, RAM, disco")
# → system_metrics.py

Umbrales de alerta:
- CPU > 80% (media 5min): ⚠️ Warning
- RAM > 85%: ⚠️ Warning  
- Disco > 90%: 🔴 Critical
- SilvaDB > 1GB: ⚠️ Revisar decay policy
```

---

### 4. Estado de SilvaDB (2 min)
```bash
curl -s http://127.0.0.1:3030/api/v1/silva/stats | jq .

# Verificar:
# - node_count: ¿crecimiento razonable?
# - edge_count: ¿ratio nodos/edges normal? (típico: 2-5 edges/nodo)
# - db_size_bytes: ¿dentro del límite?
```

---

### 5. Seguridad (1 min)
```bash
# ¿El kernel NO está expuesto en 0.0.0.0 con dev_mode=true?
cat tylluan.toml | grep -A 3 "\[nexus\]"

# Criterio: host = "127.0.0.1" O (host = "0.0.0.0" AND dev_mode = false)
# Si: host = "0.0.0.0" AND dev_mode = true → 🔴 CRITICAL: RCE disponible
```

---

## Acciones por Resultado

| Resultado | Acción Inmediata |
|-----------|-----------------|
| Guild en crash loop | `POST /api/v1/guilds/{name}/reset-backoff` + investigar logs |
| RAM > 85% | `POST /api/v1/maintenance/vacuum` + revisar si hay fugas |
| Disco > 90% | Rotar logs + `POST /api/v1/maintenance/checkpoint` |
| dev_mode=true + 0.0.0.0 | Cambiar a 127.0.0.1 INMEDIATAMENTE |
| SilvaDB > 1GB | Habilitar decay en tylluan.toml temporalmente |

---

## Persistir Resultados

```
tylluan_remember("wardens:audit_result — [fecha]: estado=[healthy/degraded/critical], 
guilds=[N/M activos], cpu=[X%], ram=[Y%], issues=[lista]")
```

---

## Escalado

Si durante la auditoría se encuentra un problema crítico:
1. Escribir en Blackboard: `[WARDEN ALERT] [descripción del problema]`
2. Notificar al gremio `builders` si requiere fix de código
3. Documentar en SilvaDB como `wardens:incident`
