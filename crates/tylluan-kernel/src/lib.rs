//! # TylluanNexus Kernel Library
//!
//! Shared logic, types, and AI engines for the TylluanNexus ecosystem.

// CI's toolchain (rustc 1.98) ships a newer clippy than local dev environments,
// introducing `chunks_exact_to_as_chunks` as a style suggestion (slice::as_chunks
// is a recent stabilization). It fires on ~15 pre-existing f32-from-bytes decode
// sites across memory/silva/*.rs and memory/auto_link.rs, none of which are
// correctness issues — chunks_exact(4) is still valid, just no longer clippy's
// preferred idiom. Allowed at the crate level rather than rewritten piecemeal to
// unblock CI without touching working decode logic; revisit if/when the local
// toolchain catches up and the idiom migration is done deliberately.
#![allow(unknown_lints, clippy::chunks_exact_to_as_chunks)]

pub mod config;
/// D4983: `cleanup` vive en la libreria para que su test corra en `--lib`
/// (antes era `mod cleanup;` del binario y su test no entraba en la suite).
pub mod cleanup;
pub mod workspace;
pub mod logging;
pub mod recall_telemetry;
pub mod consensus;
pub mod security;
pub mod transport;
pub mod registry;
pub mod memory;
pub mod router;
pub mod doctor;
pub mod maintenance;
pub mod curriculum;
pub mod guard;
pub mod runtime;
pub mod hormones;
pub mod tunnel;
pub mod metrics_ring;
#[cfg(feature = "observability")]
pub mod metrics_exporter;
pub mod federation;
pub mod repo_map;
pub mod process_isolation;
pub mod eval;
pub mod mlp;
pub mod models;
pub mod version_check;

#[cfg(test)]
pub(crate) mod test_support;

/// D4984 (H-HER-S-01 de Hermes): **UNA** cerradura para los tests que mutan
/// variables de entorno. Antes cada modulo declaraba su propio `static
/// ENV_LOCK`, asi que las cerraduras no se excluian entre si y dos tests podian
/// pisarse `TYLLUAN_DATA_DIR` en `--lib` (que corre en paralelo). Con `dev_mode`
/// y `DATA_DIR` moviendose bajo los pies, eso es un fallo intermitente que
/// aparece en CI y no en tu maquina.
#[cfg(test)]
pub(crate) mod test_env {
    use std::sync::{Mutex, MutexGuard, OnceLock};

