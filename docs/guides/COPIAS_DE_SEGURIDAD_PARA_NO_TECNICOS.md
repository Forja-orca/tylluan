# Copias de seguridad y restauración explicadas sin tecnicismos

> Para quien usa Tylluan a diario pero no programa. Si buscas el procedimiento
> técnico completo (simulacro mensual, instancia efímera, rutas del manifiesto),
> lee `RUNBOOK_RESTORE.md`, que está en esta misma carpeta.

## 1. Qué es una copia de seguridad de Tylluan

Tylluan guarda tu memoria y el estado del hub en varios ficheros de base de datos
(las "bases"): conversaciones y recuerdos, perfiles de agentes, contratos,
coloquio, auditoría, etc. Una **copia de seguridad** es una carpeta con una
fotografía consistente de esas bases más un fichero `backup-manifest.json` que
describe qué se copió.

- La copia se puede hacer **con Tylluan encendido**: no interrumpe nada.
- Copia las **12 bases** del hub.
- **No** copia tus secretos salvo que se lo pidas explícitamente (ver punto 5).
- Un backup que nunca se ha restaurado no es un backup de verdad: haz una
  prueba de restauración de vez en cuando (punto 6).

## 2. Antes de empezar

1. Abre una terminal (PowerShell en Windows, Terminal en Linux/macOS) en la
   carpeta donde está tu `tylluan.toml` (la carpeta desde la que arrancas
   Tylluan).
2. Comprueba que el programa responde:

   ```
   tylluan-cli --help
   ```

3. Elige una **carpeta de destino** para la copia, fuera del disco donde está
   instalado el programa y, a ser posible, en otro disco o USB. En los ejemplos
   se escribe `<carpeta-de-destino>`: sustitúyelo por la ruta que elijas.

## 3. Hacer la copia

```
tylluan-cli backup <carpeta-de-destino>
```

Ejemplo de cómo se lee: si quieres guardarla en una carpeta llamada
`copias\2026-10-31`, escribe esa ruta en lugar de `<carpeta-de-destino>`.

Qué debe aparecer:

- una línea por cada base copiada (`✅ ... (db, N bytes)`);
- al final, `N database(s) backed up` y la ruta del `backup-manifest.json`.

Si avisa de que una base "not found, skipping", esa base no existía en ese
momento; revisa que estás lanzando el comando desde la carpeta correcta.

## 4. Guardar la copia en un USB

Conecta el USB y copia **la carpeta entera** (no solo los `.db`: el manifiesto
también hace falta). En Windows:

```
xcopy "<carpeta-de-destino>" "<letra-del-USB>:\Tylluan-copia\" /E /I
```

En Linux/macOS:

```
cp -r "<carpeta-de-destino>" "<punto-de-montaje-del-USB>/Tylluan-copia"
```

Sustituye `<letra-del-USB>` o `<punto-de-montaje-del-USB>` por el del USB que
veas en tu equipo. Expulsa el USB de forma segura antes de desconectarlo.

## 5. Secretos: solo si de verdad los necesitas

La copia normal **no** incluye `identity.key` ni `oauth_jwt_secret`. Si quieres
llevarlos (por ejemplo, para mudar el hub a otro equipo), añade el flag:

```
tylluan-cli backup <carpeta-de-destino> --with-secrets
```

- El comando avisa de que la copia contiene secretos.
- **Guarda esa copia solo en un destino de confianza**: nunca en un
  repositorio, ni en un USB compartido, ni en la nube sin cifrar.
- Sin `--with-secrets`, esos dos ficheros no se copian.

## 6. Restaurar una copia

Restaurar sustituye las bases actuales por las de la copia. Antes de tocar nada,
el programa guarda una "foto" del estado actual en una carpeta
`pre_restore_<fecha>`.

### 6.1. Si vas a restaurar sobre el Tylluan que usas a diario

1. Para Tylluan:

   ```
   tylluan-cli stop --port <puerto>
   ```

   (`<puerto>` es el puerto de tu kernel; si no lo cambiaste, es el que use tu
   instalación. `tylluan-cli status` te lo recuerda.)

2. Restaura:

   ```
   tylluan-cli restore <carpeta-de-la-copia> --port <puerto>
   ```

3. Vuelve a arrancar:

   ```
   tylluan-cli start
   ```

### 6.2. Si solo quieres comprobar que la copia sirve (sin tocar tu Tylluan)

Restaura en una carpeta aparte, con un puerto libre:

```
tylluan-cli restore <carpeta-de-la-copia> --target-dir <carpeta-nueva> --port <puerto-libre>
```

Esto no modifica tu hub actual. Es la forma recomendada de probar una copia.

## 7. Qué pasa si Tylluan está encendido

- **Copia (`backup`)**: funciona con Tylluan encendido. No lo pares.
- **Restauración (`restore`)**: se niega si el kernel que usa ese destino está
  vivo. El mensaje es claro e incluye el **puerto** y el **PID** (el número del
  proceso) que tienes que parar, por ejemplo:

  ```
  rechazando restore in situ: el kernel de este DATA_DIR sigue vivo (PID 1234, puerto <puerto>).
  Paralo primero con: tylluan-cli stop --port <puerto>
  ```

- Un Tylluan distinto (otra instalación, otro directorio de datos) no impide
  restaurar: solo bloquea el kernel del destino que estás restaurando.

## 8. Comprobar que la copia está bien

- Abre `backup-manifest.json` con un editor de texto: debe listar las bases y su
  tamaño.
- Haz una restauración de prueba con `--target-dir` (punto 6.2) y comprueba que
  arranca con `tylluan-cli start`.

## 9. Problemas frecuentes

| Mensaje | Qué significa | Qué hacer |
|---|---|---|
| `nothing backed up — no usable source database found` | No encontró bases en la carpeta desde la que lanzaste el comando | Lánzalo desde la carpeta de tu `tylluan.toml` |
| `size mismatch for ...` | La copia está incompleta o se corrompió | Vuelve a hacer el backup y cópiala de nuevo al USB |
| `rechazando restore in situ ...` | El kernel sigue vivo | Páralo con `tylluan-cli stop --port <puerto>` y repite |
| `not found, skipping` | Esa base no existía al hacer la copia | Normal si nunca usaste esa función; si esperabas esa base, revisa la carpeta de trabajo |

## 10. Glosario mínimo

- **Kernel**: el programa de Tylluan que está funcionando y guarda tus datos.
- **DATA_DIR**: la carpeta donde Tylluan guarda sus bases de datos.
- **PID**: número que identifica un programa en marcha; sirve para pararlo.
- **Puerto**: número por el que tu equipo se comunica con Tylluan.
- **Manifiesto**: fichero `backup-manifest.json` que dice qué contiene la copia.
