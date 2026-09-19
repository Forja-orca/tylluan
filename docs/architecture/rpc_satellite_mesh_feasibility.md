# Backend RPC de llama.cpp entre instituciones vía satélite — veredicto de viabilidad

> **Autor:** Claude Code (Tech Lead)
> **Contexto:** José preguntó si el backend RPC de `llama.cpp` permitiría
> compartir cómputo de inferencia entre instituciones distintas (p.ej.
> hospitales) conectadas por enlace satelital — visión: una red de
> donación de GPU ociosa entre centros médicos/de investigación, con el
> mismo principio de confianza binaria ya adoptado para Federación
> (`FederationPeer.approved`, sin contabilidad entre pares).
> **Fecha:** 2026-09-19
> **Estado:** Investigación de viabilidad técnica — veredicto NO-GO para
> el mecanismo evaluado, con una vía alternativa real identificada.
> **Precede a:** la entrada "Inference Mesh" del Desván de la biblia
> (2026-08-23/29), que investigó el mismo backend entre máquinas propias
> de un mismo usuario, nunca entre instituciones ni sobre WAN/satélite.

---

## 1. La pregunta exacta que se investigó

¿Es viable usar el backend RPC de `llama.cpp` (`GGML_RPC=ON`,
`rpc-server` + `llama-server --rpc`) para repartir un mismo modelo entre
máquinas de instituciones distintas, conectadas por enlace satelital
(el caso real: un centro médico remoto con Starlink u otro proveedor
similar)? La pregunta previa del Desván nunca midió esto — asumía red
local o, como mucho, la misma organización.

## 2. El ancho de banda NO es el problema — verificado

