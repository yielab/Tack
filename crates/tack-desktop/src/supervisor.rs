//! Decides whether to attach to an already-running `tack` server or spawn one,
//! then supervises the spawned process.
//!
//! Everything here talks to the server only over HTTP and to the child process
//! only through pid/kill — it never links `tack-api`, `tack-db`, `tack-orch` or
//! `tack-runner`. Production wires [`SidecarLauncher`] to the real Tauri
//! sidecar; tests wire it to a plain script, so this logic runs without a
//! webview.

use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

pub const DEFAULT_PORT: u16 = 3210;
const HEALTH_TIMEOUT: Duration = Duration::from_secs(15);
const HEALTH_POLL_INTERVAL: Duration = Duration::from_millis(300);
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// The four `TACK_*` variables the sidecar needs, built from the pinned
/// per-OS data root (see [`crate::paths`]).
#[derive(Debug, Clone)]
pub struct ServerFolders {
    pub database_url: String,
    pub storage_dir: PathBuf,
    pub runner_state_dir: PathBuf,
    pub log_file: PathBuf,
}

impl ServerFolders {
    pub fn env_vars(&self) -> Vec<(String, String)> {
        vec![
            ("TACK_DATABASE_URL".to_string(), self.database_url.clone()),
            (
                "TACK_STORAGE_DIR".to_string(),
                self.storage_dir.display().to_string(),
            ),
            (
                "TACK_RUNNER_STATE_DIR".to_string(),
                self.runner_state_dir.display().to_string(),
            ),
            (
                "TACK_LOG_FILE".to_string(),
                self.log_file.display().to_string(),
            ),
        ]
    }
}

/// The body of `GET /api/health`, narrowed to the fields the supervisor reads.
/// Defined here, not imported from `tack-api`, because this crate parses the
/// server's JSON like any other HTTP client — it does not link the server.
#[derive(Debug, Clone, Deserialize)]
pub struct HealthBody {
    pub status: String,
    pub version: String,
}

#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    #[error("no Tack server answered health within {0:?} and none could be started")]
    HealthTimeout(Duration),
    #[error("failed to spawn the sidecar: {0}")]
    SpawnFailed(String),
    #[error("port {0} is already in use by something that is not Tack")]
    PortOccupiedByOther(u16),
    #[error(
        "the server at this port is version {server_version}, older than the {bundled_version} \
         this app bundles; update the server before attaching"
    )]
    OutdatedServer {
        server_version: String,
        bundled_version: String,
    },
}

/// How an already-running (attached) server's version compares to the
/// version bundled with this app. Spawn mode never needs this: a server
/// this app started is always the bundled binary itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionCheck {
    Compatible,
    Outdated,
    /// One or both version strings did not parse as semver. Not evidence of
    /// being outdated -- attaching proceeds rather than refusing on a guess.
    Unknown,
}

/// Compares `server_version` (from the attached server's `/api/health`)
/// against `bundled_version` (this app's own compiled-in version, which
/// tracks the workspace version a release build ships alongside).
pub fn check_server_version(server_version: &str, bundled_version: &str) -> VersionCheck {
    match (
        semver::Version::parse(server_version),
        semver::Version::parse(bundled_version),
    ) {
        (Ok(server), Ok(bundled)) if server < bundled => VersionCheck::Outdated,
        (Ok(_), Ok(_)) => VersionCheck::Compatible,
        _ => VersionCheck::Unknown,
    }
}

/// What the supervisor decided after probing the configured port.
#[derive(Debug)]
pub enum Outcome<P> {
    /// A Tack server already answered `/api/health`. Nothing was started and
    /// this process is never signalled (rule: never stop a server you did not
    /// start).
    Attached { health: HealthBody },
    /// No server answered; one was spawned and is now healthy.
    Started { health: HealthBody, process: P },
}

