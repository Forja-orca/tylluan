# Prueba externa 02 — portátil Linux de laboratorio

- **Fecha:** noviembre de 2026, pocos días antes del 16
- **Versión probada:** kernel instalado en el portátil (muy anterior a main)
- **Entorno:** portátil Ubuntu de laboratorio, usuario sin privilegios
- **Quién probó:** agente del equipo en otro sistema operativo (no es tercero puro)

## Qué encontró
- El guild `filesystem` no arranca: el espacio de trabajo no tiene el directorio `guilds/` y el
  `python3` del sistema no tiene `mcp`. El arranque falla en unos 130 ms con un mensaje ambiguo
  («MCP handshake failed»).
- Un venv con `--copies` y fastmcp no lo arregla mientras falte el plugin.
- El kernel probado estaba 2445 commits por detrás de main, así que parte de lo visto ya no aplica.

## Lo que cambió
Hallazgo `INSTALL-PYTHON-DISCOVERY-FRAGIL` con la evidencia de Linux. Pendiente: repetir con la
release vigente en una instalación limpia.
