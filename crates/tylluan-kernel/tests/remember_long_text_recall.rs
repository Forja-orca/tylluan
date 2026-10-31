//! B-9 — Pilar Recordar: la memoria recién guardada debe recuperarse.
//!
//! Reportado en vivo el 2026-10-30: se guardó con `tylluan_remember` un
//! resumen de ~3.000 caracteres y el DCR (`handler_remember.rs`, umbral 0,87)
//! lo **fusionó** en un nodo parecido. Después, una consulta casi calcada de
//! su contenido no lo devolvió entre los 5 primeros.
//!
//! Este fichero fija la conducta esperada en dos pruebas:
//!
//! 1. `two_long_texts_stay_separate_and_are_recallable` — dos textos largos
//!    sobre temas distintos deben quedar como nodos propios y cada paráfrasis
//!    debe devolver el suyo entre los 5 primeros. **Falla hoy.**
//! 2. `dcr_merge_table` — no afirma nada: mide para qué longitud y similitud
//!    se activa la fusión y si el texto fusionado sigue siendo recuperable,
//!    e imprime la tabla (20 pares).
//!
//! ## Por qué van con `#[ignore]` y con un gate de entorno
//!
//! Los dos necesitan el motor denso real (el DCR no existe sin `engine()`:
//! `early_embedding` queda en `None` y no se compara nada). Cargar ONNX y un
//! modelo de ~1-2 GB dentro de `cargo test` no es aceptable en CI, así que:
//!
//! ```text
//! TYLLUAN_E2E_EMBEDDINGS=1 cargo test -p tylluan-kernel --test remember_long_text_recall -- --ignored --nocapture
//! ```
//!
//! Sin la variable, cada prueba imprime el motivo y sale sin fallar. Con ella
//! y el modelo en el caché de fastembed, se ejecutan de verdad. El día que el
//! DCR deje de fusionar textos largos, se borra el `#[ignore]` del caso 1 y la
//! prueba debe pasar tal cual, sin tocarla.
//!
//! Modelo: `TYLLUAN_E2E_MODEL` (por defecto `bge-m3`, 1024d, el que ya está en
//! el caché compartido de esta máquina). Nunca mxbai aquí: descargaría 1,25 GB.

use std::sync::Arc;

use tokio::sync::broadcast;

use tylluan_kernel::curriculum::CurriculumLearner;
use tylluan_kernel::doctor::Doctor;
use tylluan_kernel::memory::agent_nodes::AgentNodeRouter;
use tylluan_kernel::memory::mailbox::Mailbox;
use tylluan_kernel::memory::silva::SilvaDB;
use tylluan_kernel::registry::guild_process::GuildRegistry;
use tylluan_kernel::router::catalog::builtin_catalog;
use tylluan_kernel::router::embeddings::EmbeddingEngine;
use tylluan_kernel::router::matcher::GuildMatcher;
use tylluan_kernel::transport::server::TylluanServer;
use tylluan_kernel::transport::server::handler_recall::{RecallCache, handle_tylluan_recall};
use tylluan_kernel::transport::server::handler_remember::handle_tylluan_remember;

// ─────────────────────────── motor denso ───────────────────────────

/// `None` = no se pidió el E2E o el motor no está disponible (se explica y se
/// sale sin fallar). `Some` = motor real cargado.
fn load_engine() -> Option<EmbeddingEngine> {
    if std::env::var("TYLLUAN_E2E_EMBEDDINGS").ok().as_deref() != Some("1") {
        println!(
            "SKIP: falta TYLLUAN_E2E_EMBEDDINGS=1 (el DCR no compara nada sin motor denso)"
        );
        return None;
    }
    let model = std::env::var("TYLLUAN_E2E_MODEL").unwrap_or_else(|_| "bge-m3".to_string());
    match EmbeddingEngine::load(&model) {
        Ok(engine) => {
            println!("motor denso cargado: {model} ({}d)", engine.dimension());
            Some(engine)
        }
        Err(e) => {
            println!("SKIP: no se pudo cargar '{model}': {e}");
            None
        }
    }
}

