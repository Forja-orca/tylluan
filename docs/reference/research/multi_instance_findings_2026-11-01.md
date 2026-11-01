# Varias instancias de Tylluan en la misma máquina: lo aprendido (2026-11-01)

Premisa del dueño: tantas instancias como permitan los recursos, nunca limitadas por las instaladas. Prueba: `scripts/multi_instance_check.py`, ejecutada dos veces con N=3 (por DSH y por el TL), kernels de prueba en los puertos 47091 a 47093, cada uno con su `--port`, su configuración y su `TYLLUAN_DATA_DIR`, 2 hilos por instancia. Producción (puerto 47004) no se tocó y siguió sana en las dos ejecuciones.

## Resultado
- 24/24 comprobaciones PASS en ambas ejecuciones; 0 escrituras compartidas fuera de las carpetas de instancia.
- Un recuerdo escrito en una instancia se recupera en ella y no aparece en las otras dos (6 comprobaciones cruzadas).
- Cada kernel se apagó por su PID exacto y liberó su puerto. Nunca se usó `taskkill` por nombre.
- En el código no hay cerrojo de instancia única. `PeerDecision` (main.rs) no toca a un kernel vecino con otro puerto, ni cuando no puede saber su puerto. Con un puerto distinto de 47004, la carpeta de datos se separa sola (`./data-port-<puerto>`).

## Qué cuesta cada instancia
- Cada instancia guarda **su propia copia del modelo de embeddings**: unos 1,4 GB en su `.fastembed_cache` (3 instancias = 4,2 GB). Los datos reales eran 4,3 MB por instancia. El coste es disco, no estado compartido.
- Dos de tres instancias seguían en `warming_up` cuando se consultó `/health` por primera vez (el modelo carga al arrancar). El script espera, pero el arranque simultáneo de varias instancias compite por CPU y disco.
- Escriben además, dentro de su carpeta de trabajo: `data/active_port.json`, `data/dht_peers.json`, `logs/kernel.log` y `registry.json`. Son relativos al directorio actual, así que dos instancias lanzadas desde la misma carpeta se pisarían: **hay que dar a cada instancia su propio directorio de trabajo**, además de puerto y datos.
- Rutas globales que existen en la máquina y el script solo lista (no las escribe): `~/.tylluan` y dos carpetas de `com.tylluan.desktop.feasibility` en AppData. Conviene revisar si alguna llega a escribirse en un uso real.

## Lo que no se probó
Más de 3 instancias; instancias con el dashboard y los guilds de Python en marcha a la vez; el instalador (T-DSH-150); la carga de memoria con las tres respondiendo peticiones reales; SQLCipher. Un Windows limpio no sustituye esta prueba: comprueba dependencias ocultas, no el número de instancias.

## Resultado de T-DSH-174: una caché de modelos compartida
`--cache-mode shared` (por defecto) hace que las instancias de prueba compartan una sola caché de solo lectura (`FASTEMBED_CACHE_DIR`, modo sin conexión). Medido por DSH y repetido por el TL con la máquina quieta, N=3: la caché pasa de unos 4,47 GB (tres copias) a 1.490.898.820 bytes (una), 26/26 comprobaciones PASS, la caché no se modifica durante la prueba, y el arranque (unos 134 a 145 s con tres instancias a la vez) no empeora de forma apreciable. El aislamiento de datos sigue intacto. Una primera repetición del TL (con una semilla de 7 GB por error y otro grupo de kernels corriendo a la vez) terminó con un `ConnectionResetError` en el recall cruzado; la causa no se determinó (la segunda, con la máquina quieta, pasó 26/26). Mientras no se sepa, **no conviene lanzar dos pruebas de varias instancias a la vez**, y el script debe convertir los errores de red en un fallo claro en vez de abortar.

Cifras: mi ejecución del 2026-11-01 con el binario de la rama de integración (`3a692b61`) y la de DSH sobre `75ca6d81`. La carpeta de trabajo de 4,2 GB se borró después, con permiso del dueño.
