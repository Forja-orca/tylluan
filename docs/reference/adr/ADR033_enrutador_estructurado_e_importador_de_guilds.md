# ADR-033 — Enrutador estructurado y importador de guilds

- **Estado:** Propuesto (16-nov-2026). Lo propone el TL por mandato del operador; la aprobación formal es del operador.
- **Clase de riesgo:** A (instala y ejecuta código ajeno).

## Contexto

`tylluan_do` traduce lenguaje natural a guild, herramienta y argumentos. Medido en `main`:

- El kernel copia el intent crudo en nueve campos de texto a la vez (`handler_do/prepare.rs:157-159`);
  solo la ruta, la URL y el comando salen de expresiones regulares.
- Sin embeddings (perfil portable) el router compara por subcadena con las descripciones de los guilds
  (`router/mod.rs`). Una prueba externa reportó `NO_GUILD_MATCH` con «list the files».
- En el registro de auditoría local (8775 filas hasta el 1-nov), `filesystem` falla 28 de 59 veces.
- El catálogo se construye al arrancar leyendo `guilds/` (`router/catalog.rs:541`); no hay registro en
  caliente ni importador. Los servidores MCP externos (`[[external_mcp]]`) se despachan por palabras clave.

Quien llama a Tylluan ya es un modelo de lenguaje: reconvertir su prosa en argumentos tipados duplica su trabajo.
Cloudflare (26-sep-2025, 20-feb-2026) y Anthropic (4-nov-2025) llegaron al mismo punto por otro camino:
descubrimiento progresivo y llamadas tipadas.

## Decisión

1. **Estructura primero.** `tylluan_do` acepta `guild`, `tool` y `arguments` tipados. Solo se rellenan los
   parámetros que el esquema del guild declara; lo que falta produce un error correctivo que dice qué falta.
2. **El router es un recuperador.** Con `explain=true` devuelve los 3 mejores guilds con sus esquemas, sin
   ejecutar. Con embeddings usa vectores; sin ellos, BM25 sobre descripción + frases + ejemplos.
3. **Descriptor rico por guild:** descripción, frases disparadoras, palabras negativas, ejemplos de llamada,
   `required_args`, `CAPABILITIES`. Se publica en `list_available_guilds`.
4. **Registro en caliente** del catálogo (recarga sin reiniciar el kernel).
5. **Importador de repos** (opt-in, apagado por defecto):
   cuarentena (`git clone --depth 1`, sin ejecutar nada) → escaneo → propuesta de guild redactada por el agente
   conectado (o pegada por la persona) → compuertas (dependencias con hash, `CAPABILITIES` declaradas, aprobación
   humana por plan mode) → **autoprueba de ruta**: intents sintéticos que comprueban que el guild nuevo se alcanza
   y no roba consultas a los existentes; el informe de precisión decide la activación.
6. **LM de apoyo:** solo como punto de inferencia opcional que el usuario configura. Nunca obligatorio
   (ancla de 1 token/s y tope de 2-3 GB).

## Banco de pruebas (condición para todo lo demás)

Intents reales etiquetados (guild, tool, argumentos esperados). Métricas: % guild correcto, % argumentos
correctos, % `NO_GUILD_MATCH`, antes y después de cada fase, con y sin embeddings. El banco vive fuera del
repositorio público (los intents contienen rutas locales).

## Consecuencias

- Puede añadir una herramienta al contrato de 6 (AUD-07): requiere enmienda formal.
- El importador abre una superficie de instalación de código ajeno: por eso opt-in, cuarentena y aprobación humana.
- Los guilds importados no heredan confianza: cada uno corre bajo el sandbox y la cadena de auditoría actuales.

## Fases

R0 banco de pruebas · R1 argumentos declarados + error correctivo · R2 modo recuperador · R3 registro en caliente ·
R4 importador + autoprueba · R5 caja de importación en el dashboard.

## No verificado

Que el rendimiento medido en el perfil portable represente el uso real con embeddings; el coste de la autoprueba.