    fn lock() -> &'static Mutex<()> {
        static L: OnceLock<Mutex<()>> = OnceLock::new();
        L.get_or_init(|| Mutex::new(()))
    }

    /// Toma la cerradura global de entorno. Un test que la tenga NO debe
    /// esperar otra: manten el bloque corto.
    pub(crate) fn serial() -> MutexGuard<'static, ()> {
        lock().lock().unwrap_or_else(|p| p.into_inner())
    }

    /// D5028 (`TEST-ENV-LOCK-SISTEMICO`): ratchet sobre la cerradura global de entorno.
    ///
    /// El crate tiene UNA cerradura desde D4984, pero siguen apareciendo tests que
    /// mutan `TYLLUAN_*` sin tomarla, y cada cerradura privada de modulo no excluye a
    /// las demas (fue la causa de D4984 y la reproduje en D5027). Este gate escanea
    /// `src/` y exige que las mutaciones sin cerradura coincidan EXACTAMENTE con la
    /// lista de abajo: una mutacion nueva sin cerradura lo pone rojo, y tambien lo
    /// pone rojo una entrada de la lista que ya fue cerrada y no se borro, asi que la
    /// lista no puede pudrirse.
    ///
    /// Es heuristica textual, no analisis de flujo: mira el cuerpo de cada `fn` buscando
    /// `set_var`/`remove_var` de un literal `TYLLUAN_`, y la considera cubierta si ese
    /// cuerpo toma la cerradura o delega en un wrapper que la toma (como `con_data_dir`).
    /// Sirve para cerrar la puerta a nuevas fugas y dimensionar las pendientes; migrar
    /// las pendientes sigue siendo decision del TL. Las agujas van concatenadas a
    /// proposito: escrita la aguja literal, este fichero la contendria y el gate quedaria
    /// vacio (medido en D5027, `TEST-GATE-AGUJA-LITERAL`).
    #[test]
    fn mutaciones_de_entorno_sin_cerradura_no_crecen() {
        let pone = ["set_var(\"TYLLUAN_"].concat();
        let quita = ["remove_var(\"TYLLUAN_"].concat();
        let cierre = ["ser", "ial()"].concat();
        let pendientes: &[(&str, &str)] = &[
            // Inventario medido por el propio gate el 2026-11-26: 23 funciones del crate
            // mutan `TYLLUAN_*`. D5029 migro 5 de las 9 pendientes a la cerradura global;
            // quedan las 4 de `router/embeddings.rs`, que usan su `ENV_MUTEX` privado y
            // ademas mutan variables que no son TYLLUAN_* (HF_HOME, ORT/OMP/RAYON):
            // migrar esas es decision del TL (`TEST-ENV-LOCK-SISTEMICO`). Cerrar una
            // exige borrarla de aqui, y el gate pone rojo si sobra o falta alguna.
            ("router/embeddings.rs", "d166_la_cache_hf_es_absoluta_compartida_y_hf_home_manda"),
            ("router/embeddings.rs", "d166_sin_pesos_ni_permiso_el_motor_degrade_sin_llamar_a_fastembed"),
            ("router/embeddings.rs", "test_apply_onnx_thread_cap_sets_env_vars"),
            ("router/embeddings.rs", "test_resolve_onnx_thread_cap_precedence"),
        ];

        let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let ficheros = recolectar_rs(&raiz);
        assert!(
            ficheros.len() > 100,
            "el escaneo encontro solo {} ficheros .rs bajo {}: el gate no puede juzgar nada",
            ficheros.len(),
            raiz.display()
        );

        let mut wrappers: Vec<String> = Vec::new();
        for f in &ficheros {
            for (nombre, cuerpo) in funciones_de(f) {
                if cuerpo.contains(&cierre) && cuerpo.contains("impl Fn") {
                    wrappers.push(nombre);
                }
            }
        }

        let mut sin_cerradura: Vec<String> = Vec::new();
        let mut mutadoras = 0usize;
        for f in &ficheros {
            let rel = f
                .strip_prefix(&raiz)
                .unwrap_or(f)
                .to_string_lossy()
                .replace('\\', "/");
            for (nombre, cuerpo) in funciones_de(f) {
                if nombre.is_empty() {
                    continue;
                }
                if !cuerpo.contains(&pone) && !cuerpo.contains(&quita) {
                    continue;
                }
                mutadoras += 1;
                let cubierta = cuerpo.contains(&cierre)
                    || wrappers.iter().any(|w| cuerpo.contains(&format!("{w}(")));
                if !cubierta {
                    sin_cerradura.push(format!("{rel}::{nombre}"));
                }
            }
        }

        let mut esperado: Vec<String> = pendientes
            .iter()
            .map(|(f, n)| format!("{f}::{n}"))
            .collect();
        sin_cerradura.sort();
        esperado.sort();
        let nuevas: Vec<&String> = sin_cerradura
            .iter()
            .filter(|x| !esperado.contains(x))
            .collect();
        let obsoletas: Vec<&String> = esperado
            .iter()
            .filter(|x| !sin_cerradura.contains(x))
            .collect();
        assert!(
            nuevas.is_empty() && obsoletas.is_empty(),
            "\nmutaciones de entorno sin la cerradura global (D5028): {} de {} funciones que mutan TYLLUAN_*.\n  nuevas sin cerrar: {nuevas:?}\n  ya cerradas y siguen en la lista: {obsoletas:?}\n  estado actual: {sin_cerradura:?}\n  (si acabas de cerrar una, sacala de `pendientes`; si acabas de abrir una, tomala con crate::test_env::serial)",
            sin_cerradura.len(),
            mutadoras
        );
    }

    fn recolectar_rs(raiz: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut cola = vec![raiz.to_path_buf()];
        let mut out = Vec::new();
        while let Some(dir) = cola.pop() {
            for entrada in std::fs::read_dir(&dir)
                .unwrap_or_else(|e| panic!("el escaneo no puede leer {}: {e}", dir.display()))
            {
                let p = entrada
                    .unwrap_or_else(|e| panic!("entrada de directorio ilegible: {e}"))
                    .path();
                if p.is_dir() {
                    cola.push(p);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    out.push(p);
                }
            }
        }
        out
    }

    fn funciones_de(ruta: &std::path::Path) -> Vec<(String, String)> {
        let crudo = std::fs::read_to_string(ruta)
            .unwrap_or_else(|e| panic!("no se puede leer {}: {e}", ruta.display()));
        let mut out: Vec<(String, String)> = Vec::new();
        let mut actual: Option<(String, String)> = None;
        for linea in crudo.lines() {
            let t = linea.trim_start();
            if t.starts_with("//") {
                continue;
            }
            let es_fn = t.contains('(')
                && (t.starts_with("fn ")
                    || t.starts_with("pub fn ")
                    || t.starts_with("pub(crate) fn ")
                    || t.starts_with("async fn ")
                    || t.starts_with("pub async fn "));
            if es_fn {
                if let Some((n, cuerpo)) = actual.take() {
                    out.push((n, cuerpo));
                }
                let resto = t.split("fn ").nth(1).unwrap_or("");
                let nombre: String = resto
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                // La firma va DENTRO del cuerpo: es donde estan los parametros de clausura
                // (`impl FnOnce`), y sin ella un wrapper que sostiene la cerradura, como
                // `con_data_dir`, no se reconocia como tal y sus llamadores salian como
                // pendientes (medido en la primera pasada del gate: 11 en vez de 9).
                let mut cuerpo = String::new();
                cuerpo.push_str(linea);
                cuerpo.push('\n');
                actual = Some((nombre, cuerpo));
            } else if let Some((_, cuerpo)) = actual.as_mut() {
                cuerpo.push_str(linea);
                cuerpo.push('\n');
            }
        }
        if let Some((n, cuerpo)) = actual {
            out.push((n, cuerpo));
        }
        out
    }
}
