# H-20 — Auditoría read-only de mi propio stack (hermes)

Asignación: TL claude-code T1635. Regla: SOLO lectura, no actualicé nada. Evidencia = comando + salida real (2026-10-24 ~22:20 local). Comparo con las corregidas esta semana del TL: Node 24.22.1, certifi>=2026.10.19, pip>=26.2.2.

| Componente | Versión instalada | Comando | vs corregida |
|---|---|---|---|
| Hermes | v0.22.4 (2026.9.24) · local 4d04303818b6 | `hermes --version` | n/a |
| Python venv del harness | 3.11.16 | `%LOCALAPPDATA%\hermes\hermes-agent\venv\Scripts\python.exe --version` | n/a |
| Python del PATH | 3.14.7 (venv `installs/959c…/environments/27f2…`) | `python --version` + `which python` | n/a |
| Node del PATH | **v24.13.0** | `node --version` + `which node` → `C:\Program Files\nodejs\node` | **BAJO la fix 24.22.1** [misma línea 24.x] |
| Node embebido A (hermes/node) | **v22.23.2** | `%LOCALAPPDATA%\hermes\node\node.exe --version` | línea 22.x; si aplica el aviso de esta semana a 22.x: NO ENCONTRADO (no leí el aviso Node) |
| Node embebido B (hermes/tools) | v26.7.0 | `%LOCALAPPDATA%\hermes\tools\node-26.7.0-win32-x64\node.exe --version` | línea actual, por encima de 24.x |
| npm | 11.14.1 | `npm --version` | n/a (TL no dio objetivo) |
| certifi (venv harness 3.11) | **2026.05.20** | `…venv\Scripts\python.exe -c "import certifi; print(certifi.__version__)"` | **BAJO >=2026.10.19** |
| certifi (entorno PATH 3.14) | **2026.05.20** | `ls …venv/Lib/site-packages \| grep certifi` → `certifi-2026.5.20.dist-info` | **BAJO >=2026.10.19** |
| pip | **NO EXISTE en ninguno de los dos entornos** | `python -m pip --version` → `No module named pip` (ambos) | objetivo pip>=26.2.2: NO ENCONTRADO — nada que comparar |
| uv (gestor real) | 0.12.17 | `uv --version` | sin objetivo del TL |

## Lecturas [I]
- El terminal del harness resuelve `node` → 24.13.0 del sistema: es el que ejecuta `npx`/herramientas npm en sesiones; los dos nodes embebidos (22.23.2 y 26.7.0) los usa el paquete del desktop, cuál para qué NO lo verifiqué (grep solo devolvió dumps de sesión, no el launcher real).
- pip ausente + uv presente = el requisito pip>=26.2.2 de DSH es inaplicable aquí; si el gate `check_supply_chain.sh` de DSH sondea `pip --version`, en esta máquina debe reportar NO ENCONTRADO, no FALLA.
- Dos certifi viejos idénticas (2026.05.20) en ambos entornos: la fix de certifi de esta semana NO está en mi máquina.

## Cumplimiento
Solo lectura ejecutada; cero actualizaciones, cero escrituras fuera de este doc. NO ENCONTRADO declarado donde no hay dato (pip, alcance del aviso Node 22.x). Doc SIN commitear (regla 1).

## Verificacion del TL (2026-10-24)
Reproduje con los comandos de la tabla: el Node embebido de `%LOCALAPPDATA%\hermes\node` es v22.23.2, el de `tools\node-26.7.0-win32-x64` es v26.7.0, el certifi del venv de Hermes (Python 3.11.16) es 2026.05.20 y el modulo `pip` no existe en ese venv. Comparado con el aviso oficial de Node (corregidas 22.23.4, 24.22.1 y 26.12.1), los dos Node embebidos estan por debajo. No comprobe el Node del PATH de Hermes (24.13.0 en esta maquina) en su propio entorno ni el alcance exacto del aviso a la linea 22.x en Hermes. Las lecturas marcadas [I] son de Hermes.
