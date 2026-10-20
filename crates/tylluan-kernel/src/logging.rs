//! Rotación de logs por tamaño (D-4).
//!
//! `./logs/kernel.log` crecía sin límite (1,3 GB observados en un kernel real).
//! Este writer rota por tamaño y conserva un número acotado de ficheros:
//!
//! ```text
//! kernel.log        ← vivo (se escribe aquí)
//! kernel.log.1      ← el anterior
//! kernel.log.2      ← el de antes
//! ...
//! kernel.log.{max_files-1}
//! ```
//!
//! Reglas:
//! - **Nunca parte una línea**: si una entrada no cabe en el hueco que queda, se
//!   escribe entera en el fichero actual y la rotación ocurre en la siguiente
//!   escritura. Un registro de más de `max_bytes` vive solo en su fichero.
//! - Un fichero que ya nace por encima del límite (el caso real de 1,3 GB) se
//!   rota al abrirlo, antes de escribir nada nuevo.
//! - Si la rotación falla (permisos, fichero bloqueado en Windows), el writer
//!   **sigue escribiendo** en el fichero actual y cuenta el fallo: preferimos un
//!   log demasiado grande a perder líneas.
//! - `max_files = 1` desactiva el historial: se vacía el fichero vivo al rotar.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Writer con rotación por tamaño, pensado para `tracing_subscriber`.
pub struct RotatingFileWriter {
    dir: PathBuf,
    base: String,
    max_bytes: u64,
    max_files: usize,
    file: File,
    written: u64,
    rotation_failures: u64,
}

impl RotatingFileWriter {
    /// Abre (o crea) `dir/base` en modo append con rotación por tamaño.
    ///
    /// `max_bytes = 0` desactiva la rotación (comportamiento histórico). El
    /// límite es aproximado: se comprueba antes de cada escritura, así que un
    /// fichero puede superar `max_bytes` solo por el tamaño de un registro
    /// completo.
    pub fn open(dir: &Path, base: &str, max_bytes: u64, max_files: usize) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let live = dir.join(base);
        let mut writer = Self {
            dir: dir.to_path_buf(),
            base: base.to_string(),
            max_bytes,
            max_files: max_files.max(1),
            file: OpenOptions::new().create(true).append(true).open(&live)?,
            written: fs::metadata(&live).map(|m| m.len()).unwrap_or(0),
            rotation_failures: 0,
        };
        if writer.max_bytes > 0 && writer.written > writer.max_bytes {
            writer.rotate()?;
        }
        Ok(writer)
    }

    /// Tamaño actual del fichero vivo, en bytes.
    pub fn current_len(&self) -> u64 {
        self.written
    }

    /// Rotaciones fallidas desde el arranque. No es fatal: el writer sigue
    /// escribiendo en el fichero actual.
    pub fn rotation_failures(&self) -> u64 {
        self.rotation_failures
    }

    fn path_for(&self, n: usize) -> PathBuf {
        self.dir.join(format!("{}.{n}", self.base))
    }

    fn rotate(&mut self) -> io::Result<()> {
        if self.max_files <= 1 {
            // Truncar sobre el handle abierto no es fiable en Windows; se
            // recrea el fichero y se reabre en modo append.
            let live = self.dir.join(&self.base);
            self.file = File::create(&live)?;
            self.file = OpenOptions::new().append(true).open(&live)?;
            self.written = 0;
            return Ok(());
        }
        let last = self.max_files - 1;
        // Se borra el más antiguo y se desplaza el resto de mayor a menor para
        // no pisar ficheros que aún hay que mover (rename no sobrescribe en
        // Windows).
        let _ = fs::remove_file(self.path_for(last));
        for i in (1..last).rev() {
            let from = self.path_for(i);
            if from.exists() {
                let to = self.path_for(i + 1);
                let _ = fs::remove_file(&to);
                fs::rename(&from, &to)?;
            }
        }
        let live = self.dir.join(&self.base);
        let first = self.path_for(1);
        let _ = fs::remove_file(&first);
        fs::rename(&live, &first)?;
        self.file = OpenOptions::new().create(true).append(true).open(&live)?;
        self.written = 0;
        Ok(())
    }
}