/// Minimal process-control surface the supervisor needs: enough to shut a
/// spawned server down and notice it exiting on its own, nothing else.
/// Implemented once for the real `tauri_plugin_shell` sidecar and once for a
/// plain `std::process::Child` in tests.
pub trait SidecarHandle {
    fn pid(&self) -> u32;
    /// Hard-kill. Consuming `self` matches `tauri_plugin_shell`'s
    /// `CommandChild::kill`, which does the same.
    fn kill(self) -> std::io::Result<()>;
    /// Non-blocking: `None` while the process is still running. Once this
    /// returns `Some`, later calls may keep returning `None` — the caller is
    /// expected to stop asking once it has consumed the report.
    fn exited(&mut self) -> Option<ExitReport>;
}

/// How a watched process ended, as reported by [`SidecarHandle::exited`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitReport {
    pub code: Option<i32>,
    pub signal: Option<i32>,
}

impl std::fmt::Display for ExitReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.code, self.signal) {
            (Some(code), _) => write!(f, "code {code}"),
            (None, Some(signal)) => write!(f, "signal {signal}"),
            (None, None) => write!(f, "unknown"),
        }
    }
}

/// Spawns the bundled `tack` binary as `serve --with-runner` with the given
/// extra environment on top of `TACK_HOST`/`TACK_PORT`, which the launcher
/// owns because only it knows the sidecar's fixed argv.
pub trait SidecarLauncher {
    type Process: SidecarHandle;
    fn spawn(&self, env: &[(String, String)]) -> Result<Self::Process, SupervisorError>;
}

/// Probes `<base_url>/api/health`. `None` means nothing answered yet
/// (connection refused, timeout, or a non-2xx/unparseable body) — the normal
/// "not up yet" case while waiting, not a hard error.
pub async fn probe_health(client: &reqwest::Client, base_url: &str) -> Option<HealthBody> {
    let url = format!("{base_url}/api/health");
    let response = client
        .get(&url)
        .timeout(Duration::from_secs(2))
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json::<HealthBody>().await.ok()
}

/// True when something accepts a TCP connection on `port` — used only to tell
/// "nothing is listening yet" (spawn) apart from "something is listening but
/// it isn't answering like Tack" (refuse, per the port-conflict rule) before
/// ever attempting to bind there ourselves.
async fn port_has_a_listener(port: u16) -> bool {
    tokio::time::timeout(
        Duration::from_millis(300),
        tokio::net::TcpStream::connect(("127.0.0.1", port)),
    )
    .await
    .map(|r| r.is_ok())
    .unwrap_or(false)
}

/// Attaches if a Tack server already answers on `base_url`; otherwise spawns
/// one through `launcher` and polls until it is healthy or `HEALTH_TIMEOUT`
/// elapses. `port` is the same port `base_url` names — the caller owns
/// building both from one source of truth; this function never binds a port
/// itself, only probes and connects to one. `bundled_version` is this app's
/// own version; an attach whose server reports an older one is refused
/// (`OutdatedServer`) before anything else happens with it.
pub async fn attach_or_start<L: SidecarLauncher>(
    client: &reqwest::Client,
    base_url: &str,
    port: u16,
    launcher: &L,
    folders: &ServerFolders,
    bundled_version: &str,
) -> Result<Outcome<L::Process>, SupervisorError> {
    if let Some(health) = probe_health(client, base_url).await {
        return match check_server_version(&health.version, bundled_version) {
            VersionCheck::Outdated => Err(SupervisorError::OutdatedServer {
                server_version: health.version,
                bundled_version: bundled_version.to_string(),
            }),
            VersionCheck::Compatible | VersionCheck::Unknown => Ok(Outcome::Attached { health }),
        };
    }

    if port_has_a_listener(port).await {
        return Err(SupervisorError::PortOccupiedByOther(port));
    }

    let process = launcher.spawn(&folders.env_vars())?;

    let deadline = tokio::time::Instant::now() + HEALTH_TIMEOUT;
    loop {
        if let Some(health) = probe_health(client, base_url).await {
            return Ok(Outcome::Started { health, process });
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(SupervisorError::HealthTimeout(HEALTH_TIMEOUT));
        }
        tokio::time::sleep(HEALTH_POLL_INTERVAL).await;
    }
}

