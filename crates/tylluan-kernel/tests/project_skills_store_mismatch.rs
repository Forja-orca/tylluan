//! M-BUFFY-14 — Project Skills: el panel mezcla DOS almacenes (hallazgo E-08 de Deep).
//!
//! Reproducción, no arreglo. Cada test afirma lo que el usuario espera de un
//! único listado de habilidades:
//!
//!   1. lo que guardo, lo veo en la lista;
//!   2. lo que veo en la lista, lo puedo abrir;
//!   3. lo que borro, desaparece de la lista;
//!   4. existe una ruta para borrar una habilidad concreta.
//!
//! Hoy las cuatro cosas fallan porque conviven dos almacenes:
//!
//!   - FICHERO  `.agents/skills/<name>.md`  ← GET/POST `/api/v1/skills`
//!   - SILVADB  nodo `skill:project:<hash>:<name>` ← `@skill:save|get|delete|list`
//!
//! El panel (`dashboard/src/components/ProjectSkillsPanel.tsx`) lista por HTTP
//! (fichero), guarda/abre por `tylluan_do` (Silva) y borra por HTTP con
//! fallback a Silva. Ver `docs/internal/IMPL_M-BUFFY-14_buffy_2026-10.md`.
//!
//! Están `#[ignore]` para no teñir la suite normal de rojo. Ejecutar:
//!
//! ```text
//! cargo test -p tylluan-kernel --test project_skills_store_mismatch -- --ignored --nocapture
//! ```
//!
//! Cuando el fix entre (una sola fuente de verdad), se quita el `#[ignore]` y
//! estos tests pasan a ser la regresión permanente del hallazgo.

mod support;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;

/// Los handlers de skills de `api_admin.rs` resuelven rutas RELATIVAS al cwd
/// (`./.agents/skills`, `./.tylluan/skills`, `./skills`), no del `HttpState`.
/// Este fichero es su propio binario de test, así que mover su cwd a un
/// directorio temporal queda contenido en este proceso y no escribe nada en el
/// árbol del repo. Se hace una sola vez: el cwd es global al proceso.
fn skills_workspace() -> &'static std::path::Path {
    static DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!(
            "tylluan_mmb14_skills_{}_{}",
            std::process::id(),
            support::test_counter_next()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_current_dir(&dir).unwrap();
        dir
    })
}

/// El id de nodo que produce `@skill:save` (`handler_do/skill.rs`): hash
/// `acc * 31 + byte` del cwd. Se duplica aquí a propósito — la reproducción
/// tiene que usar exactamente el id de producción o no prueba nada.
fn skill_node_id(name: &str) -> String {
    let root = std::env::current_dir().unwrap();
    let hash = root
        .to_string_lossy()
        .as_bytes()
        .iter()
        .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(*b as u64));
    format!("skill:project:{hash:x}:{name}")
}

/// Metadatos con la forma exacta que escribe `@skill:save`, incluido
/// `project_root`, que es el filtro de `@skill:list`.
fn skill_meta(name: &str) -> String {
    let root = std::env::current_dir().unwrap();
    serde_json::json!({
        "project_root": root.to_string_lossy(),
        "name": name,
    })
    .to_string()
}

/// Guarda una habilidad por la vía del FICHERO, igual que
/// `POST /api/v1/skills` (`.agents/skills/<name>.md`).
fn write_skill_file(name: &str, content: &str) {
    let dir = skills_workspace().join(".agents").join("skills");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{name}.md")), content).unwrap();
}

async fn list_skill_names(state: std::sync::Arc<tylluan_kernel::transport::http::HttpState>) -> Vec<String> {
    let app = support::build_test_app(state);
    let req = Request::builder()
        .uri("/api/v1/skills")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "GET /api/v1/skills debe responder 200");
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    json.as_array()
        .expect("GET /api/v1/skills devuelve un array de nombres")
        .iter()
        .filter_map(|v| v.get("name").and_then(|n| n.as_str()).map(str::to_string))
        .collect()
}

