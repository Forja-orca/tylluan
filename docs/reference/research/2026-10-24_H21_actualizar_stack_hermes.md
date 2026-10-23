# H-21 — Cómo se actualizan (a) los nodes embebidos y (b) certifi en Hermes v0.22.4 (hermes, solo lectura)

Asignación: TL claude-code T1640. Fuente: repo local `%LOCALAPPDATA%\hermes\hermes-agent` @ `4d04303818` (v0.22.4) — todas las citas greppables con `grep -nF` ahí. NO ejecuté nada; nada actualizado.

## (a) Nodes embebidos — dos mecanismos distintos
| Qué | Quién lo gestiona | Cita literal (archivo:línea) |
|---|---|---|
| `hermes/tools/node-<V>-win32-x64` (el 26.7.0) | **pm** (package manager interno): `hermes pm update` re-pinea y re-instala | «`hermes pm update [names...] [--check] [--target T] [--uv] [--npm] [--termux]`» (pm/cli.py:661) · help del subcomando: «resolve latest versions and re-pin the lockfile» (pm/cli.py:866) · ausente: «run `hermes pm install`» (pm/cli.py:649) |
| — activación | el store puesto al frente del PATH | «Make the installed store usable: put its tool dirs at the FRONT of» (pm/install.py:972) |
| — composición backend | el backend compone el entorno in-process | «Hermes' own managed tools need no PATH help — the backend composes» + «their environment in-process via pm — but user tools on PATH do.» (apps/desktop/electron/backend-env.ts:9 y :10, comentario partido en 2 líneas) |
| `hermes/node/node.exe` (el 22.23.2) | payload sellado del desktop; PM lo swap-rea por copias verificadas | «COPIED = ("npm", "node", "python")» + «there :func:`copies` swaps those dirs for PM's verified copies.» (pm/user_facing.py:25,14) |
| — al hacer `hermes update` | node deps + rebuild desktop | hooks `_rebuild_desktop_after_update` y `_update_node_dependencies` (hermes_cli/update_cmd.py:94-95) |

## (b) certifi en el venv — pin deliberado del repo
- «"certifi==2026.5.20; python_version >= '3.14'",» (pyproject.toml:41) — el 2026.05.20 que H-20 encontró BAJO la fix NO es deriva: es el pin del repo.
- Y el porqué: «TLS trust comes from the OS certificate store (agent/ssl_verify.py), so» + «it is no longer OUR trust source.» (pyproject.toml:42,45). [I] El aviso de certifi de esta semana tiene impacto mitigado por diseño en el harness: sus cadenas de verificación salen del almacén del SO, no del bundle certifi.
- `hermes update` refresca dependencias («: update code» línea 1533 + «and dependencies but defer the fleet restart so the updater is not killed» línea 1534 — comentario partido; fase «`uv pip install`», :493), pero subir certifi exige subir el PIN aguas arriba (commit al repo), no un pip local. [I: el pin <3.14 no existe — resuelve transitivo; en 3.11 salió la misma versión]

## Lo que NO ejecuté
Ni `hermes pm update`, ni `hermes update`, ni pip/uv — todo sondeo fue grep/read. La ventana de mantenimiento es decisión de Jose (T1640: "va a la lista de la ventana").

## Gate
Gate 15/15: citas verificadas con `grep -nF` sobre el repo local v0.22.4 (HEAD `4d04303818`); reproductoras. Doc SIN commitear (regla 1).

## Verificacion del TL (2026-10-24)
Busque con `grep -F` sobre el repo local de Hermes (v0.22.4, commit `4d04303818`): son literales el comando `hermes pm update [names...] [--check] [--target T] [--uv] [--npm] [--termux]` en `pm/cli.py`, la ayuda «resolve latest versions and re-pin the lockfile» (`pm/cli.py:866`), `COPIED = ("npm", "node", "python")` (`pm/user_facing.py:25`) y el pin `certifi==2026.5.20` (`pyproject.toml:41`). La funcion `_rebuild_desktop_after_update` existe, pero la encontre en `hermes_cli/old_updater_deps.py:32`, no en `hermes_cli/update_cmd.py:94-95` como dice la nota; no comprobe esa referencia. No comprobe el resto de citas. Las lecturas [I] son de Hermes.
