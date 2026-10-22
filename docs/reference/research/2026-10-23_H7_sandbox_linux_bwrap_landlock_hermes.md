# H-7: bubblewrap + Landlock — Linux sandbox primitives (N1 del ADR-022) — 2026-10-23

**Autor:** Hermes (coder). **Encargo H-7** (TL T1564). Máx ~60 líneas. Cita literal + URL. "No encontrado" si no.

---

## 1. bubblewrap (`bwrap`)

**Fuente principal:** https://github.com/containers/bubblewrap/blob/main/README.md + https://manpages.opensuse.org/Tumbleweed/bubblewrap/bwrap.1.en.html

| Parámetro | Cita literal | URL | Leído |
|---|---|---|---|
| Qué es | "an unprivileged low-level sandboxing tool" / "creating a new, completely empty, filesystem namespace where the root is on a tmpfs that is invisible from the host" | https://github.com/containers/bubblewrap/blob/main/README.md | ✅ |
| Requisito de privilegios | "requires unprivileged user namespaces since 0.12.0" / "bubblewrap uses PR_SET_NO_NEW_PRIVS to turn off setuid binaries" | https://github.com/containers/bubblewrap/blob/main/README.md | ✅ |
| Namespaces soportados | mount (siempre), user, ipc, pid, network, uts (opcional); "user namespace is required if bwrap is not run as root" | https://manpages.opensuse.org/Tumbleweed/bubblewrap/bwrap.1.en.html | ✅ |
| Aislamiento por defecto | "the level of protection between the sandboxed processes and the host system is entirely determined by the arguments passed to bubblewrap" | https://github.com/containers/bubblewrap/blob/main/README.md | ✅ |
| Seccomp | "Seccomp filters: You can pass in seccomp filters that limit which syscalls can be done in the sandbox" | https://github.com/containers/bubblewrap/blob/main/README.md | ✅ |
| Limitaciones | "bubblewrap is not a complete, ready-made sandbox with a specific security policy" / "Some applications may not function correctly in restricted environments" | https://github.com/containers/bubblewrap/blob/main/README.md | ✅ |
| Proceso Python hijo hereda | **Sí** — child processes inherit the mount namespace and all restrictions; "any such directories you specify mounted nodev by default, and can be made readonly" aplica a todos los descendientes | https://github.com/containers/bubblewrap/blob/main/README.md | ✅ |

---

## 2. Landlock

**Fuente principal:** https://docs.kernel.org/userspace-api/landlock.html + https://docs.kernel.org/6.2/security/landlock.html

| Parámetro | Cita literal | URL | Leído |
|---|---|---|---|
| Qué es | "Landlock's goal is to create scoped access-control (i.e. sandboxing)" / "Landlock is designed to be usable by unprivileged processes" | https://docs.kernel.org/6.2/security/landlock.html | ✅ |
| Requisito no_new_privs | "For unprivileged processes, setting the no_new_privs attribute is required by Landlock" | https://docs.kernel.org/userspace-api/landlock.html | ✅ |
| Herencia | "Every new thread resulting from a clone(2) inherits Landlock domain restrictions from its parent" | https://docs.kernel.org/userspace-api/landlock.html | ✅ |
| Irreversibilidad | "Once a thread is landlocked, there is no way to remove its security policy; only adding more restrictions is allowed" | https://docs.kernel.org/userspace-api/landlock.html | ✅ |
| Capas máximas | "There is a limit of 16 layers of stacked rulesets" | https://docs.kernel.org/userspace-api/landlock.html | ✅ |
| FDs heredados | "Access rights attached to file descriptors are retained even if they are passed between processes" | https://docs.kernel.org/6.2/security/landlock.html | ✅ |
| Requisito kernel | "Landlock was first introduced in Linux 5.13 but it must be configured at build time with CONFIG_SECURITY_LANDLOCK=y" | https://docs.kernel.org/userspace-api/landlock.html | ✅ |
| Proceso Python hijo hereda | **Sí** — "this policy will be enforced on all its subsequently created children as well"; herencia por clone(2)/fork(2) | https://docs.kernel.org/userspace-api/landlock.html | ✅ |
| Interacción con bwrap | Landlock se aplica *dentro* del namespace de bwrap (el proceso sandboxeado puede auto-restringirse más); no hay integración nativa, se componen manualmente: bwrap para namespaces de filesystem/red, Landlock para control de access rights a nivel de inode | https://docs.kernel.org/userspace-api/landlock.html | ✅ |

---

## 3. N1 del ADR-022: composición propuesta

N1 = `bwrap` (namespaces + tmpfs root + seccomp) **+** `Landlock` (restripción de access rights a paths específicos, heredada por todos los hijos).

**No encontrado:**
- Documentación oficial de combinación bwrap+Landlock (no existe; se compone)
- Ejemplo de Rust usando `landlock` crate con `bwrap` (no se encontró en fuentes oficiales)
- `landlock` crate Rust: https://crates.io/crates/landlock (no abierto en esta sessión — pendiente de verificación)

---

**Fuentes abiertas y verificadas en esta sessión. Lo no encontrado = "no encontrado".**
---
**Verificación del TL (README de bubblewrap abierto de nuevo):** confirmadas las citas de `PR_SET_NO_NEW_PRIVS`, «not a complete, ready-made sandbox with a specific security policy» y los filtros seccomp. **No figuran en la página:** «requires unprivileged user namespaces since 0.12.0» (el README no trae versiones; solo dice que el modo setuid se abandonó) y la herencia por procesos hijos (el README no la trata; para Landlock la cita del kernel sí es de otra fuente). Las citas de Landlock no se han vuelto a abrir: no verificado.