/// Servidor de prueba con la misma forma que producción en lo que importa al
/// DCR: el matcher instala el motor (así `early_embedding` existe) y SilvaDB
/// reusa ese MISMO `Arc` para la cascada de recall — igual que `main.rs`.
async fn server_with_engine(engine: EmbeddingEngine) -> TylluanServer {
    let registry = GuildRegistry::new(
        std::env::temp_dir().join(format!("tylluan_b9_reg_{}", std::process::id())),
        300,
        Default::default(),
        3,
    );
    let registry = Arc::new(tokio::sync::RwLock::new(registry));

    let mut matcher = GuildMatcher::new(builtin_catalog());
    matcher
        .install_engine(engine, None)
        .expect("install_engine sobre el matcher del test");
    let engine_arc = matcher.engine_arc().expect("el motor queda instalado");

    let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
    silva.init().await.unwrap();
    silva.install_dense_engine(engine_arc);

    let doctor = Arc::new(Doctor::new(
        registry.clone(),
        silva.clone(),
        Arc::new(std::sync::Mutex::new(
            CurriculumLearner::new_in_memory(5).unwrap(),
        )),
    ));
    let (tx, _) = broadcast::channel(16);
    let node_router = AgentNodeRouter::new(tx);

    TylluanServer::new(
        registry,
        Arc::new(matcher),
        silva,
        Arc::new(Mailbox::in_memory().await.unwrap()),
        doctor,
        node_router,
    )
}

// ─────────────────────────── utilidades ───────────────────────────

fn remember_args(text: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
    Some(serde_json::Map::from_iter([(
        "content".to_string(),
        serde_json::Value::String(text.to_string()),
    )]))
}

fn recall_args(query: &str, limit: u64) -> Option<serde_json::Map<String, serde_json::Value>> {
    Some(serde_json::Map::from_iter([
        (
            "query".to_string(),
            serde_json::Value::String(query.to_string()),
        ),
        ("limit".to_string(), serde_json::json!(limit)),
        ("compact".to_string(), serde_json::json!(false)),
    ]))
}