async fn save_skill_over_http(
    state: std::sync::Arc<tylluan_kernel::transport::http::HttpState>,
    name: &str,
    content: &str,
) -> StatusCode {
    let app = support::build_test_app(state);
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/skills")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({ "name": name, "content": content }).to_string(),
        ))
        .unwrap();
    app.oneshot(req).await.unwrap().status()
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. Guardar por MCP (`@skill:save` → Silva) debe verse en la lista del panel
//    (GET /api/v1/skills → fichero).  HOY: invisible.
// ─────────────────────────────────────────────────────────────────────────────
#[tokio::test(flavor = "multi_thread")]
#[ignore = "M-BUFFY-14: dos almacenes — lo guardado en SilvaDB no aparece en GET /api/v1/skills (fichero)"]
async fn skill_saved_through_mcp_shows_up_in_the_http_list() {
    skills_workspace();
    let state = support::test_state("mmb14_t1").await;
    let name = "mmb14_mcp_only";

    // Lo que hace `@skill:save:<name>: <content>`: nodo en SilvaDB.
    state
        .silva
        .upsert_node(&skill_node_id(name), "project_skill", "guardado por MCP", &skill_meta(name))
        .await
        .unwrap();

    // Lo que ve el panel al abrirse: GET /api/v1/skills.
    let names = list_skill_names(state.clone()).await;

    assert!(
        names.iter().any(|n| n == name),
        "M-BUFFY-14: `@skill:save` escribe en SilvaDB pero GET /api/v1/skills \
         (api_admin.rs, project_skills_list_handler) solo lee ficheros de \
         ./.agents/skills, ./.tylluan/skills y ./skills. Una habilidad guardada \
         por el propio panel queda invisible: nombres en la lista = {names:?}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. Guardar por HTTP (fichero) debe poder ABRIRSE por `@skill:get` (Silva).
//    HOY: "Skill '<name>' not found in this project."
// ─────────────────────────────────────────────────────────────────────────────
#[tokio::test(flavor = "multi_thread")]
#[ignore = "M-BUFFY-14: dos almacenes — lo guardado por POST /api/v1/skills (fichero) no se puede abrir por @skill:get (SilvaDB)"]
async fn skill_saved_through_http_can_be_opened_by_get_intent() {
    skills_workspace();
    let state = support::test_state("mmb14_t2").await;
    let name = "mmb14_http_only";

    let status = save_skill_over_http(state.clone(), name, "guardado por HTTP").await;
    assert_eq!(status, StatusCode::OK, "POST /api/v1/skills debe aceptar la habilidad");

    // El fichero sí aparece en la lista (esto es lo que despista: la lista miente
    // al usuario haciéndole creer que la habilidad existe en un solo sitio).
    let names = list_skill_names(state.clone()).await;
    assert!(
        names.iter().any(|n| n == name),
        "precondición: el fichero recién escrito debe salir en la lista, lista = {names:?}"
    );

    // Al pulsar la habilidad, el panel llama `@skill:get:<name>` → SilvaDB.
    let node = state.silva.get_node(&skill_node_id(name)).await.unwrap();

    assert!(
        node.is_some(),
        "M-BUFFY-14: la habilidad '{name}' se guardó como fichero \
         (.agents/skills/{name}.md) y se lista correctamente, pero al abrirla \
         el panel pregunta a SilvaDB por el nodo '{}', que no existe → \
         \"Skill '{name}' not found in this project.\"",
        skill_node_id(name)
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. Borrar una habilidad listada debe quitarla de la lista.
//    HOY: el borrado va a Silva, el .md sigue en disco y sigue listado.
// ─────────────────────────────────────────────────────────────────────────────
#[tokio::test(flavor = "multi_thread")]
#[ignore = "M-BUFFY-14: dos almacenes — @skill:delete borra el nodo de SilvaDB y el .md sigue listado"]
async fn deleting_a_listed_skill_removes_it_from_the_list() {
    skills_workspace();
    let state = support::test_state("mmb14_t3").await;
    let name = "mmb14_delete_me";

    write_skill_file(name, "contenido que debe desaparecer");
    let before = list_skill_names(state.clone()).await;
    assert!(
        before.iter().any(|n| n == name),
        "precondición: el fichero escrito debe listarse, lista = {before:?}"
    );

    // Camino real del panel: DELETE /api/v1/skills/<name> (no existe) y
    // fallback a `@skill:delete:<name>`, que borra el nodo de SilvaDB.
    let deleted = state.silva.delete_node(&skill_node_id(name)).await.unwrap();

    let after = list_skill_names(state.clone()).await;
    assert!(
        !after.iter().any(|n| n == name),
        "M-BUFFY-14: se pidió borrar '{name}'. SilvaDB dijo deleted={deleted} \
         (no había nodo que borrar) y el fichero .agents/skills/{name}.md sigue \
         en disco, así que la habilidad sigue en la lista = {after:?}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. Debe existir la ruta que el panel ya llama para borrar una habilidad.
//    HOY: DELETE /api/v1/skills/<name> → 404 (routes.rs solo registra GET y POST).
// ─────────────────────────────────────────────────────────────────────────────
#[tokio::test(flavor = "multi_thread")]
#[ignore = "M-BUFFY-14: DELETE /api/v1/skills/<name> no está registrada — el panel cae al fallback de @skill:delete"]
async fn delete_route_for_a_single_skill_is_registered() {
    skills_workspace();
    let state = support::test_state("mmb14_t4").await;
    let name = "mmb14_route_probe";

    write_skill_file(name, "para borrar por ruta");

    let app = support::build_test_app(state);
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/v1/skills/{name}"))
        .body(Body::empty())
        .unwrap();
    let status = app.oneshot(req).await.unwrap().status();

    assert!(
        status.is_success(),
        "M-BUFFY-14: el dashboard llama DELETE /api/v1/skills/{name} \
         (dashboard/src/lib/api/memory.ts, deleteProjectSkill) pero esa ruta no \
         existe en api_v1_routes (routes.rs registra get+post en /api/v1/skills). \
         Respuesta real: {status}"
    );
}
