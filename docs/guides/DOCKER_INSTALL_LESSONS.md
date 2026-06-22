# Docker Install — Lecciones aprendidas

Problemas encontrados al construir y desplegar la imagen Docker por primera vez,
y cómo están resueltos en el `Dockerfile` actual para que no ocurran de nuevo.

---

## 1. libonnxruntime.so no encontrada en runtime

**Síntoma:** El kernel arrancaba dentro del contenedor pero fallaba al cargar embeddings:
```
Error: libonnxruntime.so: cannot open shared object file: No such file or directory
```

**Causa:** `pip install onnxruntime` instala `libonnxruntime.so.1.X.Y` en el venv
pero no crea el symlink sin versión (`libonnxruntime.so`) que el crate `ort` (load-dynamic)
busca en `LD_LIBRARY_PATH`.

**Fix aplicado en Dockerfile:**
```dockerfile
RUN /opt/tylluan-venv/bin/pip install onnxruntime>=1.20.0 \
    && find /opt/tylluan-venv -name 'libonnxruntime.so*' -exec cp -P {} /usr/local/lib/ \; \
    && ldconfig /usr/local/lib/
```
`ldconfig` actualiza el caché del linker dinámico y crea los symlinks necesarios.

**Lección:** Siempre ejecutar `ldconfig` después de copiar `.so` de Python al sistema.

---

## 2. Imagen de Rust desactualizada

**Síntoma:** `cargo build` fallaba con errores de edición 2024 / features de estabilización tardía.

**Fix:** Cambio de `rust:1.82-bookworm` → `rust:1.88-bookworm` (tracking latest stable).

**Lección:** No pinear a versiones de Rust demasiado antiguas. El proyecto usa features
del compilador recientes. Usar `rust:latest` o una versión explícita reciente.

---

## 3. Build context innecesariamente grande

**Síntoma:** `docker compose build` tardaba mucho enviando el contexto.

**Fix en `.dockerignore`:**
```
.fastembed_cache/
scratch/
coloquio_digest/
data/
models/
target/
```

**Lección:** Siempre tener `.dockerignore` actualizado. `models/` son varios GB y no deben
entrar en el contexto — se montan como volumen.

---

## 4. Configuración separada por instancia

**Problema:** Una sola `tylluan.toml` para nativo + Docker causaría conflictos de puerto
y rutas (`/home/tylluan/data` vs `./data`).

**Solución implementada:**
- `tylluan.toml` → instancia nativa Windows (`:3030`, `host = "127.0.0.1"`, `dev_mode = true`)
- `tylluan.docker.toml` → instancia Docker (`:3030` interno, `host = "0.0.0.0"`, `dev_mode = false`)
- Montado vía `docker-compose.secondary.yml`: `./tylluan.docker.toml:/home/tylluan/tylluan.toml:ro`

**Regla:** `host = "0.0.0.0"` es seguro en Docker porque el port binding del host ya
restringe el acceso. NUNCA combinarlo con `dev_mode = true` (LAN RCE).

---

## 5. SQLite compartido entre instancias

**Problema:** Si dos instancias apuntan al mismo `data/tylluan.db`, SQLite WAL se corrompe
bajo escritura concurrente.

**Solución:** `data-docker/` completamente separado, montado como volumen independiente.

```yaml
volumes:
  - ./data-docker:/home/tylluan/data   # NO comparte ./data/
  - ./models:/home/tylluan/models:ro   # Modelos compartidos en solo lectura
```

---

## 6. Token de autenticación por instancia

**Solución:** `.tylluan-token` (nativo) y `.tylluan-token-docker` (Docker) son archivos
diferentes, en `.gitignore`, generados por `scripts/docker-init-clean.ps1`.

---

## 7. PYTHONPATH para los guilds Python

**Síntoma:** Los guilds (`bash`, `git`, `filesystem`, etc.) crashean en loop con:
```
ModuleNotFoundError: No module named 'guilds'
```

**Causa:** Los guilds se copian a `/opt/tylluan/guilds/` pero Python se invoca desde
el workspace `/home/tylluan` y no sabe buscar en `/opt/tylluan`.

**Fix en Dockerfile:** Crear un symlink desde el workspace al directorio de guilds:
```dockerfile
RUN mkdir -p /home/tylluan/data /home/tylluan/models \
    && ln -s /opt/tylluan/guilds /home/tylluan/guilds \
    && chown -R tylluan:tylluan /home/tylluan /opt/tylluan
```

**Por qué el symlink y no `ENV PYTHONPATH`:** El kernel Rust sobreescribe
`PYTHONPATH` explícitamente con `workspace_root` (`/home/tylluan`) en
`guild_process.rs:248`. El ENV del contenedor queda ignorado. El symlink
hace que `/home/tylluan/guilds/` exista apuntando a `/opt/tylluan/guilds/`,
así Python los encuentra cuando busca en el PYTHONPATH que pone el kernel.

**Lección:** Inspeccionar el código fuente antes de asumir que un ENV var
del Dockerfile se propagará a subprocesos — el proceso padre puede sobreescribirlo.

---

## Para el release público en GitHub

### Experiencia ideal (1 comando)

```bash
git clone https://github.com/tylluan/tylluan
cd tylluan
docker compose up -d
curl http://127.0.0.1:3030/health
```

### Lo que el Dockerfile ya resuelve automáticamente

| Dependencia | Cómo se resuelve | Lección |
|---|---|---|
| **Rust + compilación** | `rust:1.88-bookworm` builder stage | No pinner a versiones viejas (usar latest o `1.88+`) |
| **ONNX Runtime** | `pip install onnxruntime` + ldconfig + symlink | `ldconfig` es obligatorio después de copiar `.so` al sistema |
| **Python guilds** | symlink `/home/tylluan/guilds → /opt/tylluan/guilds` | No asumir que ENV PYTHONPATH llega a subprocesos — el kernel lo sobreescribe |
| **SQLite separado** | Volumen `./data-docker:/home/tylluan/data` | Dos instancias nunca comparten el mismo `data/` |
| **Config por instancia** | `tylluan.docker.toml` montado como `:ro` | `host = "0.0.0.0"` es seguro en Docker SOLO si `dev_mode = false` |
| **Modelos ONNX** | Volumen `./models:/home/tylluan/models:ro` | No copiar GB dentro de la imagen |
| **Token de autenticación** | Generado por `scripts/docker-init-clean.ps1` | Cada instancia necesita su propio token |

### Scripts de inicialización

| Script | Plataforma | Qué hace |
|---|---|---|
| `scripts/docker-init-clean.ps1` | Windows (PowerShell) | Crea `data-docker/`, genera token, imprime comandos |
| `scripts/install.sh` | Linux | Instalación nativa con systemd, descarga binarios pre-compilados |
| Docker compose | Multi-plataforma | `docker compose up -d` arranque inmediato |

### Checklist pre-release (lo que el mantenedor debe verificar)

- [ ] `docker compose build` → éxito sin errores
- [ ] `docker compose up -d` → contenedor healthy en <30s
- [ ] `curl /health` → `{"status":"ok"}`
- [ ] Al menos 3 guilds Python (bash, filesystem, git) registrados
- [ ] `.dockerignore` actualizado (models/, data/, target/ → ~60MB de contexto)
- [ ] Token históricos purgados vía `git filter-repo` en clon separado
- [ ] `docker compose down` + `up` clean (persistencia de datos)
- [ ] Test en Linux limpio (VM o CI) — Windows Docker Desktop tiene diferencias