/// Texto que devuelve la herramienta (concatenación de todos los bloques).
fn tool_text(res: &rmcp::model::CallToolResult) -> String {
    res.content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `(id, node_type, longitud del contenido)` de todos los nodos, por SQL: es
/// el único recuento que no depende de que el nodo tenga embedding.
fn node_rows(silva: &SilvaDB) -> Vec<(String, String, i64)> {
    let guard = silva.conn_lock();
    let conn = guard.try_lock().expect("conexión disponible");
    let mut stmt = conn
        .prepare("SELECT id, COALESCE(type, ''), LENGTH(content) FROM nodes ORDER BY id")
        .expect("prepare nodes");
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .expect("query nodes")
        .flatten()
        .collect()
}

/// Similitud coseno entre dos embeddings (el motor ya los devuelve L2).
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

// ─────────────────── textos largos del caso de aceptación ───────────────────

const PREAMBULO: &str = "Contexto del proyecto Tylluan: el kernel atiende en un solo puerto, \
SilvaDB guarda los nodos de memoria, las guilds ejecutan herramientas, el recall combina BM25 \
con embeddings y los agentes hablan por el transport HTTP. Todo corre en el mismo proceso.";

fn texto_backup() -> String {
    let mut s = String::from(PREAMBULO);
    s.push_str(
        "\n\nCopia de seguridad de SilvaDB sin parar el kernel. La orden tylluan-cli backup usa \
VACUUM INTO para obtener una copia consistente leyendo el fichero de origen con su WAL \
incluido, de modo que el kernel sigue escribiendo mientras la copia avanza. El destino lleva \
un manifiesto con nombre, origen y tamaño de cada base. Si el fichero no empieza por la \
cabecera SQLite se salta con un aviso, porque una build cifrada con SQLCipher no produce un \
SQLite plano legible. La restauracion es distinta: exige el kernel parado, hace una foto \
previa en data/pre_restore_<marca> y solo despues sustituye cada destino borrando antes los \
sidecars -wal y -shm, que en otro caso corromperian el fichero restaurado.",
    );
    for i in 0..6 {
        s.push_str(&format!(
            "\n\nDetalle {i}: la copia mide el origen con quick_check antes de aceptarlo; si el \
resultado no es ok se intenta igualmente una copia de rescate, y la restauracion vuelve a \
validar tamaño y cabecera de cada fichero del manifiesto antes de tocar un solo destino. Un \
nombre con separadores de ruta en el manifiesto se rechaza como entrada insegura, y un tamaño \
que no coincide con el anotado se trata como copia truncada."
        ));
    }
    s
}

fn texto_fingerprint() -> String {
    let mut s = String::from(PREAMBULO);
    s.push_str(
        "\n\nHuella de los embeddings y salud del indice vectorial. Cada fila de node_embeddings \
guarda el nombre del modelo y su hash, y el kernel declara una huella esperada contra la que \
juzga cada vector candidato: si la dimension no coincide se cuenta como excluido por dimension \
y si el hash no coincide se cuenta como excluido por modelo. Los contadores son visibles en un \
endpoint de diagnostico y por defecto solo observan, no filtran. Cuando el modelo cambia sin \
avisar, los vectores viejos siguen en la base y el recall semantico devuelve nodos cuyo vector \
no representa el texto actual; por eso el reindexador recalcula y vuelve a guardar la huella.",
    );
    for i in 0..6 {
        s.push_str(&format!(
            "\n\nDetalle {i}: la huella se calcula una sola vez al cargar el motor, a partir de \
la revision del modelo, la dimension, la normalizacion y los ficheros de identidad del snapshot. \
Si el cache de fastembed no se localiza, la huella queda desconocida de forma honesta y el motor \
sigue funcionando, pero los embeddings se guardan sin hash y en el siguiente arranque se marcan \
como anteriores al esquema. Ningun contador borra filas por su cuenta."
        ));
    }
    s
}

// ─────────────────── causa raíz, sin motor denso ───────────────────

/// El DCR no borra el nodo viejo: reescribe su contenido con
/// `update_node_content` (`existing\n\n[actualización] incoming`). Esa función
/// actualiza `nodes.content` y `content_hash`, pero **no toca `nodes_fts`**.
///
/// El índice FTS5 se sincroniza en el mismo transacción en
/// `upsert_node_with_validity`, y `merge_node_into` lo limpia por rowid (D-23),
/// pero el único camino que reescribe contenido en su sitio se quedó fuera.
/// Consecuencia: BM25 sigue viendo el texto viejo, así que el fragmento
/// añadido no es buscable por palabra. Junto con el embedding del nodo
/// fusionado (que para mxbai se trunca a 512 tokens y queda dominado por el
/// texto viejo), el contenido nuevo no lo alcanza ninguno de los dos canales:
/// eso es "la memoria recién guardada no se recupera".
///
/// No necesita motor: reproduce la escritura exacta del DCR y comprueba sólo
/// el canal léxico. Falla hoy.
#[tokio::test(flavor = "multi_thread")]
async fn dcr_merged_content_is_searchable() {
    let silva = Arc::new(SilvaDB::in_memory().await.unwrap());
    silva.init().await.unwrap();

    // El texto viejo no contiene ninguna palabra del fragmento nuevo: así la
    // prueba distingue "lo indexado" de "lo añadido" sin ambigüedad.
    let viejo = "Resumen de la copia de seguridad: el backup usa VACUUM INTO y el kernel \
                 debe estar parado";
    silva.upsert_node("nodo", "memory", viejo, "{}").await.unwrap();

    // Escritura exacta del DCR cuando fusiona (handler_remember.rs).
    let nuevo = "Además se midió el coste de restaurar en frío";
    let fusionado = format!("{viejo}\n\n[actualización] {nuevo}");
    silva.update_node_content("nodo", &fusionado).await.unwrap();

    // El contenido sí se reescribió: esto no es lo que falla.
    let node = silva.get_node("nodo").await.unwrap().expect("el nodo existe");
    assert!(node.content.contains(nuevo), "el content debe llevar lo añadido");

    // 1) El índice FTS debe llevar el texto nuevo (antes seguía con el viejo).
    let indexadas: i64 = {
        let guard = silva.conn_lock();
        let conn = guard.try_lock().expect("conexión disponible");
        conn.query_row(
            "SELECT COUNT(*) FROM nodes_fts WHERE nodes_fts MATCH 'coste'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };

    // 2) El canal léxico debe devolver el nodo por el término añadido.
    //
    // T-DSH-153: aquí se consulta el término marcador ('coste'), no la frase
    // completa. `sanitize_fts_query` construye un AND de todos los tokens
    // ("qué" AND "coste" AND "tiene" AND "el" AND "simulacro"), así que una
    // frase que solo comparte 'el'/'coste' no puede casar por FTS y el fallback
    // LIKE busca la frase entera como subcadena. Esa semántica de consulta es
    // un defecto aparte (cambio de recuperación: requiere A/B), no el
    // desincronizado FTS que arregla esta tarea; con el término marcador se
    // comprueba exactamente que lo añadido quedó indexado y es recuperable.
    let hits = silva.search("coste", 5, None).await.unwrap();
    let ids: Vec<String> = hits.iter().map(|n| n.id.clone()).collect();

    assert!(
        indexadas == 1 && ids.iter().any(|id| id == "nodo"),
        "tras fusionar, 'coste' debe estar en nodes_fts (filas={indexadas}, esperado 1) \
         y la búsqueda debe devolver el nodo (devolvió {ids:?})"
    );
}

// ─────────────────────────── casos de aceptación ───────────────────────────

/// CASO 1 (guarda de regresión, hoy PASA). Dos textos largos de temas
/// distintos con vocabulario de proyecto compartido: cada uno como nodo propio
/// y cada paráfrasis en el top 5.
///
/// Medido el 2026-10-30 con bge-m3 y con mxbai-embed-large (el defecto de
/// producción): en ambos casos quedan dos nodos y las dos paráfrasis vuelven.
/// Es decir, el escenario "temas distintos" que describía la cola NO
/// reproduce el fallo; el disparador real es la segunda escritura sobre el
/// mismo tema (ver `two_long_texts_stay_separate_and_are_recallable` vs
/// `same_topic_addendum_is_recoverable`). Se conserva como guarda: si alguien
/// afloja el umbral o el DCR empieza a fusionar de más, esta prueba lo caza.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requiere motor denso real (TYLLUAN_E2E_EMBEDDINGS=1 + modelo en caché)"]
async fn two_long_texts_stay_separate_and_are_recallable() {
    let Some(engine) = load_engine() else { return };
    let server = server_with_engine(engine).await;

    let a = texto_backup();
    let b = texto_fingerprint();
    println!(
        "texto A: {} chars | texto B: {} chars",
        a.chars().count(),
        b.chars().count()
    );

    let ra = handle_tylluan_remember(&server, remember_args(&a))
        .await
        .unwrap();
    println!(
        "guardar A -> {}",
        tool_text(&ra).lines().next().unwrap_or("")
    );
    let rb = handle_tylluan_remember(&server, remember_args(&b))
        .await
        .unwrap();
    println!(
        "guardar B -> {}",
        tool_text(&rb).lines().next().unwrap_or("")
    );

    let rows = node_rows(&server.silva);
    println!("nodos tras las dos escrituras: {}", rows.len());
    for (id, ty, len) in &rows {
        println!("  - {id} [{ty}] {len} chars");
    }

    // (a) dos nodos propios: el texto B no puede haberse ido dentro de A.
    assert_eq!(
        rows.len(),
        2,
        "los dos textos deben quedar como nodos propios; hay {} nodo(s): {rows:?}",
        rows.len()
    );

    // (b) paráfrasis de cada uno en el top 5.
    let casos = [
        (
            "A",
            "¿Cómo se saca una copia de seguridad de SilvaDB sin parar el kernel y qué se hace con el -wal al restaurar?",
            "sidecars",
        ),
        (
            "B",
            "¿Qué ocurre cuando el hash del modelo de embeddings no coincide con el que se guardó?",
            "huella esperada",
        ),
    ];
    let mut fallos = Vec::new();
    for (etiqueta, consulta, aguja) in casos {
        let res = handle_tylluan_recall(&server, recall_args(consulta, 5))
            .await
            .unwrap();
        let texto = tool_text(&res);
        let encontrado = texto.contains(aguja);
        println!(
            "\nrecall {etiqueta} (limit 5) -> aguja '{aguja}': {}",
            if encontrado { "SÍ" } else { "NO" }
        );
        println!("{}", texto.chars().take(600).collect::<String>());
        if !encontrado {
            fallos.push(format!("{etiqueta} ({aguja})"));
        }
    }
    assert!(
        fallos.is_empty(),
        "paráfrasis que no devuelven su recuerdo en el top 5: {fallos:?}"
    );
}

/// CASO 2 — el que falla hoy, y el que describe el reporte: un texto largo ya
/// guardado y después otro texto largo **del mismo tema** que aporta datos
/// nuevos. El DCR lo fusiona (similitud >= 0,87) y el nodo resultante tiene que
/// seguir devolviendo lo añadido en el top 5.
///
/// Falla por dos canales a la vez:
///   * `update_node_content` no refresca `nodes_fts` → BM25 no ve el añadido
///     (demostrado sin motor en `dcr_merged_content_is_searchable`);
///   * el embedding del nodo fusionado se recalcula sobre el texto completo,
///     que para mxbai se trunca a 512 tokens, así que el vector sigue
///     representando sobre todo el texto viejo.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "B-9 abierto: lo añadido a un nodo fusionado por DCR no se recupera"]
async fn same_topic_addendum_is_recoverable() {
    let Some(engine) = load_engine() else { return };
    let server = server_with_engine(engine).await;
    // Con competición: si el nodo fusionado no entra en el top 5, el marcador
    // desaparece del resultado. Con 1-2 nodos la prueba no mediría nada.
    seed_decoys(&server).await;

    let a = recortar(&texto_backup(), 3000);
    let nuevo = "Además, el ensayo midió el coste de la restauración en frío: 4,3 GB de copia \
                 y sesenta y un segundos de restauración.";
    let b = format!("{a}\n\n{nuevo}");
    println!("texto A: {} chars | texto B (A + añadido): {} chars", a.chars().count(), b.chars().count());

    let _ = handle_tylluan_remember(&server, remember_args(&a)).await.unwrap();
    let rb = handle_tylluan_remember(&server, remember_args(&b)).await.unwrap();
    println!(
        "guardar B -> {}",
        tool_text(&rb).lines().next().unwrap_or("")
    );
    let rows = node_rows(&server.silva);
    println!("nodos: {} {:?}", rows.len(), rows.iter().map(|r| r.2).collect::<Vec<_>>());

    // La consulta sólo puede resolverse con lo añadido: "frío" no aparece en A.
    let res = handle_tylluan_recall(
        &server,
        recall_args("¿qué coste tiene la restauración en frío?", 5),
    )
    .await
    .unwrap();
    let texto = tool_text(&res);
    let recuperable = texto.contains("frío");
    println!("recall del añadido -> {}", if recuperable { "SÍ" } else { "NO" });
    println!("{}", texto.chars().take(700).collect::<String>());

    assert!(
        recuperable,
        "el recuerdo recién guardado (lo añadido al nodo fusionado) debe recuperarse; nodos={}, recall no devolvió 'frío'",
        rows.len()
    );
}

// ─────────────────────────── medición de 20 pares ───────────────────────────

/// Textos cortos de relleno: compiten por el top 5 con el nodo que se mide.
/// Comparten el vocabulario del proyecto (kernel, SilvaDB, guilds, recall) y
/// NUNCA contienen los marcadores de las familias, así que si el marcador
/// aparece en el top 5 es porque el nodo medido entró, no por casualidad.
const DECOYS: [&str; 20] = [
    "El kernel arranca en un solo proceso y expone el estado por HTTP.",
    "Las guilds se descubren del catálogo y se arrancan bajo demanda.",
    "SilvaDB guarda nodos con peso y decae por estabilidad FSRS.",
    "El recall combina BM25 con embeddings y funde los rangos.",
    "Los agentes hablan por el transport HTTP con bearer token.",
    "La federación empuja y tira instantáneas entre pares.",
    "El panel muestra el estado del kernel sin escribir en la base.",
    "Los contratos de trabajo acotado reparten turnos entre agentes.",
    "El juez de escritura revisa lo que entra en memoria.",
    "La telemetría de recall mide latencia y abstenciones.",
    "El índice disperso se calcula en el mismo punto que el denso.",
    "El reindexador agnóstico recalcula vectores viejos.",
    "Las herramientas soberanas son seis y se listan en tools/list.",
    "El journal registra lo que el kernel hizo en cada tarea.",
    "El registro de acciones pendientes pide confirmación del operador.",
    "El limitador por IP protege las rutas del panel.",
    "Los guilds de inferencia tardan segundos en CPU y no se abrevian.",
    "El puerto del kernel es único y no hay proxy delante.",
    "La cuarentena aparta nodos sospechosos sin borrarlos.",
    "El pulso de salud responde antes de que el motor denso esté listo.",
];

/// Siembra el corpus de relleno una sola vez por servidor.
async fn seed_decoys(server: &TylluanServer) {
    for texto in DECOYS {
        let _ = handle_tylluan_remember(server, remember_args(texto))
            .await
            .unwrap();
    }
}

/// Extrae el id del nodo de la respuesta de `tylluan_remember` (nuevo o
/// fusionado), para poder retirarlo entre filas sin tocar los decoys.
fn id_de_la_respuesta(texto: &str) -> Option<String> {
    if let Some(rest) = texto.split("Stored node ").nth(1) {
        let id: String = rest
            .trim_start_matches("memory:")
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if !id.is_empty() {
            return Some(format!("memory:{id}"));
        }
    }
    if let Some(rest) = texto.split("into node '").nth(1) {
        return Some(rest.split('\'').next().unwrap_or_default().to_string());
    }
    None
}

fn borrar_nodos(server: &TylluanServer, ids: &[String]) {
    let guard = server.silva.conn_lock();
    let conn = guard.try_lock().expect("conexión disponible");
    for id in ids {
        let _ = conn.execute("DELETE FROM node_embeddings WHERE node_id = ?1", [id]);
        let _ = conn.execute("DELETE FROM nodes WHERE id = ?1", [id]);
    }
}

fn par_similar() -> (String, String) {
    let a = texto_backup();
    // Mismo tema, un dato nuevo al final: es el caso que se reportó.
    let b = format!(
        "{a}\n\nAdemás, el ensayo midió el coste de la restauración en frío: 4,3 GB de copia \
         y sesenta y un segundos."
    );
    (a, b)
}

fn par_relacionado() -> (String, String) {
    (texto_backup(), texto_fingerprint())
}

fn par_vocabulario_compartido() -> (String, String) {
    let a = texto_backup();
    let b = format!(
        "{PREAMBULO}\n\nEl registro de tareas del runtime expone por HTTP una vista de solo \
lectura con las tareas en curso, su clase de permiso y el instante en que se encolaron, sin \
tocar SilvaDB ni las guilds y sin escribir nada en el journal del kernel."
    );
    (a, b)
}

fn par_distinto() -> (String, String) {
    let a = texto_backup();
    let b = "La receta de bizcocho de limón lleva harina, huevos, azúcar, aceite de oliva suave, \
ralladura de dos limones y levadura. Se mezcla todo, se hornea cuarenta minutos a 180 grados y \
se deja enfriar antes de desmoldar. El glaseado se hace con zumo de limón y azúcar glas hasta \
que queda espeso."
        .to_string();
    (a, b)
}

/// Recorta un texto a `max` caracteres por el final de una frase.
fn recortar(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    match cut.rfind(". ") {
        Some(i) => cut[..i + 1].to_string(),
        None => cut,
    }
}

/// `longitud -> similitud -> fusionó (sí/no) -> recuperable (sí/no)` con 20
/// pares. No afirma nada: la tabla es la evidencia.
///
/// Cada familia trae SU consulta y SU marcador, y el marcador sólo existe en el
/// texto B. El corpus lleva 20 nodos de relleno: sin competición, "recuperable"
/// sería trivial (1-2 nodos siempre caben en un top 5).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "B-9 medición: imprime la tabla de fusión por longitud y similitud (requiere motor denso)"]
async fn dcr_merge_table() {
    let Some(engine) = load_engine() else { return };
    let dimension = engine.dimension();
    println!("motor: {dimension}d");
    let server = server_with_engine(engine).await;
    let engine = server.matcher.engine_arc().expect("motor instalado");

    seed_decoys(&server).await;
    let base = node_rows(&server.silva).len();
    println!("corpus de relleno: {base} nodos");

    let longitudes = [500usize, 1000, 2000, 3000, 4000];
    type Familia = (
        &'static str,
        fn() -> (String, String),
        &'static str,
        &'static str,
    );
    let familias: [Familia; 4] = [
        (
            "mismo-tema",
            par_similar,
            "¿qué coste tiene la restauración en frío?",
            "frío",
        ),
        (
            "relacionado",
            par_relacionado,
            "¿qué pasa si el hash del modelo de embeddings no coincide?",
            "huella esperada",
        ),
        (
            "vocab-compartido",
            par_vocabulario_compartido,
            "¿qué muestra el registro de tareas del runtime?",
            "registro de tareas",
        ),
        (
            "distinto",
            par_distinto,
            "¿qué lleva la receta de bizcocho de limón?",
            "bizcocho",
        ),
    ];

    println!(
        "
{:<18} {:>6} {:>6} {:>9} {:>13}  respuesta de B",
        "familia", "chars", "sim", "fusionó", "recuperable"
    );
    println!("{}", "-".repeat(92));

    for (familia, build, consulta, aguja) in familias {
        for max in longitudes {
            let (a, b) = build();
            let a = recortar(&a, max);
            let b = recortar(&b, max);

            let ea = engine.embed(&a).expect("embed A");
            let eb = engine.embed(&b).expect("embed B");
            let sim = cosine(&ea, &eb);

            let ra = handle_tylluan_remember(&server, remember_args(&a))
                .await
                .unwrap();
            let rb = handle_tylluan_remember(&server, remember_args(&b))
                .await
                .unwrap();
            let tras = node_rows(&server.silva).len();
            let fusiono = tras - base < 2;
            let respuesta_b = tool_text(&rb);
            let marca = if respuesta_b.starts_with("DCR:") {
                "DCR"
            } else {
                "nodo nuevo"
            };

            let res = handle_tylluan_recall(&server, recall_args(consulta, 5))
                .await
                .unwrap();
            let recuperable = tool_text(&res).contains(aguja);

            println!(
                "{:<18} {:>6} {:>6.3} {:>9} {:>13}  {marca} (+{} nodo/s)",
                familia,
                a.chars().count(),
                sim,
                if fusiono { "sí" } else { "no" },
                if recuperable { "sí" } else { "no" },
                tras - base,
            );

            let mut ids: Vec<String> = Vec::new();
            ids.extend(id_de_la_respuesta(&tool_text(&ra)));
            ids.extend(id_de_la_respuesta(&tool_text(&rb)));
            borrar_nodos(&server, &ids);
            *server.recall_cache.lock().await = RecallCache::new(20);
        }
    }
    println!(
        "
consulta y marcador propios de cada familia; el marcador sólo aparece en el texto B."
    );
    println!("top 5 sobre un corpus de {base} nodos de relleno.");
}
