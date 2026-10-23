# COLA DE HERMES — 2026-10-24 (escrita por el TL; el Coloquio está caído)

Mismas reglas de siempre: solo investigación, un documento en `docs/reference/research/`, cita literal buscable con Ctrl+F (URL o ruta de fichero local), inferencias marcadas, <= 60 líneas, SIN commitear, en tu worktree. Cuando vuelva el Coloquio, avisa con `[H-nn COMPLETA]`.

## H-11 (pendiente) — Licencia LFM 1.0 de d1-3B
Documento: `docs/reference/research/2026-10-24_H11_licencia_lfm1_d1_hermes.md`. Qué dice exactamente la licencia sobre uso comercial y umbrales, redistribución, uso como servicio, atribución y restricciones, y si recomendar o documentar el modelo en un repo MIT, o que el usuario lo descargue por su cuenta, exige algo. Fuente primaria: el fichero LICENSE del repo `LiquidAI/d1-3B` en Hugging Face. Solo hechos citados; si es compatible con MIT lo decide José.

## H-12 — Cerrar los «no verificados» del informe de configuración de Hermes
El TL escribió un informe sobre tu configuración con huecos. Tienes el código fuente de Hermes en `%LOCALAPPDATA%\hermes\hermes-agent` (fuente primaria, léelo con `grep`):
1. **Cabeceras de autenticación de un servidor MCP HTTP** en `config.yaml`: ¿`headers:`? ¿expansión de variables de entorno? ¿`auth:`? Cita el código o la doc que lo define.
2. **`windows-sandbox-fallback.json`** (`{"state":"fallback","reason":"boot-loop"}` en `%APPDATA%\hermes`): qué lo escribe, qué significa y qué cambia en el comportamiento del Desktop. Busca el nombre del fichero en `apps/desktop`.
3. **Issue #48820** (el gateway que muere por un Job Object tras `hermes update`): qué dice el arreglo, si ya está en v0.22.4 y qué comando lo recupera.
4. **Checkpoints**: cómo elige Hermes la raíz del proyecto a la que hace `git add -A` y por qué pudo agotar 60 s con `cwd = D:\hermes-projects` (la doc dice que omite directorios de más de 50 000 ficheros).

Entrega `docs/reference/research/2026-10-24_H12_config_hermes_huecos_hermes.md` con tabla pregunta -> cita -> fuente -> estado.
