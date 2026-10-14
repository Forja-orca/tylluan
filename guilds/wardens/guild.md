# 🛡️ Gremio de Guardianes (Wardens Guild)
Version: 1.0.0  
Oficio Real: **Integridad, Seguridad y Observabilidad del Sistema**

---

> **Nota (2026-10-14).** Este documento se corrigió contra el árbol real de `guilds/`. Lo que aquí figura como existente existe; lo que falta está marcado como "no existe". Los *agentes/roles* son ficheros de texto para cargar en un cliente IDE, no procesos del kernel; los procesos que el kernel arranca son los plugins Python.

## Identidad del Gremio

Los Guardianes son el gremio de la salud sistémica en TylluanNexus. Su oficio es **observar, auditar y proteger** el ecosistema — desde métricas de CPU hasta integridad de guilds, desde logs de errores hasta cumplimiento de seguridad.

Un Guardián nunca cierra los ojos. Ve todo, registra todo, alerta a tiempo.

---

## Misión

> "El sistema que no puede observarse a sí mismo, no puede confiar en sí mismo."

---

## Estructura del Gremio

```
wardens/
├── guild.md
├── (agents/ y sub-agents/: no existen; guardian es un rol previsto)
├── workflows/
│   └── health-audit.md         ← Workflow: Auditoría de Salud Completa
└── plugins/
    ├── audit.py
    └── biome_warden.py
        (monitor.py y system_metrics.py viven en guilds/watchers/plugins/; no hay sandbox/)
```

---

## Agentes del Gremio

| Agente | Rol | Especialidad Principal |
|--------|-----|----------------------|
| `guardian` | Guardián (rol previsto, sin fichero) | Monitoreo continuo, auditorías, alertas de seguridad |

---

## Plugins del Gremio

| Plugin | Descripción |
|--------|-------------|
| `monitor` | Observación de procesos, logs y recursos en tiempo real — en `guilds/watchers/plugins/` |
| `audit` | Auditoría de integridad de guilds y herramientas |
| `system_metrics` | Métricas de CPU, RAM, disco y red — en `guilds/watchers/plugins/` |

---

## Reglas de Colaboración

1. **Alertas Proactivas**: Si el `guardian` detecta una anomalía (crash loop, uso de RAM > 90%), escribe en el Blackboard inmediatamente.
2. **Sin Silencio de Errores**: Ningún error se ignora. Se registra, se clasifica y se asigna.
3. **Read-Only por Defecto**: Los Guardianes observan pero no modifican. Para remediar, delegan al gremio `builders`.
4. **Cycle Audits**: Cada 24h (o en cada mantenimiento), el `guardian` ejecuta el workflow `health-audit.md`.

---

## Memoria Compartida del Gremio

- **Namespace**: `wardens:`
- **Tipos de nodos**: `alert`, `incident`, `audit_result`, `metric_snapshot`

```
tylluan_remember("wardens:incident — 2026-05-12: guild filesystem crash loop por límite de handles. Fix: ulimit -n 8192")
tylluan_recall("wardens: incidentes de crash loop")
```

---

## Workflows Pre-Baked

| Workflow | Cuándo Usarlo |
|----------|--------------|
| `health-audit.md` | Auditoría completa del sistema: guilds, memoria, disco, seguridad |
