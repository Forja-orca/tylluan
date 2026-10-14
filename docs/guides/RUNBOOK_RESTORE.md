# RUNBOOK — Backup, restore y simulacro de restauración

> **Por qué existe:** la memoria acumulada (`silva.db`, `tylluan.db`, `mailbox.db`, `peers.db`, `audit.db`)
> vive en un solo disco de una sola máquina. Un backup que nunca se ha restaurado NO es un backup.
> El simulacro se ejecuta **1 vez al mes** (primer lunes), dura ~15 minutos y **no toca el kernel de producción**.

Este runbook usa los comandos `tylluan-cli backup` y `tylluan-cli restore` (ver `tylluan-cli --help`).
Los nombres de fichero y las reglas de abajo salen de `crates/tylluan-cli/src/backup.rs`.

## Qué hacen los dos comandos

| Comando | Qué hace | Cuándo |
|---|---|---|
| `tylluan-cli backup <DIR>` | Copia consistente con `VACUUM INTO` **con el kernel encendido**. Escribe los `.db` y un `backup-manifest.json`. | Cuando quieras; no para el kernel. |
| `tylluan-cli restore <DIR>` | Sustituye las bases de datos por las del backup. **Se niega a correr si hay cualquier proceso `tylluan-nexus` o `/health` responde.** Antes de tocar nada guarda el estado actual en `<data>/pre_restore_<fecha>/`. | Recuperación real, con el kernel parado (`tylluan-cli stop`). |

Fuentes por defecto del backup: `[silva].db_path` y `[memory].db_path` de `tylluan.toml` (si no hay, `./data/silva.db` y `./data/tylluan.db`) más `peers.db`, `mailbox.db` y `audit.db` junto a ellos. `--db <fichero>` añade bases extra. Los ficheros que no existen o no son SQLite se omiten con un aviso.

**Ejecuta ambos comandos desde el directorio que contiene `tylluan.toml`**: es donde leen la configuración y donde resuelven `./data`.

## ⚠️ Antes de un `restore` real: lee las rutas del manifiesto

`restore` escribe cada base de datos en la ruta `source` que figura en `backup-manifest.json`. Si el backup se hizo con rutas **absolutas**, el restore escribirá en esas rutas absolutas, estés donde estés. Abre el manifiesto y comprueba los `source` antes de ejecutar nada. No uses `restore` para un simulacro: con producción encendida se niega a correr, y con producción apagada podría sobrescribirla.

## Simulacro mensual (producción intacta)

### Pre-requisitos
- Kernel de producción corriendo (no se para; el simulacro usa una instancia efímera en otro puerto).
- Espacio libre para una copia de tus bases de datos.

### Paso 1 — Snapshot consistente

Desde el directorio de tu `tylluan.toml`:

```powershell
$stamp = Get-Date -Format "yyyy-MM-dd"
tylluan-cli backup "E:\tylluan-restore-drill\$stamp\backup"
```

Debe imprimir `N database(s) backed up` y crear `backup-manifest.json`. Si faltan bases de datos que esperabas, el aviso lo dirá.

### Paso 2 — Instancia efímera en directorio y puerto propios

Nunca arranques el kernel de prueba desde el directorio de producción ni sobre su `data/`.

```powershell
$drill = "E:\tylluan-restore-drill\$stamp\run"
New-Item -ItemType Directory -Force "$drill\data" | Out-Null

# Los .db del backup son ficheros SQLite completos y consistentes: se copian tal cual.
Copy-Item "E:\tylluan-restore-drill\$stamp\backup\*.db" "$drill\data\"

# Tu configuración, con otro puerto y sin stdio:
Copy-Item .\tylluan.toml "$drill\tylluan.toml"
# Edita $drill\tylluan.toml:
#   [nexus] port = 47019, host = "127.0.0.1", transport = ["http", "sse"]
#   y que [memory].db_path y [silva].db_path apunten a ./data/ (rutas relativas)

Copy-Item (Get-Command tylluan-nexus).Source "$drill\"
cd $drill
$env:TYLLUAN_DATA_DIR = "$drill\data"
.\tylluan-nexus.exe        # en primer plano, en su propia terminal
```

### Paso 3 — Verificación (en otra terminal)

```powershell
# 1. Vive y es la versión esperada (puede decir "warming_up" unos segundos antes de "ok")
curl http://127.0.0.1:47019/health

# 2. La memoria está entera: compara "node_count" con el de producción (curl .../api/v1/silva/stats en tu puerto real)
curl http://127.0.0.1:47019/api/v1/silva/stats

# 3. La búsqueda sobre memoria funciona en el snapshot: usa una frase que sepas que existe en producción
curl "http://127.0.0.1:47019/api/v1/memory/search?q=una-frase-que-exista&limit=3"

# 4. El coloquio se restauró
curl "http://127.0.0.1:47019/api/v1/coloquio/channels"
```

Si tu instancia efímera usa `dev_mode = false`, añade `Authorization: Bearer <token>` (el token se genera en `.tylluan-token` del directorio de arranque de esa instancia).

### Paso 4 — Cierre y registro

```powershell
# Ctrl+C en la terminal del kernel efímero.
# Registra el resultado en el historial de abajo y en el canal general de Coloquio:
#   [RESTORE-DRILL <fecha>] OK|FAIL — nodos: N, recall: OK|FAIL, notas: ...
# Conserva el snapshot del mes; borra el del mes anterior.
```

## Recuperación real (kernel parado)

```powershell
tylluan-cli stop
# Revisa los "source" de backup-manifest.json (ver aviso de arriba)
tylluan-cli restore "E:\ruta\al\backup"
# Guarda el estado anterior en <data>\pre_restore_<fecha>\ — consérvalo hasta verificar el hub.
tylluan-cli start
```

Después de arrancar, repite los cuatro checks del paso 3 contra el puerto real.

## Criterios de FALLO (cualquiera = incidente, avisar al equipo)
- El número de nodos difiere más de un 5 % de producción.
- El recall devuelve vacío o error sobre contenido que existe en producción.
- El kernel efímero no arranca con el snapshot.
- `backup` omite una base de datos que debería existir.

## Historial de drills
| Fecha | Resultado | Nodos | Verificó | Notas |
|---|---|---|---|---|
| 2026-06-11 | ✅ PASS | 2418 | the operator + Claude Code | health ok, 2418 nodos (±18 vs prod), recall score 0.88, 8 canales coloquio restaurados. Guild memory no disponible (esperado — guilds Python no forman parte del snapshot). Hecho con el procedimiento anterior (copia con Python), antes de que existiera `tylluan-cli backup`. |
| 2026-10-14 | ✅ PASS (entorno aislado) | 5 | Claude Code | Binarios Linux publicados de v0.17.1 en WSL, datos de prueba: backup en caliente con el kernel encendido (5 bases de datos, `source` relativos), copia a un directorio nuevo, segundo kernel en otro puerto: `node_count` 5 = 5; de 3 recuerdos únicos escritos antes del backup se buscaron 2 con `/api/v1/memory/search` y los 2 aparecieron. `tylluan-cli restore` se negó a correr con el kernel arriba, como documenta. Observado: `conflicted_count` pasó de 0 a 1 tras el arranque de la copia (no investigado). |

> El procedimiento con `tylluan-cli backup` se probó de punta a punta el 2026-10-14 en un entorno aislado con datos de prueba (fila de arriba), no con una memoria real: el primer simulacro con tus datos será su primera verificación real.