/// Graceful-then-forceful shutdown of a spawned server: SIGTERM, poll
/// liveness up to `SHUTDOWN_GRACE`, then a hard kill if it is still alive.
/// Always finishes by calling `process.kill()` even when SIGTERM already
/// worked — for a real OS child that is a no-op signal to a process that no
/// longer exists, and it is also this function's only way to reap the handle
/// (`SidecarHandle` has no separate wait/reap method, matching
/// `tauri_plugin_shell::CommandChild`, which reaps internally). A `kill()`
/// error from that final call is only surfaced when the process was still
/// alive going into it — once the graceful path already confirmed exit, an
/// error from the redundant call means nothing.
/// Blocking — call it off the async runtime's worker threads (e.g. via
/// `spawn_blocking`) so it never stalls the event loop for the full grace
/// window.
#[cfg(unix)]
pub fn shutdown<P: SidecarHandle>(process: P) -> std::io::Result<()> {
    let pid = process.pid() as libc::pid_t;
    // SAFETY: `pid` is a process this application itself spawned and is still
    // tracking; signal 0 only checks liveness (delivers nothing), and SIGTERM
    // is the standard graceful-stop request. Both are documented libc calls
    // taking plain integers, with no aliasing or lifetime requirements.
    unsafe {
        libc::kill(pid, libc::SIGTERM);
    }

    let deadline = std::time::Instant::now() + SHUTDOWN_GRACE;
    loop {
        // SAFETY: same pid, same signal-0 liveness check as above.
        let alive = unsafe { libc::kill(pid, 0) } == 0;
        if !alive {
            let _ = process.kill();
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return process.kill();
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(not(unix))]
pub fn shutdown<P: SidecarHandle>(process: P) -> std::io::Result<()> {
    // No SIGTERM equivalent wired for this platform yet; not_measured beyond
    // this hard kill, which is what `CommandChild::kill` already does.
    process.kill()
}

/// Consecutive missed health polls an attached server gets before the watch
/// treats it as unresponsive rather than a single dropped poll.
pub const MISSED_TICKS_BEFORE_UNRESPONSIVE: u8 = 5;

/// Which of the two watch behaviours applies this tick — derived by the
/// caller from whatever [`Outcome`] the supervisor settled on, since a
/// server this app started is watched by its process exiting and one this
/// app attached to is watched by its health answering. `Unknown` covers the
/// window before that outcome is known (the tray's poll loop starts before
/// [`attach_or_start`] resolves).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerKind {
    Unknown,
    Started,
    Attached,
}

/// What the watch has observed so far, carried by the caller from one tick
/// to the next. Not tied to [`ServerKind`] itself — the caller re-derives the
/// kind every tick — so `Unknown` ticks reset it rather than leaving stale
/// counters behind from before the kind was known.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WatchState {
    missing_streak: u8,
    notified: bool,
}

/// Something the watch decided this tick that the caller must act on once:
/// update the tray's status line and show its one dialog. Every other tick
/// returns `None` — most of them, since a healthy server changes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchEvent {
    /// A server this app started has exited. Fires exactly once per exit.
    StartedExited(ExitReport),
    /// A server this app attached to has missed
    /// [`MISSED_TICKS_BEFORE_UNRESPONSIVE`] consecutive health polls. Fires
    /// once per unresponsive episode.
    AttachedUnresponsive,
    /// An attached server that had gone unresponsive answered health again.
    AttachedRecovered,
}

