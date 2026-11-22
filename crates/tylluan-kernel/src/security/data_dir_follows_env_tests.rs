// ─── D4979: los ficheros de datos siguen al DATA_DIR, no al cwd ────────────────
// H-20261119-JOURNAL-DB-CWD / P1 DOS-INSTANCIAS: una instancia con
// `TYLLUAN_DATA_DIR` propio seguía escribiendo `data/outputs` y
// `data/scheduler_confusion.db` **en el directorio desde el que arrancase**, así que
// dos kernels en la misma máquina se pisaban (medido: el `data/` del cwd aparecía con
// `dht_peers.json`, D4978, y sin arreglar también con estos).
//
// Tests de comportamiento: si alguien vuelve a poner un literal relativo al cwd, el
// path deja de empezar por el DATA_DIR y esto se pone rojo.
#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    // D4984: la cerradura es COMPARTIDA con el resto de tests que mutan entorno
    // (antes cada módulo tenía la suya y no se excluían).

    fn con_data_dir<T>(dir: &str, f: impl FnOnce() -> T) -> T {
        let _guard = crate::test_env::serial();
        let previo = std::env::var("TYLLUAN_DATA_DIR").ok();
        unsafe { std::env::set_var("TYLLUAN_DATA_DIR", dir) };
        let salida = f();
        match previo {
            Some(p) => unsafe { std::env::set_var("TYLLUAN_DATA_DIR", p) },
            None => unsafe { std::env::remove_var("TYLLUAN_DATA_DIR") },
        }
        salida
    }

    fn sin_data_dir<T>(f: impl FnOnce() -> T) -> T {
        let _guard = crate::test_env::serial();
        let previo = std::env::var("TYLLUAN_DATA_DIR").ok();
        unsafe { std::env::remove_var("TYLLUAN_DATA_DIR") };
        let salida = f();
        match previo {
            Some(p) => unsafe { std::env::set_var("TYLLUAN_DATA_DIR", p) },
            None => unsafe { std::env::remove_var("TYLLUAN_DATA_DIR") },
        }
        salida
    }

    #[test]
    fn outputs_root_sigue_al_data_dir() {
        let root = con_data_dir("D:/tmp/instancia-a", crate::registry::outputs::outputs_root);
        assert_eq!(root, PathBuf::from("D:/tmp/instancia-a/outputs"));
    }

    #[test]
    fn scheduler_confusion_sigue_al_data_dir() {
        let p = con_data_dir("D:/tmp/instancia-b", || {
            unsafe { std::env::remove_var("TYLLUAN_CONFUSION_DB") };
            crate::router::scheduler::confusion::confusion_db_path()
        });
        assert_eq!(p, PathBuf::from("D:/tmp/instancia-b/scheduler_confusion.db"));
    }

    /// El primitivo compartido también (es lo que usan journal/contracts/peers…).
    #[test]
    fn data_file_sigue_al_data_dir() {
        let p = con_data_dir("D:/tmp/instancia-c", || {
            crate::config::data_file("audit.db")
        });
        assert_eq!(p, PathBuf::from("D:/tmp/instancia-c/audit.db"));
    }

    /// Control: sin `TYLLUAN_DATA_DIR` todo queda exactamente como antes
    /// (`./data/...`), que es lo que producción usa hoy — el cambio no toca a nadie
    /// que no aísle su instancia.
    #[test]
    fn sin_data_dir_el_default_historico_no_cambia() {
        let root = sin_data_dir(crate::registry::outputs::outputs_root);
        assert_eq!(root, PathBuf::from("./data/outputs"));
        let p = sin_data_dir(|| {
            unsafe { std::env::remove_var("TYLLUAN_CONFUSION_DB") };
            crate::router::scheduler::confusion::confusion_db_path()
        });
        assert_eq!(p, PathBuf::from("./data/scheduler_confusion.db"));
    }
}
