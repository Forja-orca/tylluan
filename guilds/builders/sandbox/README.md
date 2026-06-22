# 🔬 Sandbox Lab — Gremio de Constructores
Guild: `builders`  
Version: 1.0.0

---

## ¿Qué es el Sandbox?

El Sandbox es el **espacio de experimentación seguro** del gremio. Aquí los agentes pueden:

- Probar integraciones nuevas sin arriesgar el código de producción
- Validar scripts y automatizaciones antes de usarlos en el kernel
- Crear y refinar workflows experimentales
- Compartir hallazgos con otros agentes del gremio

**Regla fundamental**: Nada llega a `guilds/core/` o al kernel sin haber pasado por el sandbox primero (si es experimental).

---

## Estructura del Sandbox

```
sandbox/
├── README.md                   ← Este archivo
├── experiments/                ← Experimentos activos (carpeta principal)
│   ├── template/               ← Template para nuevos experimentos
│   │   ├── experiment.md       ← Descripción del experimento
│   │   ├── test_script.py      ← Script de prueba (Python)
│   │   └── notes.md            ← Notas del agente
│   └── .gitkeep
└── staging.toml                ← Config de entorno staging (subset de tylluan.toml)
```

---

## Cómo Crear un Experimento

### Paso 1: Crear la carpeta del experimento
```bash
mkdir guilds/builders/sandbox/experiments/mi-experimento-FECHA
```

### Paso 2: Crear experiment.md con la descripción
```markdown
# Experimento: [Nombre]
Agente: [quien lo inicia]
Fecha: [YYYY-MM-DD]
Objetivo: [qué se quiere probar]
Hipótesis: [por qué crees que funcionará]
Status: [active | completed | abandoned]
```

### Paso 3: Implementar y validar
- Scripts en el directorio del experimento (no en `guilds/core/`)
- Probar en entorno aislado primero
- Documentar resultados en `notes.md`

### Paso 4: Promover o descartar
```
Si funciona:
  → Proponer como nuevo workflow en guilds/builders/workflows/
  → O integrar en guilds/core/ como nuevo plugin
  → tylluan_remember("builders:experiment-result — [nombre]: [resultado]")

Si no funciona:
  → Documentar por qué no funcionó
  → tylluan_remember("builders:lesson — [lo que no funciona y por qué]")
  → Mover a experiments/archived/
```

---

## staging.toml — Entorno de Pruebas Aislado

```toml
[nexus]
host = "127.0.0.1"
port = 3031          # Puerto diferente al producción (3030)
dev_mode = true

[silva]
db_path = "./sandbox/staging.db"   # DB separada de producción

[guilds.core]
always_on = ["bash", "filesystem"] # Mínimo necesario para experimentos
```

Para usar el entorno staging:
```bash
TYLLUAN_CONFIG=guilds/builders/sandbox/staging.toml cargo run -p tylluan-kernel
```

---

## Experimentos Activos

_(Los agentes añaden aquí sus experimentos en curso)_

| Experimento | Agente | Objetivo | Status |
|-------------|--------|----------|--------|
| — | — | — | — |

---

## Reglas del Sandbox

1. **Aislamiento**: Nunca usar la DB de producción (`data/silva.db`) desde el sandbox.
2. **Limpieza**: Los experimentos completados (>30 días) se archivan o eliminan.
3. **No Secretos**: No commitear tokens o credenciales en experimentos.
4. **Compartir**: Si un experimento tiene un hallazgo valioso, documentarlo en SilvaDB.
