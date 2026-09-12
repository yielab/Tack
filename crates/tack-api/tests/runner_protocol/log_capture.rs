//! Shared process-global `tracing` capture guard backing every redaction
//! test in this binary (`handlers`, `decisions`, `artifact_events`). Proves
//! that a capturing test's buffer actually receives the real output the
//! production handlers emit. Nothing to run directly: `CaptureGuard::start`
//! installs it on first use.

use std::cell::RefCell;
use std::sync::{Arc, Mutex, Once};

thread_local! {
    /// Set only by `CaptureGuard::start`, and only on the calling test's own
    /// thread; `None` on every other thread in the binary, so the shared
    /// global writer below silently discards everything it's given for
    /// tests that never call it. Never needs to survive a cross-thread hop
    /// mid-`.await`: every `#[tokio::test]` here runs its whole async body
    /// on one dedicated OS thread (the default current-thread flavor).
    static LOG_CAPTURE: RefCell<Option<Arc<Mutex<Vec<u8>>>>> = const { RefCell::new(None) };
}

static GLOBAL_LOG_CAPTURE_INIT: Once = Once::new();

/// Installs the one `tracing` subscriber this test binary ever installs, as
/// the process-wide global default — exactly once, idempotently.
///
/// A bare `tracing::subscriber::set_default` thread-local override was
/// tried first and is flaky (~1 in 10) under `cargo test`'s parallel
/// execution: `tracing`'s callsite-interest cache is process-global, so
/// whichever subscriber is active on whatever thread first touches a given
/// `event!` callsite gets cached forever for it — a thread-local override
/// on a different thread is never consulted again for that callsite. A
/// single global default, installed before any test's first HTTP request,
/// closes the race instead of narrowing it; under nextest (one process per
/// test) the race cannot occur at all, but under `cargo test` it can.
pub(crate) fn ensure_global_log_capture_installed() {
    GLOBAL_LOG_CAPTURE_INIT.call_once(|| {
        let subscriber = tracing_subscriber::fmt()
            .with_writer(GlobalLogWriter)
            .with_max_level(tracing::Level::DEBUG)
            .finish();
        tracing::subscriber::set_global_default(subscriber).expect(
            "GLOBAL_LOG_CAPTURE_INIT guards the only global tracing subscriber \
             this binary ever installs",
        );
    });
}

/// Zero-sized `MakeWriter` for the global subscriber. Every event it's given
/// is routed through the calling thread's `LOG_CAPTURE` slot, so writes from
/// tests that never call `CaptureGuard::start` go nowhere.
struct GlobalLogWriter;

impl std::io::Write for GlobalLogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        LOG_CAPTURE.with(|cell| {
            if let Some(buffer) = cell.borrow().as_ref() {
                buffer.lock().unwrap().extend_from_slice(buf);
            }
        });
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for GlobalLogWriter {
    type Writer = GlobalLogWriter;
    fn make_writer(&'a self) -> Self::Writer {
        GlobalLogWriter
    }
}

/// RAII scope: while alive, real `tracing` output from this thread is
/// collected into the returned buffer instead of being discarded.
pub(crate) struct CaptureGuard;

impl CaptureGuard {
    pub(crate) fn start() -> (Self, Arc<Mutex<Vec<u8>>>) {
        ensure_global_log_capture_installed();
        let buffer = Arc::new(Mutex::new(Vec::new()));
        LOG_CAPTURE.with(|cell| *cell.borrow_mut() = Some(buffer.clone()));
        (Self, buffer)
    }
}

impl Drop for CaptureGuard {
    fn drop(&mut self) {
        LOG_CAPTURE.with(|cell| *cell.borrow_mut() = None);
    }
}
