# D-11 — Capa 1 y acciones con consecuencias: diseño, no parche (Deep, 2026-10-20)

**Estado: documento de diseño. Cero cambios de código.** Base `origin/main` = `cad1477`.
Insumos: `docs/internal/IMPL_b4_coherencegate_puerta_escritura_buffy_2026-10.md` (B-4) y
`docs/reference/research/2026-10-20_patrones_de_diseno_contra_inyeccion_de_prompts.md` (TL).

## 1. Punto de partida medido (no mi opinión)

| Dato de B-4 | Valor |
|---|---|
| Capa 1 (lista de subcadenas) sobre 40 inyecciones | **8/40** (20 %) |
| Falsos positivos sobre 40 benignos | **0/40** |
| Evasión completa | español (5), codificación (3), traducción (4), indirecta (8), jailbreak estructurado (4), SQL/código hostil (4) |
| Capa 2 (juez LLM) en el drill | 0 cuarentenas (causas: truncado —ya corregido—, fallos del juez —D-10—, y H2/H5 —cerrados en D-9) |

Lectura correcta de esos números: la capa 1 **no es un filtro**, es un **detector de lo
obvio en inglés literal**. Alargar la lista sube el 8 y, con suerte, no sube el 0 de falsos
positivos… hasta que se añade "ignore" o "system prompt" en español y empieza a morder
documentación técnica legítima (el corpus benigno ya contiene frases sobre prompts).

## 2. Por qué una lista más larga se agota (aritmética, no pesimismo)

Cada categoría evade por un motivo **distinto y estructural**, y cada uno pide una técnica
distinta de la lista de subcadenas:

| Categoría (n) | Por qué evade una lista de subcadenas | Qué la detectaría de verdad |
|---|---|---|
| español (5) | Los patrones están sólo en inglés | Multilingüe, o modelo |
| codificación (3) | El texto no aparece: aparece su codificación | Decodificar antes de mirar (base64/rot13/hex) |
| traducción (4) | Paráfrasis: no hay subcadena común | Modelo, o normalización semántica |
| indirecta (8) | La instrucción va en dato citado ("el documento dice: …") | Modelo (rol/estructura), no léxico |
| jailbreak estructurado (4) | Formato (roles falsos, delimitadores) | Estructura, no palabras |
| SQL/código hostil (4) | No es una instrucción, es una carga | No es asunto de la capa 1: es del consumidor |

Conclusión: la capa 1 puede subir del 20 % al 30-40 % con dos tardes, y seguirá fallando en
todo lo que importa. **Su función debe cambiar de "decidir" a "marcar y enrutar"**, y la
garantía tiene que estar en otro sitio: en lo que el agente puede hacer *después*.

## 3. El principio (cita del artículo, §3.1)

> "once an LLM agent has ingested untrusted input, it must be constrained … so that it is
> impossible for that input to trigger any consequential actions"

Es decir: **la garantía no puede depender de la detección**. Un fallo de la capa 1 debe
cambiar la *calidad del recall*, no desbloquear acciones. Esto es verificable en el código
hoy: no lo cumple (§5).

## 4. Tabla pedida — acción con consecuencias → quién puede dispararla tras leer memoria → límite propuesto → coste en fricción

| Acción con consecuencias | Quién puede dispararla hoy tras leer memoria no confiable | Límite propuesto | Coste en fricción |
|---|---|---|---|
| **Ejecutar shell** (`tylluan_do` → guild `bash`) | Cualquier agente cuyo rol ACL incluya `bash`; el texto leído no aporta nada al gate — el agente ya tenía permiso antes de leer | El rol efectivo **baja** durante el turno en que el contexto contiene contenido no confiable (marcado `untrusted_content` o en cuarentena): `bash` pasa a requerir aprobación fuera de banda | Alto en flujos mixtos (leer Coloquio y luego correr tests). Mitigable: el marcado sólo aplica a memoria de procedencia no confiable, no a la conversación propia |
| **Escribir/borrar ficheros** (`filesystem`) | Ídem, por rol ACL | Igual que `bash`; además, escritura fuera del workspace siempre con aprobación | Medio: la mayoría de escrituras legítimas caen dentro del workspace |
| **Git con efectos remotos** (`git push`, `reset --hard`) | Ídem | Nunca automático tras contenido no confiable; requiere aprobación con identidad distinta (§5.2) | Bajo (es raro y ya es delicado) |
| **Aprobar y ejecutar un plan propio** (`approve_action` con `requestId` de un `plan:true` del mismo agente) | **El propio agente que creó el plan.** `handlers.rs:192-213` resuelve y ejecuta sin comparar quién aprueba con quién pidió (`PendingPlan` guarda `agent_id` pero no se usa) | Exigir **principal distinto**: humano/operador, u otro agente con rol superior. Si coinciden, `403 self_approval` | Medio: rompe el "yo planifico y yo apruebo" que hoy usan los agentes para tareas rutinarias; compensar con `grant_level` para guilds inocuos |
| **Mandarse mensajes entre agentes** (`agent_send_mail`, `nodo_send`, `nodo_broadcast`) | Cualquier agente; el mensaje entra en el contexto de otro agente | Marcar todo mensaje inter-agente como `untrusted_content` en destino (ya hay bandera, no se propaga), y no permitir que un mensaje **por sí solo** active un plan | Bajo, pero cambia la semántica de Coloquio (hoy se trata como conversación de confianza) |
| **Cambiar la propia identidad/persona** (`register_identity`, `agent_set_persona`) | Cualquier agente, y persiste | Requiere aprobación humana; una identidad no puede redefinirse desde dentro de una sesión que ha leído datos no confiables | Bajo: es una acción excepcional |
| **Envenenar el grafo** (`tylluan_graph add_triple`) | Cualquier agente | Aceptado por diseño (es conocimiento), pero todo triplete originado en memoria no confiable se marca y **no** cuenta para decisiones (`tylluan_decide`) | Nulo si el marcado es automático |
| **Escribir memoria** (`tylluan_remember`) | Cualquier agente | Ya limitado por L1/L2 (D-9 añade H2/H5). La capa 2 no debe ser la única barrera: la escritura es de **bajo** riesgo si el consumo está limitado (arriba) | Nulo |
| **Dispatch remoto P2P** (ejecutar tool en otra máquina) | Quien tenga el guild en su rol y el peer lo anuncie | El receptor valida el rol del emisor **y** si el emisor está en modo "contexto no confiable"; sin esa señal, rechaza acciones con efectos | Alto de implementar (contrato entre pares), bajo de usar |
| **`tylluan_decide`** | — | Sin efectos secundarios por invariante (ADR-021) | Nulo |

