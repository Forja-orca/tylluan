# ADR-034 — Docker Engine y NVIDIA Container Toolkit: instalarlos y mantenerlos, con consentimiento

- **Estado:** Propuesto (22-nov-2026). Lo propone el TL por mandato del operador; la aprobación formal es del operador.
- **Clase de riesgo:** A (instala software con privilegios de administrador y toca el stack grafico de la maquina).
- **Alcance:** **solo diseño.** Este ADR no implementa nada y no fija la version concreta de cada componente.

## Contexto

Hoy Tylluan **usa** Docker (perfil contenedor: `docker-compose.yml`, `Dockerfile`, job `Docker Smoke` del CI)
pero **no lo instala ni lo mantiene**, y lo mismo con el toolkit de NVIDIA para pasar GPUs a los contenedores.
Medido en `main`:

- `docker-compose.yml` declara tres servicios; dos imagenes auxiliares ya estan fijadas por digest desde el
  22-nov (hallazgo `H-20261122-AUDIT-DOCKER-DOCS`), pero **la version de Docker Engine que ejecuta el composees
  la que el operador tenga instalada**, cualquiera.
- El kernel expone `GET /api/v1/doctor` (`api_admin.rs:925`, `api_ops.rs:108`) y el CLI tiene `tylluan doctor`
  (`crates/tylluan-cli/src/main.rs:660`), que **diagnostican** — leer, no instalar.
- La instalacion oficial (`install.sh` / `install.ps1`) instala **Tylluan**, no sus dependencias de sistema.
- Instalar Docker Engine exige **root/Administradores** en Linux/Windows; el toolkit de NVIDIA toca
  repositorios firmados y, en la practica, el driver. Es exactamente la clase de accion que **no** puede
  ocurrir por sorpresa en una maquina que puede estar entrenando algo de cuatro dias.

El operador pidio que Tylluan **lo descargue y lo mantenga actualizado**, en la instalacion o via `doctor`.
Este ADR fija **como** hacerlo sin convertir una herramienta en un instalador silencioso de sistema.

## Decision

1. **Tres niveles, ninguno automatico por defecto.**
   - `tylluan doctor` (lectura): detecta si falta Docker / toolkit, la version instalada, si hay GPUs y si el
     driver las ve. **No instala**: imprime el comando exacto, la version fijada y la fuente.
   - `tylluan doctor --fix <componente>`: **instala o actualiza con consentimiento explicito por componente**
     (Docker y toolkit son dos consentimientos distintos, nunca uno que cubra al otro).
   - `install.sh --with-docker[=<sha256>]`: opt-in en la instalacion, con el mismo consentimiento por escrito.
2. **Version y fuente fijadas, no «latest».** Cada componente tiene una version exacta en una **tabla de
   compatibilidad** versionada en el repo (Docker Engine X, NVIDIA Container Toolkit Y, driver minimo Z, y la
   version de ONNX Runtime / CUDA que Tylluan pinea). El instalador **no** ejecuta `curl | sh` sin verificar:
   o usa el gestor de paquetes de la distro con version exacta, o descarga el script oficial y **comprueba su
   SHA-256 contra el valor del repo** antes de ejecutarlo.
3. **Consentimiento con datos, y registrado.** El prompt dice, para cada componente: que se va a instalar,
   **version exacta**, de donde sale, **que privilegios** pide, que se va a comprobar despues y **como se
   desinstala**. El consentimiento se guarda en el journal de auditoria con el hash del comando ejecutado.
4. **Idempotente y sin degradar.** Si ya esta instalado y es >= la version fijada, **no toca nada**. Si es mas
   nueva, no se baja de version salvo `--allow-downgrade` explicito. Si no hay red, dice que no puede y no
   deja nada a medias.
5. **Verificacion real despues de instalar, no «deberia funcionar».** `docker version`, `nvidia-ctk --version`,
   `nvidia-smi` y un **contenedor de prueba `--gpus all` que ejecute `nvidia-smi`**, con timeout. Si cualquiera
   falla, se reporta el paso exacto y se ofrece la vuelta atras; nunca se declara «instalado» por el codigo de
   salida del instalador.
6. **Sin privilegios no se finge.** Si no hay admin, el comando **no** intenta `sudo` interactivo ni pide
   contrasena: imprime lo que hay que ejecutar y sale con un codigo distinto (3) para que un script lo note.
7. **Nada de esto es requisito para arrancar.** El perfil `portable` y el kernel nativo **no** dependen de
   Docker: si falta, se degrada el perfil contenedor y se dice, sin bloquear el arranque.

## Alternativas consideradas y por que no

- **No hacer nada (estado actual)** — rechazada como *unico* modo: es lo que el operador pide resolver; hoy la
  version de Docker de la maquina es una incognita que nadie comprueba.
- **Instalarlo siempre en la instalacion, sin preguntar** — rechazada: pide admin implicito, rompe sistemas
  gestionados por politicas y convierte un `install.sh` en algo que cambia el sistema operativo.
- **Usar `:latest` / paquete sin fijar del repo de la distro** — rechazada: es la deriva que el propio hallazgo
  `H-20261122-AUDIT-DOCKER-DOCS` acaba de cerrar en las imagenes; repetirla en el Engine seria incoherente.
- **Instalar tambien el driver NVIDIA** — fuera de alcance a proposito: el driver afecta a todo lo que use
  GPU (incluido un entrenamiento en curso). Este ADR cubre **toolkit**, no driver; actualizar driver merece su
  propio ADR con su propio consentimiento y su propia ventana.

## Consecuencias

- `doctor` gana superficie: contrato de salida **estable en JSON** (para que el dashboard lo pinte) y un modo
  `--json` documentado.
- Hay que **mantener la tabla de versiones** por SO y por version de ONNX/CUDA; caduca y alguien tiene que
  revisarla (es deuda explicita, no un detalle).
- **Tests hermeticos obligatorios**: en CI no se instala nada. Se prueba la logica con un ejecutor falso
  (deteccion, consentimiento, negativa sin admin, verificacion posterior) y **una mutacion** que quite el
  consentimiento y ponga el test rojo.
- Coste en el caso bueno: cero (ya instalado -> no toca nada).

## Verificacion (criterios para cuando se implemente)

1. Matriz de casos: ya instalado y a la version / version vieja / version mas nueva / sin admin / sin red /
   GPUs presentes / sin GPUs. Un test por caso, con ejecutor falso.
2. Mutacion: quitar el paso de consentimiento -> el test de «no instala sin consentimiento» se pone rojo.
3. Mutacion: aceptar `curl | sh` sin comprobar hash -> el test de «script no verificado se rechaza» se pone rojo.
4. En una maquina real (fuera de CI, con permiso del operador): `doctor --fix docker` en un sistema sin Docker
   y comprobar que el contenedor de prueba arranca; pegar la salida.

## No verificado / limites de este ADR

- **No decide la version concreta** de Docker ni del toolkit: eso exige medir contra el ONNX Runtime pineado y
  contra el driver de cada maquina. Es el primer trabajo de la implementacion, no de este documento.
- **No cubre Windows/Docker Desktop**: ahi el Engine no se instala por gestor de paquetes y exige Hyper-V/WSL2;
  merece su propia seccion cuando se decida.
- **No cubre actualizar el driver NVIDIA** (ver arriba).
- **No he medido** ninguna maquina sin Docker para este ADR: el contexto son lecturas de codigo (`api_admin.rs`,
  `api_ops.rs`, `main.rs`) y los ficheros de contenedor del repo, no un ensayo de instalacion.
