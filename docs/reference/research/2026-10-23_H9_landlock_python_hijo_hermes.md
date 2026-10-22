# H-9 — Landlock: ABI/kernel por regla, herencia al hijo Python, aplicación a Command (Hermes)

Método: cada cita entre comillas angulares es verbatim y localizable con Ctrl+F en la página abierta esta sesión (curl HTTP 200, guardada y grep-eada; comandos al pie). Lo que no es cita va marcado como [inferencia]. Páginas: K=https://docs.kernel.org/userspace-api/landlock.html · R=https://docs.rs/landlock/latest/landlock/enum.ABI.html (landlock 0.4.7) · C=https://docs.rs/landlock/latest/landlock/ · A=https://docs.rs/landlock/latest/landlock/all.html · S=https://doc.rust-lang.org/stable/std/os/unix/process/trait.CommandExt.html · G=https://raw.githubusercontent.com/landlock-lsm/rust-landlock/main/examples/sandboxer.rs

| Pregunta | Cita literal | Página | Estado |
|---|---|---|---|
| ¿Cuándo existe Landlock / requisito de build? | «Landlock was first introduced in Linux 5.13 but it must be configured at build time with CONFIG_SECURITY_LANDLOCK=y» | K | abierto en esta sesión |
| ABI de las reglas de ficheros | «All Landlock kernel interfaces are supported by the first ABI version unless explicitly noted in their documentation.» | K | abierto |
| ABI mínimo de ficheros y su kernel | «First Landlock ABI, introduced with Linux 5.13» | R | abierto |
| ABI de las reglas TCP | «Support added in Landlock ABI version 4» (aparece para BIND_TCP y para CONNECT_TCP) · «Network rules (since ABI v4 for TCP and v10 for UDP)» | K | abierto |
| Kernel mínimo del ABI v4 (TCP) | «Fourth Landlock ABI, introduced with Linux 6.7» · «Fifth Landlock ABI, introduced with Linux 6.10» (v5) | R | abierto |
| ¿Hereda un proceso hijo? | «Every new thread resulting from a clone(2) inherits Landlock domain restrictions from its parent.» · «When a thread sandboxes itself, we have the guarantee that the related security policy will stay enforced on all this thread’s descendants.» | K | abierto |
| ¿Se puede quitar la restricción después? | «Once a thread is landlocked, there is no way to remove its security policy; only adding more restrictions is allowed.» | K | abierto |
| ¿Aplicación a Command existe en el crate? | NO: la página de todos los items (0.4.7) lista exactamente estos traits: Access, Compatible, HandledAccess, RestrictSelfAttr, Rule, RulesetAttr, RulesetCreatedAttr, SyscallFlag — sin CommandPreExecExt. La URL trait.CommandPreExecExt.html de 0.3.17 devuelve HTTP 404 (medido con curl -o /dev/null -w, no inferido). | A | abierto |
| pre_exec en std | «unsafe fn pre_exec» figura como método de CommandExt («Available on Unix only.»); la descripción de la firma no se cita porque el render de tipos con enlaces puede variar. | S | abierto |
| Patrón oficial del crate | El sandboxer monta el ruleset y luego ejecuta: «.add_rules(PortEnv::new(ENV_TCP_BIND_NAME, AccessNet::BindTcp)?.iter())?» y termina en «.exec()»; usa «use std::os::unix::process::CommandExt;» — no usa pre_exec. | G | abierto |

## Q2 — respuesta para el hijo Python (subprocess)
CPython `subprocess.Popen` = clone/fork + execve del binario python. Por las dos citas de herencia (K), el hijo nace dentro del dominio Landlock del padre, y por la tercera, ninguna `execve` intermedia puede quitárselo. **[inferencia, no cita]:** por tanto una guild Python hija queda restringida sin tocar su código — basta aplicar Landlock en el kernel Rust antes de spawn. Análogo citado en K: «This is similar to seccomp inheritance».

## Q3 — aplicación desde Rust a un Command
Con landlock 0.4.7 **no hay** helper de pre_exec ni extensión sobre Command (tabla anterior, fila A). Dos caminos, ambos sin tocar la guild: (a) instalar el ruleset en el proceso kernel ANTES de spawn y confiar en la herencia (patrón sandboxer: builder Ruleset → exec del hijo; G); (b) `CommandExt::pre_exec` (unsafe, S) para aplicar el ruleset solo al hijo entre fork y exec — útil si el padre no quiere restringirse a sí mismo. [inferencia]: (a) es más simple y es lo que demuestra el ejemplo oficial; (b) añade riesgo unsafe entre fork/exec.

## Extras verificados para el ADR-022 (nivel N1)
- Límite de apilado: «There is a limit of 16 layers of stacked rulesets.» (K).
- El crate 0.4.7 va por detrás del kernel documentado: «This crate exposes the Landlock features available as of Linux 7.1» con ABI hasta V9 (R enum), mientras K documenta ya ABI v10 (UDP) y v11 (flag no_new_privs).
- UDP: «Support added in Landlock ABI version 10» (K); la enum R (0.4.7) se queda en V9.

## No encontrado / límites declarados
- La página K consultada NO incluye una tabla ABI→versión de kernel por ABI; el kernel mínimo por ABI viene de la enum del crate (R), no de K.
- Requisitos de privilegios para crear el dominio: la página K consultada usa «the task has CAP_SYS_ADMIN in its namespace or is running with no_new_privs» en el contexto de LANDLOCK_RESTRICT_SELF_NO_NEW_PRIVS; no abrí la sección de permisos completa — marcado como cita parcial.
- Landlock es Linux-only: no encontrado como frase literal en K; [inferencia por ausencia — es un LSM del kernel].

## Verificación (cada cita se comprueba así)
```
curl -sL https://docs.kernel.org/userspace-api/landlock.html | sed 's/<[^>]*>//g' | tr -s '[:space:]' ' ' | grep -F -o "Every new thread resulting from a clone(2) inherits Landlock domain restrictions from its parent."
curl -sL https://docs.rs/landlock/latest/landlock/enum.ABI.html | sed 's/<[^>]*>//g' | tr -s '[:space:]' ' ' | grep -F -o "Fourth Landlock ABI, introduced with Linux 6.7"
curl -s -o /dev/null -w "%{http_code}\n" https://docs.rs/landlock/0.3.17/landlock/trait.CommandPreExecExt.html  # 404
```

## Verificacion del TL (2026-10-23)
Re-ejecutadas con curl+grep -F: la cita de herencia por clone(2) (1 coincidencia), «Fourth Landlock ABI, introduced with Linux 6.7» (1) y el limite de 16 capas (1); `CommandPreExecExt` 0.3.17 devuelve HTTP 404. Las inferencias de Q2/Q3 siguen siendo inferencias (no probadas con un proceso real). Pendiente de probar en Linux: que un hijo Python spawneado tras `restrict_self` quede restringido.
