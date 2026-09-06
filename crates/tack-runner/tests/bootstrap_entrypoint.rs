//! Proves `tack_runner::bootstrap` is a real, external composition root:
//! callable from a crate that only depends on `tack-runner` as a library,
//! and stoppable purely by the `Shutdown` it was handed — no process signal
//! involved, unlike `cli.rs`'s subprocess tests in this same directory.

use std::{
    io::{Read, Write},
    net::TcpListener as StdTcpListener,
    path::PathBuf,
    time::Duration,
};

use tack_runner::{
    ConfigOverrides, EnrollmentCredential, RunnerConfig, RunnerConfigSources, RunnerError,
    Shutdown,
    bootstrap::{RunnerLimits, build_runtime, run},
    harness::process::ProcessLimits,
};

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/contracts/runner-v1")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("fixture {name} is readable"))
}

/// A state directory that removes itself, and everything written under it,
/// when the returned guard drops — including when an assertion panics first.
fn temp_state_dir(label: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(label)
        .tempdir()
        .expect("temporary directory")
}

fn test_limits() -> RunnerLimits {
    RunnerLimits {
        harness_process: ProcessLimits::new(1024, 1024, Duration::from_secs(1)),
        protocol_request_timeout: Duration::from_secs(5),
    }
}

/// A single-exchange HTTP/1.1 mock: accepts one connection, waits for the
/// full request, sleeps `respond_after`, then replies with the frozen
/// `enrollment.response.json` contract fixture. The delay exists so the
/// test can guarantee its shutdown request lands before the reply does —
/// the runtime's first shutdown check happens right after enrollment
/// completes, so this makes the race deterministic instead of timing-luck.
fn spawn_delayed_enrollment_server(respond_after: Duration) -> String {
    let listener = StdTcpListener::bind("127.0.0.1:0").expect("bind mock server");
    let addr = listener.local_addr().expect("mock server address");

    std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 4096];
        loop {
            let read = stream.read(&mut chunk).unwrap_or(0);
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..read]);
            let Some(header_end) = find_subslice(&buffer, b"\r\n\r\n") else {
                continue;
            };
            let head = String::from_utf8_lossy(&buffer[..header_end]).to_string();
            let content_length = head
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            if buffer.len() >= header_end + 4 + content_length {
                break;
            }
        }

        std::thread::sleep(respond_after);

        let body = fixture("enrollment.response.json");
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    });

    format!("http://{addr}")
}

#[tokio::test]
async fn the_composition_root_stops_on_an_injected_shutdown_with_no_process_signal() {
    let state_dir = temp_state_dir("shutdown");
    let base_url = spawn_delayed_enrollment_server(Duration::from_millis(300));
    let config = RunnerConfig::from_sources(RunnerConfigSources {
        command_line: ConfigOverrides {
            api_base_url: Some(base_url),
            runner_id: Some("bootstrap-entrypoint-test".into()),
            state_dir: Some(state_dir.path().to_path_buf()),
            enrollment_credential: Some(EnrollmentCredential::new("test-only-credential")),
            ..ConfigOverrides::default()
        },
        ..RunnerConfigSources::default()
    })
    .expect("test configuration is valid");

    let (shutdown, shutdown_handle) = Shutdown::channel();

    // The proof this test exists for: `run` is a plain public async
    // function reached as `tack_runner::bootstrap::run` from a crate that
    // only sees tack-runner's public API, started here as an ordinary task
    // rather than a subprocess sent a signal.
    let task = tokio::spawn(run(config, test_limits(), shutdown));

    shutdown_handle.request();

    // The claim here is that the runtime stops once shutdown is requested —
    // not that it does so within any particular number of seconds — so this
    // timeout exists only to turn an actual wedge into a reported failure
    // instead of a process that never returns. It has to clear two very
    // different floors at once: comfortably above the delay a machine
    // saturated by the rest of this suite can add to a single OS thread's
    // own scheduling, and comfortably below the point (three minutes) this
    // workspace's own test runner terminates a still-running test outright,
    // which would report a bare process kill instead of this test's own
    // message. The upper floor is the exact one; the lower is not, and
    // cannot be — a single observation of this test taking two orders of
    // magnitude longer than its typical sub-second run is what rules the
    // old budget out, but saturating a many-core machine with spare
    // scheduling headroom does not reproduce it, so no number here is
    // re-measurable on demand. That asymmetry is why this sits near the
    // runner's kill boundary rather than just above any figure: the cost of
    // being too generous is bounded and small, and the cost of being too
    // tight is a test that fails for reasons that are not about the code.
    let outcome = tokio::time::timeout(Duration::from_secs(150), task)
        .await
        .expect("runtime stopped after shutdown was requested, with no signal sent")
        .expect("runtime task did not panic");

    assert!(
        outcome.is_ok(),
        "a shutdown-driven stop must be reported as success, got {outcome:?}"
    );
}

/// `build_runtime` is the composition step alone, usable without ever
/// starting the runtime — this is what lets a future composer inspect or
/// hold onto the built runtime before deciding to run it. It must fail
/// synchronously and fast on a missing credential, before any network call,
/// exactly as `RunnerRuntime::run` already does once started.
#[tokio::test]
async fn build_runtime_fails_fast_on_a_missing_enrollment_credential() {
    let state_dir = temp_state_dir("missing-credential");
    let config = RunnerConfig::from_sources(RunnerConfigSources {
        command_line: ConfigOverrides {
            api_base_url: Some("http://127.0.0.1:1".into()),
            runner_id: Some("bootstrap-entrypoint-test".into()),
            state_dir: Some(state_dir.path().to_path_buf()),
            enrollment_credential: None,
            ..ConfigOverrides::default()
        },
        ..RunnerConfigSources::default()
    })
    .expect("test configuration is valid");

    let result = build_runtime(config, test_limits()).await;

    assert!(
        matches!(result, Err(RunnerError::MissingEnrollmentCredential)),
        "expected a fast, typed failure with no enrollment credential supplied"
    );
}