impl Write for RotatingFileWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.max_bytes > 0
            && self.written > 0
            && self.written.saturating_add(buf.len() as u64) > self.max_bytes
            && self.rotate().is_err()
        {
            self.rotation_failures += 1;
        }
        let n = self.file.write(buf)?;
        self.written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lee todas las líneas del historial, del más antiguo al vivo.
    fn all_lines(dir: &Path, max_files: usize) -> Vec<String> {
        let mut files: Vec<PathBuf> = (1..max_files).rev().map(|i| dir.join(format!("kernel.log.{i}"))).collect();
        files.push(dir.join("kernel.log"));
        let mut out = Vec::new();
        for f in files {
            if let Ok(content) = fs::read_to_string(&f) {
                out.extend(content.lines().map(|l| l.to_string()));
            }
        }
        out
    }

    #[test]
    fn rotates_at_size_and_preserves_every_line() {
        let dir = tempfile::tempdir().unwrap();
        // 100 líneas de ~41 B ≈ 4,1 KB; con max_files=30 la retención (~12 KB)
        // cubre todas, así que la prueba de preservación es exacta.
        let mut w = RotatingFileWriter::open(dir.path(), "kernel.log", 400, 30).unwrap();
        let expected: Vec<String> = (0..100).map(|i| format!("line-{i:03}-{}", "x".repeat(20))).collect();
        for line in &expected {
            writeln!(w, "{line}").unwrap();
        }
        w.flush().unwrap();

        // Rotó: hay más de un fichero y ninguno supera el límite (cada registro
        // es menor que él, así que la comprobación previa basta).
        let mut count = 0;
        for i in 1..30 {
            if dir.path().join(format!("kernel.log.{i}")).exists() {
                count += 1;
            }
        }
        assert!(count >= 1, "expected rotation, saw {count} rotated files");
        for f in std::fs::read_dir(dir.path()).unwrap() {
            let meta = f.unwrap().metadata().unwrap();
            assert!(meta.len() <= 400, "file exceeds max_bytes: {}", meta.len());
        }

        // Sin pérdida y sin líneas partidas, en orden.
        let got = all_lines(dir.path(), 30);
        assert_eq!(got, expected, "every line must survive, intact and in order");
    }

    #[test]
    fn keeps_at_most_max_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = RotatingFileWriter::open(dir.path(), "kernel.log", 200, 3).unwrap();
        for i in 0..200 {
            writeln!(w, "n{i:03}-{}", "y".repeat(18)).unwrap();
        }
        w.flush().unwrap();

        let files: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(files.len(), 3, "live + 2 rotated, saw {files:?}");
        // Las últimas líneas están; las primeras ya no (retención acotada).
        let got = all_lines(dir.path(), 3);
        assert_eq!(got.last().unwrap(), "n199-yyyyyyyyyyyyyyyyyy");
        assert!(!got.contains(&"n000-yyyyyyyyyyyyyyyyyy".to_string()));
    }

    #[test]
    fn oversized_record_is_never_split() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = RotatingFileWriter::open(dir.path(), "kernel.log", 256, 3).unwrap();
        w.write_all(b"small\n").unwrap();
        let big = format!("BIG-{}-END\n", "z".repeat(1000));
        w.write_all(big.as_bytes()).unwrap();
        w.write_all(b"after\n").unwrap();
        w.flush().unwrap();

        let got = all_lines(dir.path(), 3);
        assert!(got.contains(&big.trim_end().to_string()), "the big record must survive intact");
        assert!(got.iter().any(|l| l == "after"));
    }

    #[test]
    fn existing_oversized_file_is_rotated_on_open() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("kernel.log"), "x".repeat(2000)).unwrap();
        let w = RotatingFileWriter::open(dir.path(), "kernel.log", 512, 5).unwrap();
        assert_eq!(w.current_len(), 0, "live file starts empty after startup rotation");
        assert_eq!(fs::metadata(dir.path().join("kernel.log.1")).unwrap().len(), 2000);
        drop(w);
    }

    #[test]
    fn single_file_mode_truncates_instead_of_keeping_history() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = RotatingFileWriter::open(dir.path(), "kernel.log", 64, 1).unwrap();
        w.write_all("a".repeat(60).as_bytes()).unwrap();
        w.write_all("b".repeat(60).as_bytes()).unwrap();
        w.write_all(b"\n").unwrap();
        w.flush().unwrap();
        assert!(!dir.path().join("kernel.log.1").exists());
        let live = fs::read_to_string(dir.path().join("kernel.log")).unwrap();
        assert!(live.len() <= 64, "live file must not grow past the limit: {}", live.len());
    }
}
