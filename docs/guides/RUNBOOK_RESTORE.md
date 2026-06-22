# RUNBOOK — Drill de Restore de Memoria (M23/CLAUDE-5)

> **Por qué existe:** 12+ meses de memoria colectiva (silva.db + tylluan.db + mailbox.db) viven
> en un solo disco de una sola máquina. Un backup que nunca se ha restaurado NO es un backup.
> Este drill se ejecuta **1 vez al mes** (primer lunes). Duración: ~15 minutos.
> Ejecuta: the operator (el kernel de producción NO se toca). Verifica: cualquier agente.

## Pre-requisitos
- Kernel de producción corriendo en :3030 (NO se para — el drill usa una instancia efímera en otro puerto).
- ~200 MB libres en disco.

## Paso 1 — Snapshot consistente (sin parar producción)

SQLite en WAL permite copia consistente vía API de backup, NUNCA con copy directo de archivos:

```powershell
$stamp = Get-Date -Format "yyyy-MM-dd"
$dest = "E:\tylluan-restore-drill\$stamp"
New-Item -ItemType Directory -Force "$dest\data" | Out-Null

# Backup consistente vía sqlite3 (NO Copy-Item — un copy en caliente puede salir corrupto)
E:\TylluanMCPo3\.venv\Scripts\python.exe -c @"
import sqlite3
for db in ['silva', 'tylluan', 'mailbox']:
    src = sqlite3.connect(f'file:E:/TylluanMCPo3/data/{db}.db?mode=ro', uri=True)
    dst = sqlite3.connect(rf'$dest\data\{db}.db'.replace('\\','/'))
    src.backup(dst)
    dst.close(); src.close()
    print(f'{db}.db OK')
"@
```

## Paso 2 — Instancia efímera en puerto alternativo

```powershell
# Config mínima apuntando al snapshot (puerto 3045 para no violar el singleton de :3030)
Copy-Item E:\TylluanMCPo3\tylluan.toml "$dest\tylluan.toml"
# Editar $dest\tylluan.toml: port = 3045, y db_path/silva.db_path hacia $dest\data\
Copy-Item E:\TylluanMCPo3\target\debug\tylluan.exe "$dest\"

cd $dest
.\tylluan.exe   # the operator lo arranca en su terminal, en foreground
```

## Paso 3 — Verificación (los 4 checks, en otra terminal)

```powershell
# 1. Vive y es la versión esperada
curl http://127.0.0.1:3045/health          # status ok + commit

# 2. La memoria está entera
curl http://127.0.0.1:3045/api/v1/silva/stats   # node_count ~= producción (±escrituras del día)

# 3. El recall semántico funciona sobre el snapshot
curl -X POST http://127.0.0.1:3045/api/v1/do -H "Content-Type: application/json" `
  -d '{"intent":"recall herramientas soberanas tylluan"}'    # debe devolver contenido real

# 4. El coloquio se restauró
curl "http://127.0.0.1:3045/api/v1/coloquio/channels"      # canales reales presentes
```

## Paso 4 — Cierre y registro

```powershell
# Ctrl+C en la terminal del kernel efímero, luego:
# Publicar resultado en el coloquio (canal general):
#   [RESTORE-DRILL <fecha>] OK|FAIL — nodos: N, recall: OK|FAIL, notas: ...
# Conservar el snapshot del mes; borrar el del mes anterior.
```

## Criterios de FALLO (cualquiera = incidente, avisar al equipo)
- node_count difiere >5% de producción.
- Recall devuelve vacío o error sobre contenido que existe en producción.
- El kernel efímero no arranca con el snapshot.

## Historial de drills
| Fecha | Resultado | Nodos | Verificó | Notas |
|---|---|---|---|---|
| 2026-06-11 | ✅ PASS | 2418 | the operator + Claude Code | health ok, 2418 nodos (±18 vs prod), recall score 0.88, 8 canales coloquio restaurados. Guild memory no disponible (esperado — guilds Python no forman parte del snapshot). |
