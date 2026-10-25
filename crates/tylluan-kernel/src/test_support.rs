//! Test-only helpers shared by unit tests (this module is not compiled outside
//! `cfg(test)`).

use std::io::Write;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct SharedBuf(Arc<Mutex<Vec<u8>>>);

impl SharedBuf {
    fn contents(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner()))
            .into_owned()
    }
}

impl Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct SharedWriter(SharedBuf);

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for SharedWriter {
    type Writer = SharedBuf;

    fn make_writer(&'a self) -> Self::Writer {
        self.0.clone()
    }
}

/// Runs `f` with a `tracing` subscriber that captures WARN-and-above output and
/// returns what was logged. Thread-local: events emitted from other threads are
/// not captured, so tests must call the code under test on the current thread.
pub(crate) fn capture_warns<F: FnOnce()>(f: F) -> String {
    let buf = SharedBuf::default();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_writer(SharedWriter(buf.clone()))
        .finish();
    tracing::subscriber::with_default(subscriber, f);
    buf.contents()
}