/// Pure: given what the watch believed last tick, which kind of server this
/// tick is watching, whether this tick's health poll answered, and (for a
/// started server) whether the child has exited, returns what to remember
/// next tick and what changed, if anything. Touches no network, dialog or
/// timer — the caller supplies all three inputs from its own poll loop and
/// acts on the event this returns.
pub fn watch_tick(
    previous: WatchState,
    kind: ServerKind,
    health_answered: bool,
    child_exit: Option<ExitReport>,
) -> (WatchState, Option<WatchEvent>) {
    match kind {
        ServerKind::Unknown => (WatchState::default(), None),
        ServerKind::Started => {
            if previous.notified {
                return (previous, None);
            }
            match child_exit {
                Some(report) => (
                    WatchState {
                        notified: true,
                        ..previous
                    },
                    Some(WatchEvent::StartedExited(report)),
                ),
                None => (previous, None),
            }
        }
        ServerKind::Attached => {
            if health_answered {
                if previous.notified {
                    (WatchState::default(), Some(WatchEvent::AttachedRecovered))
                } else {
                    (WatchState::default(), None)
                }
            } else {
                let streak = previous.missing_streak.saturating_add(1);
                if !previous.notified && streak >= MISSED_TICKS_BEFORE_UNRESPONSIVE {
                    (
                        WatchState {
                            missing_streak: streak,
                            notified: true,
                        },
                        Some(WatchEvent::AttachedUnresponsive),
                    )
                } else {
                    (
                        WatchState {
                            missing_streak: streak,
                            notified: previous.notified,
                        },
                        None,
                    )
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;
    use std::process::{Child, Command, Stdio};

    /// A `std::process::Child`-backed [`SidecarHandle`] for tests.
    #[derive(Debug)]
    struct ChildHandle(Child);

    impl SidecarHandle for ChildHandle {
        fn pid(&self) -> u32 {
            self.0.id()
        }

        fn kill(mut self) -> std::io::Result<()> {
            // `Child::kill` only sends the signal; without `wait` the process
            // stays a zombie (a liveness check would still see it as
            // "alive") until reaped. The real sidecar reaps internally, so
            // this compensates only for the plain `std::process::Child` used
            // here.
            self.0.kill()?;
            self.0.wait()?;
            Ok(())
        }

        fn exited(&mut self) -> Option<ExitReport> {
            match self.0.try_wait() {
                Ok(Some(status)) => Some(ExitReport {
                    code: status.code(),
                    #[cfg(unix)]
                    signal: status.signal(),
                    #[cfg(not(unix))]
                    signal: None,
                }),
                _ => None,
            }
        }
    }

    /// Spawns a fixed script (a fake sidecar) that ignores argv/env content
    /// except the port it must bind, so tests need no webview and no real
    /// `tack` binary.
    struct ScriptLauncher {
        script: PathBuf,
        port: u16,
        /// Poisons the next spawn to simulate a launch failure.
        fail_next: bool,
    }

    impl SidecarLauncher for ScriptLauncher {
        type Process = ChildHandle;

        fn spawn(&self, env: &[(String, String)]) -> Result<Self::Process, SupervisorError> {
            if self.fail_next {
                return Err(SupervisorError::SpawnFailed("poisoned for test".into()));
            }
            let mut cmd = Command::new("python3");
            cmd.arg(&self.script)
                .arg(self.port.to_string())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            for (k, v) in env {
                cmd.env(k, v);
            }
            cmd.spawn()
                .map(ChildHandle)
                .map_err(|e| SupervisorError::SpawnFailed(e.to_string()))
        }
    }

    /// Writes a tiny Python HTTP server that answers `/api/health` on the port
    /// given as argv[1], reporting `version`. Python ships on every CI image
    /// this repo already targets; nothing here depends on the real `tack`
    /// binary.
    fn write_fake_sidecar(dir: &std::path::Path, version: &str) -> PathBuf {
        let path = dir.join("fake-tack.py");
        let mut f = std::fs::File::create(&path).unwrap();
        write!(
            f,
            r#"import http.server, json, sys

class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/api/health":
            body = json.dumps({{"status": "ok", "version": "{version}"}}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        else:
            self.send_response(404)
            self.end_headers()

    def log_message(self, *args):
        pass

port = int(sys.argv[1])
http.server.HTTPServer(("127.0.0.1", port), Handler).serve_forever()
"#
        )
        .unwrap();
        path
    }

    fn free_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    fn folders(root: &std::path::Path) -> ServerFolders {
        ServerFolders {
            database_url: format!("sqlite:{}/tack.db?mode=rwc", root.display()),
            storage_dir: root.join("storage"),
            runner_state_dir: root.join("runner"),
            log_file: root.join("logs/tack.log"),
        }
    }

    #[tokio::test]
    async fn spawns_and_becomes_healthy_when_nothing_is_listening() {
        let tmp = tempfile::tempdir().unwrap();
        let script = write_fake_sidecar(tmp.path(), "0.0.0-fake");
        let port = free_port();
        let base_url = format!("http://127.0.0.1:{port}");
        let client = reqwest::Client::new();
        let launcher = ScriptLauncher {
            script,
            port,
            fail_next: false,
        };

        let outcome = attach_or_start(
            &client,
            &base_url,
            port,
            &launcher,
            &folders(tmp.path()),
            "0.0.0-fake",
        )
        .await
        .expect("supervisor should spawn and observe health");

        let (health, process) = match outcome {
            Outcome::Started { health, process } => (health, process),
            Outcome::Attached { .. } => panic!("nothing was listening; must not attach"),
        };
        assert_eq!(health.status, "ok");
        assert_eq!(health.version, "0.0.0-fake");

        let pid = process.pid();
        shutdown(process).expect("shutdown should succeed");

        // SAFETY: liveness probe only, same as the production shutdown path.
        #[cfg(unix)]
        {
            let alive = unsafe { libc::kill(pid as libc::pid_t, 0) } == 0;
            assert!(!alive, "child pid {pid} must not survive shutdown");
        }
    }

    #[tokio::test]
    async fn attaches_without_spawning_when_something_already_answers() {
        let tmp = tempfile::tempdir().unwrap();
        let script = write_fake_sidecar(tmp.path(), "0.0.0-fake");
        let port = free_port();
        let base_url = format!("http://127.0.0.1:{port}");
        let client = reqwest::Client::new();

        // Start a server by hand, as a human operator would before launching
        // the app — the supervisor must attach to it, not spawn a second one.
        let mut hand_started = Command::new("python3")
            .arg(&script)
            .arg(port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        // Wait for the hand-started process to actually be listening.
        for _ in 0..50 {
            if probe_health(&client, &base_url).await.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        let launcher = ScriptLauncher {
            script,
            port,
            fail_next: true, // spawning must never be attempted
        };
        let outcome = attach_or_start(
            &client,
            &base_url,
            port,
            &launcher,
            &folders(tmp.path()),
            "0.0.0-fake",
        )
        .await
        .expect("supervisor should attach");

        match outcome {
            Outcome::Attached { health } => assert_eq!(health.status, "ok"),
            Outcome::Started { .. } => panic!("something already answered; must not spawn"),
        }

        // The hand-started server must still be running — attach never signals it.
        assert!(
            hand_started.try_wait().unwrap().is_none(),
            "attach must not touch a server it did not start"
        );
        hand_started.kill().unwrap();
        let _ = hand_started.wait();
    }

    #[tokio::test]
    async fn reports_spawn_failure_instead_of_hanging() {
        let tmp = tempfile::tempdir().unwrap();
        let script = write_fake_sidecar(tmp.path(), "0.0.0-fake");
        let port = free_port();
        let base_url = format!("http://127.0.0.1:{port}");
        let client = reqwest::Client::new();
        let launcher = ScriptLauncher {
            script,
            port,
            fail_next: true,
        };

        let err = attach_or_start(
            &client,
            &base_url,
            port,
            &launcher,
            &folders(tmp.path()),
            "0.0.0-fake",
        )
        .await
        .expect_err("a poisoned launcher must surface an error, not hang");
        assert!(matches!(err, SupervisorError::SpawnFailed(_)));
    }

    #[tokio::test]
    async fn refuses_to_spawn_when_the_port_is_held_by_something_else() {
        let tmp = tempfile::tempdir().unwrap();
        let script = write_fake_sidecar(tmp.path(), "0.0.0-fake");
        let client = reqwest::Client::new();

        // Something that is not Tack: a bare TCP listener that never answers
        // HTTP at all, let alone `/api/health`. Bound directly to port 0 and
        // held for the rest of the test, never `accept()`-ed — the kernel
        // completes the handshake for any number of connections up to the
        // backlog on its own, which is exactly the "port is open but nothing
        // Tack-shaped is behind it" case this guards. Calling `accept()` here
        // would service one connection and then, on thread exit, close the
        // listening socket out from under the second probe. Binding directly
        // here (rather than `free_port()` followed by a second bind to the
        // same number) avoids a race on a busy host where something else
        // takes the port in the gap between the two binds.
        let raw_listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = raw_listener.local_addr().unwrap().port();
        let base_url = format!("http://127.0.0.1:{port}");

        let launcher = ScriptLauncher {
            script,
            port,
            fail_next: true, // spawning must never be attempted
        };
        let err = attach_or_start(
            &client,
            &base_url,
            port,
            &launcher,
            &folders(tmp.path()),
            "0.0.0-fake",
        )
        .await
        .expect_err("a foreign listener on the port must be refused, not spawned into");
        assert!(matches!(err, SupervisorError::PortOccupiedByOther(p) if p == port));
    }

    #[tokio::test]
    async fn refuses_to_attach_to_a_server_older_than_the_bundled_version() {
        let tmp = tempfile::tempdir().unwrap();
        let script = write_fake_sidecar(tmp.path(), "0.1.0-beta.1");
        let port = free_port();
        let base_url = format!("http://127.0.0.1:{port}");
        let client = reqwest::Client::new();

        // A server started by hand, as before, but running an older release
        // than this app bundles.
        let mut hand_started = Command::new("python3")
            .arg(&script)
            .arg(port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        for _ in 0..50 {
            if probe_health(&client, &base_url).await.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        let launcher = ScriptLauncher {
            script,
            port,
            fail_next: true, // spawning must never be attempted
        };
        let err = attach_or_start(
            &client,
            &base_url,
            port,
            &launcher,
            &folders(tmp.path()),
            "0.1.0-beta.7",
        )
        .await
        .expect_err("an attached server older than the bundled version must be refused");
        assert!(matches!(
            err,
            SupervisorError::OutdatedServer { ref server_version, ref bundled_version }
                if server_version == "0.1.0-beta.1" && bundled_version == "0.1.0-beta.7"
        ));

        // Refusing to use it must not touch it — same rule as any other
        // attach: this process never signals a server it did not start.
        assert!(
            hand_started.try_wait().unwrap().is_none(),
            "refusing an outdated server must not touch it"
        );
        hand_started.kill().unwrap();
        let _ = hand_started.wait();
    }

    #[test]
    fn check_server_version_flags_a_strictly_older_semver() {
        assert_eq!(
            check_server_version("0.1.0-beta.1", "0.1.0-beta.7"),
            VersionCheck::Outdated
        );
    }

    #[test]
    fn check_server_version_orders_prerelease_numbers_numerically() {
        // A naive string compare would put "beta.10" before "beta.9" —
        // semver orders numeric prerelease identifiers as numbers, not text.
        assert_eq!(
            check_server_version("0.1.0-beta.9", "0.1.0-beta.10"),
            VersionCheck::Outdated
        );
        assert_eq!(
            check_server_version("0.1.0-beta.10", "0.1.0-beta.9"),
            VersionCheck::Compatible
        );
    }

    #[test]
    fn check_server_version_treats_equal_and_newer_as_compatible() {
        assert_eq!(
            check_server_version("0.1.0-beta.7", "0.1.0-beta.7"),
            VersionCheck::Compatible
        );
        assert_eq!(
            check_server_version("0.2.0", "0.1.0-beta.7"),
            VersionCheck::Compatible
        );
    }

    #[test]
    fn check_server_version_is_unknown_rather_than_outdated_when_unparseable() {
        assert_eq!(
            check_server_version("not-a-version", "0.1.0-beta.7"),
            VersionCheck::Unknown
        );
        assert_eq!(
            check_server_version("0.1.0-beta.7", "also-not-a-version"),
            VersionCheck::Unknown
        );
    }

    #[test]
    fn started_server_exit_is_reported_exactly_once() {
        let report = ExitReport {
            code: Some(1),
            signal: None,
        };
        let (state, event) = watch_tick(
            WatchState::default(),
            ServerKind::Started,
            true,
            Some(report),
        );
        assert_eq!(event, Some(WatchEvent::StartedExited(report)));

        // The child keeps reporting the same exit (or the caller keeps
        // asking) — already notified, so nothing fires a second time.
        let (_, event) = watch_tick(state, ServerKind::Started, true, Some(report));
        assert_eq!(event, None);
    }

    #[test]
    fn started_server_that_keeps_answering_changes_nothing() {
        let mut state = WatchState::default();
        for _ in 0..10 {
            let (next, event) = watch_tick(state, ServerKind::Started, true, None);
            assert_eq!(event, None);
            state = next;
        }
        assert_eq!(state, WatchState::default());
    }

    #[test]
    fn four_missed_attached_polls_do_not_trigger_unresponsive() {
        let mut state = WatchState::default();
        for _ in 0..4 {
            let (next, event) = watch_tick(state, ServerKind::Attached, false, None);
            assert_eq!(event, None);
            state = next;
        }
    }

    #[test]
    fn five_missed_attached_polls_trigger_unresponsive() {
        let mut state = WatchState::default();
        let mut fired = None;
        for _ in 0..5 {
            let (next, event) = watch_tick(state, ServerKind::Attached, false, None);
            state = next;
            if event.is_some() {
                fired = event;
            }
        }
        assert_eq!(fired, Some(WatchEvent::AttachedUnresponsive));
    }

    #[test]
    fn attached_server_recovers_after_failure_without_a_second_dialog() {
        let mut state = WatchState::default();
        for _ in 0..5 {
            let (next, _) = watch_tick(state, ServerKind::Attached, false, None);
            state = next;
        }
        let (recovered_state, event) = watch_tick(state, ServerKind::Attached, true, None);
        assert_eq!(event, Some(WatchEvent::AttachedRecovered));
        assert_eq!(recovered_state, WatchState::default());

        // A later, fresh episode of missing polls still only needs five
        // ticks to fire again -- recovery must not have left the streak
        // counter or the notified flag stuck.
        let mut state = recovered_state;
        let mut fired = None;
        for _ in 0..5 {
            let (next, event) = watch_tick(state, ServerKind::Attached, false, None);
            state = next;
            if event.is_some() {
                fired = event;
            }
        }
        assert_eq!(fired, Some(WatchEvent::AttachedUnresponsive));
    }

    #[test]
    fn unknown_kind_resets_any_carried_state() {
        let dirty = WatchState {
            missing_streak: 3,
            notified: true,
        };
        let (state, event) = watch_tick(dirty, ServerKind::Unknown, false, None);
        assert_eq!(state, WatchState::default());
        assert_eq!(event, None);
    }
}