El backend RPC no transfiere los pesos del modelo en cada inferencia —
solo el *hidden state* entre el punto de corte de una capa y la
siguiente, que son pocos KB por token. El ancho de banda real de
Starlink en África en 2026 (37-96 Mbps de bajada, 21-28 Mbps de subida
según región, [Ookla vía tech.africa](https://tech.africa/starlink-africa-speeds-ookla/))
sobra por varios órdenes de magnitud para esa carga. **Esta no es la
limitación real** — corrige la intuición inicial de que "sería el
ancho de banda del satélite lo que fallaría".

## 3. La latencia SÍ es descalificante — con umbral concreto documentado

El backend RPC de `llama.cpp` hace una llamada de red **síncrona por
cada operación de tensor remota** — en la práctica, un *round-trip* por
capa, por cada token generado. La propia comunidad de `llama.cpp` (hilo
de discusión del proyecto, `ggml-org/llama.cpp#9136`) documenta el
umbral de forma explícita: **<5ms apenas se nota, 50ms es doloroso,
200ms es inutilizable**. El diseño es correcto para memoria compartida
en red local (10GbE con cable DAC da latencias sub-milisegundo) — no
está pensado para acelerar, solo para poder cargar un modelo que no cabe
en una sola máquina, y paga ese coste con cada token.

**Latencia real de Starlink en las regiones donde vive el caso de uso**
(centro médico remoto, sin estación terrestre cercana): **25-222ms**,
según proximidad a una estación de puerta de enlace — Johannesburgo/
Nairobi mejoran esto sustancialmente tras sus nuevas estaciones locales;
sin estación cercana (ej. RD Congo, 127ms; Liberia, 222ms) el enlace
está directamente en zona "inutilizable" según el propio umbral de la
comunidad del proyecto. **Incluso el mejor caso real (25ms) ya cae en
la zona "doloroso"**, y eso es antes de multiplicar por las decenas de
capas de un modelo por cada token generado.

### Veredicto técnico

**NO-GO físico, no de configuración.** El backend RPC de `llama.cpp`
entre instituciones conectadas por satélite no es viable con la
arquitectura actual del proyecto — la sincronía por capa es
incompatible con cualquier latencia por encima de red local, y el
enlace satelital nunca baja de esos umbrales por física orbital (tiempo
de propagación + saltos de estación), no por calidad de servicio
mejorable con más ancho de banda o mejor hardware en los extremos.
Ninguna optimización de configuración cambia esta conclusión — está en
el diseño síncrono del protocolo mismo, ver la nota de sus propios
mantenedores sobre trabajo pendiente de optimización de overhead de red
(`#8032`), todavía sin resultados de producción.

---

## 4. La vía real — no es el mismo mecanismo, y ya existe en Tylluan

RPC comparte *el cómputo de una misma pasada hacia adelante* entre
máquinas — por eso necesita sincronía de baja latencia: todas las
máquinas participan en generar el mismo token. La visión de fondo de
José (donación de ciclos ociosos entre hospitales/centros de
investigación) **no requiere eso**. Requiere que cada institución corra
su propio modelo completo, localmente, y acepte trabajos de un pool
distribuido de peticiones independientes — el patrón de computación
voluntaria (BOINC y similares): cada unidad de trabajo es autocontenida,
tolerante a cualquier latencia, porque no depende de ningún otro nodo
para completarse.

**Esto no es una pieza nueva que construir desde cero** — es el
mecanismo de despacho A2A/Mesh que Tylluan ya tiene: cualquier peer
aprobado (`FederationPeer.approved`, confianza binaria, cero
contabilidad — el mismo principio que ya rechazó el `CreditLedger`)
puede recibir una tarea de inferencia despachada como cualquier otra
tarea A2A, ejecutarla con su propio `llama-server` local, y devolver el
resultado completo — sin ningún requisito de latencia más allá de "la
petición y la respuesta llegan en algún momento razonable", exactamente
como ya funciona el resto de A2A hoy.

**Diferencia clave frente al Desván "Inference Mesh"**: esa entrada
evaluaba compartir *memoria* entre máquinas propias vía RPC (para
correr un modelo que no cabe en una sola máquina). Esto es compartir
*capacidad de cómputo independiente* entre instituciones distintas vía
despacho de tareas — cada nodo sigue corriendo su propio modelo
completo, nadie comparte memoria de nadie. Son visiones relacionadas
pero técnicamente distintas; no deben fusionarse en el mismo diseño.

---

## 5. Qué falta para que esto sea más que una idea

Nada de lo siguiente está diseñado ni implementado — es la lista de lo
que habría que resolver antes de convertir esto en un contrato real:

1. **Formato del trabajo despachado**: ¿el prompt completo, o solo una
   referencia a una tarea que el nodo receptor ya conoce (por privacidad
   médica — un hospital no debería tener que enviar datos de paciente a
   otro nodo para que corra la inferencia)?
2. **Verificación de resultado**: a diferencia de BOINC (donde el mismo
   trabajo se manda a varios nodos y se compara), ¿cómo se confía en la
   respuesta de un nodo remoto sin volver a ejecutar la misma inferencia
   localmente? Puede no importar para investigación agregada, sí importa
   si el resultado influye en una decisión clínica real.
3. **Modelo compartido**: ¿todos los nodos donantes deben tener el mismo
   modelo cargado, o el despacho incluye qué modelo se necesita y el
   nodo decide si puede servirlo?
4. Ninguna de estas preguntas tiene urgencia — la investigación de hoy
   responde únicamente "¿es RPC el mecanismo correcto?" (no) y "¿existe
   ya la pieza de despacho correcta?" (sí, A2A/Mesh). El diseño concreto
   de un mercado de trabajos de inferencia es un contrato futuro
   separado, no parte de este documento.

---

## 6. Citas y referencias verificadas

1. **Umbral de latencia RPC de `llama.cpp`** — discusión oficial del
   proyecto: [github.com/ggml-org/llama.cpp/discussions/9136](https://github.com/ggml-org/llama.cpp/discussions/9136),
   corroborado por benchmarks independientes de red local
   ([SharedLLM](https://sharedllm.org/blog/llama-cpp-rpc-distributed-inference.html)).
2. **Diseño del backend RPC** (pooling de memoria, no aceleración de
   cómputo): [llama.cpp/tools/rpc/README.md](https://github.com/ggml-org/llama.cpp/blob/master/tools/rpc/README.md).
3. **Starlink en África, Q1 2026** (velocidad y latencia real por
   región): [Ookla vía tech.africa](https://tech.africa/starlink-africa-speeds-ookla/),
   [Developing Telecoms](https://developingtelecoms.com/telecom-technology/satellite-communications-networks/20393-starlink-beats-sub-saharan-african-isps-on-download-speeds-but-not-latency-ookla.html).
4. **Precedente rechazado, no repetido aquí**: `CreditLedger` (José,
   2026-08-08) — rechazo de contabilidad entre pares, sustituido por
   confianza binaria vía `FederationPeer.approved`. Ver
   `docs/architecture/PROPOSAL_distributed_inference_credit_mesh.md`.