## 5. Lo que está implementado, a medias, o ausente (verificado en código)

| Pieza | Estado real |
|---|---|
| ACL por rol → guild (`handler_do/mod.rs:744-766`) | **Existe** y es fail-closed si hay roles definidos. Pero es **estática**: no depende de lo que el agente acaba de leer |
| Bandera `untrusted_content` en metadata + banner de texto (`coherence_gate.rs:86-96`, `guilds/core/utils.py:204`) | **Existe, pero sólo se consume en el recall** (penaliza). No se propaga a la capa de acciones |
| Plan + aprobación (`security/grants.rs`, `handlers.rs:192`) | **Existe, pero el aprobador no se compara con el autor** → auto-aprobación posible. El audit lo registra como `human_intervention=true` (`handler_do/mod.rs:1280-1281`), así que **miente sobre quién aprobó** |
| Cuarentena (`quarantined=1`) | **Existe**; D-9 la usa para no citar en resúmenes. No la consulta ningún gate de acciones |
| Límite de egreso de red para `bash` | **Ausente**. Es la vía más directa de exfiltración y la más difícil de limitar sin romper flujos |
| Marcar el contexto del turno como contaminado | **Ausente**. No hay ningún estado por sesión/turno que diga "este agente leyó algo no confiable hace 2 llamadas" |

## 6. Qué debe garantizar la arquitectura (aunque la detección falle)

1. **Nada leído puede, por sí solo, ejecutar.** Toda acción irreversible exige una decisión
   de un principal distinto del que leyó. Se puede fallar la detección al 100 % y esto sigue
   en pie.
2. **El marcado se propaga, no se adivina.** `untrusted_content` debe viajar del nodo al
   contexto del turno, y del turno a la capa de permisos. Sin propagación, la bandera es
   decorativa (estado actual).
3. **La cuarentena debe tener efecto en el consumo, no sólo en la escritura.** Un nodo
   marcado no debería entrar en el top-k de un agente con permisos de acción, o entrar
   explícitamente etiquetado y con el turno degradado.
4. **Auditoría honesta.** Si el aprobador es el autor, el log no puede decir "intervención
   humana". Un audit que se auto-atribuye humanidad no sirve para reconstruir un incidente.
5. **Minimización de contexto** (apoyo a D-6): no persistir salidas de guilds ni texto de
   Coloquio como memoria de confianza; cada pieza de terceros que no se guarda es una
   inyección que no existe.

## 7. Orden propuesto (si el TL lo encarga)

1. **`untrusted_content` propagado al turno + degradación del rol efectivo** (punto 2 y 3
   de §6). Es el único cambio que convierte "detección" en "límite". Sin él, todo lo demás
   son parches.
2. **`403 self_approval` en `approve_action` + audit con el principal real.** Barato,
   verificable con un test de dos tokens, y cierra la auto-aprobación.
3. **Capa 1 como marcador, no como filtro**: normalización Unicode (NFKC, homóglifos),
   decodificación de base64/rot13 antes de mirar, y **siempre** marcar en vez de bloquear
   (bloquear sólo lo que ya bloquea hoy, para no perder el 0 FP). El 8/40 no mejora mucho,
   pero deja de dar falsa sensación de seguridad.
4. **Límite de egreso de red para `bash`**: el más caro y el que más valor tiene. No lo
   propondría sin decisión de producto.

## 8. Lo que NO haría

- Alargar la lista de subcadenas y llamarlo "cerrar la capa 1". Sube el número del corpus y
  no toca la garantía.
- Confiar en la cuarentena como límite: hoy no filtra el consumo, y su disparo depende de un
  juez que falla el 36 % (D-10).
- Bajar umbrales o añadir detección heurística agresiva: el 0/40 de falsos positivos es el
  activo más valioso de la capa 1 y se pierde rápido.

## Impact

- **Código:** ninguno. Documento de diseño.
- **Depredación:** ninguna; no toca ficheros de otros agentes ni configuración.
- **Qué cambia para el usuario:** nada hoy. Si se implementa §7, el agente que lee memoria no
  confiable pierde capacidad de acción en ese turno: es fricción real y hay que decidirla,
  no colarla por la puerta de atrás.
- **Coste de no hacerlo:** mientras la garantía dependa de la detección, el techo es el
  acierto del juez (que además falla el 36 % de las veces por D-10) y el 20 % de la capa 1.
