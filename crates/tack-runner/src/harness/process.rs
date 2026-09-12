//! Bounded, cancellable child-process supervision.
//!
//! This is the seam a concrete [`super::HarnessAdapter`] (`engine::HarnessAdapter`)
//! implementation composes inside `validate`/`start`/`cancel`/`wait`: it owns
//! spawning the harness CLI, capturing its stdout/stderr under a hard memory
//! bound, enforcing a timeout, and killing the *entire* descendant process
//! tree on cancellation — not just the direct child, which a plain
//! `Child::kill()` would leave behind. A long-running harness must never
//! buffer an entire run in memory, and a cancelled attempt must not leave
//! orphaned descendants running.
//!
//! ## Process-group cancellation
//!
//! On Unix, [`ProcessSpec::spawn`] places the child in a **new** process
//! group whose id equals the child's own pid (`process_group(0)`, mirroring
//! `setsid`-style detachment from the runner's own group). Any descendant the
//! child spawns without calling `setpgid` itself inherits that same group.
//! Cancellation sends the signal to the *group* (`kill(-pgid, sig)`), which
//! is why it also reaches grandchildren. This needs exactly one raw `libc`
//! symbol (`kill(2)`); see the module-level note on why that is declared via
//! a bare `extern "C"` block instead of adding the `libc` crate as a
//! dependency.
//!
//! Non-Unix targets fall back to killing only the direct child
//! (`tokio::process::Child::kill`), matching the same best-effort pattern
//! already used for non-Unix permissions in `workspace.rs`/`journal.rs`; this
//! is a documented limitation, not a silent gap.

use std::{collections::BTreeMap, path::PathBuf, time::Duration};

use thiserror::Error;
use tokio::{
    io::AsyncReadExt,
    process::{Child, Command},
    time,
};

use super::redact::{RedactedEnv, SecretMaterial};

/// What to launch and where. `working_directory` must be the attempt's own
/// workspace path (or a descendant of it) — [`ProcessSpec::spawn`] refuses to
/// start a process whose working directory escapes `workspace_root`, which is
/// the structural half of "adapters cannot cross-read each other's
/// workspaces" (the empirical half is that each attempt's process only ever
/// receives its own workspace path as `current_dir`, so a relative read never
/// resolves into a sibling attempt's files).
#[derive(Clone)]
pub struct ProcessSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// The child's *complete* environment. Spawning always starts from a
    /// cleared environment (never the runner's own) — see the module docs on
    /// why silently inheriting the host environment would itself be a rule-12
    /// leak. The adapter must include everything the harness needs (`PATH`
    /// included, if it shells out).
    pub env: BTreeMap<String, String>,
    /// Piped to the child's stdin and closed. Prefer this over `args` for a
    /// prompt body or any other large/sensitive payload: `args` becomes the
    /// process's argv, world-readable via `ps`/`/proc/<pid>/cmdline` on a
    /// shared host, while stdin is not.
    pub stdin: Option<Vec<u8>>,
    pub working_directory: PathBuf,
    pub workspace_root: PathBuf,
}

impl std::fmt::Debug for ProcessSpec {
    /// Same rationale as `EnrollmentCredential`/`RunnerCredential`: this type
    /// is trivially reachable from a `?` early return or a panic message, so
    /// its `Debug` must be safe to print unconditionally rather than relying
    /// on every call site to remember not to.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProcessSpec")
            .field("program", &self.program)
            .field("args", &"[REDACTED]")
            .field("env", &RedactedEnv(&self.env))
            .field("stdin", &self.stdin.as_ref().map(|_| "[REDACTED]"))
            .field("working_directory", &self.working_directory)
            .field("workspace_root", &self.workspace_root)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessLimits {
    pub max_stdout_bytes: usize,
    pub max_stderr_bytes: usize,
    pub timeout: Duration,
    /// Grace period between SIGTERM and SIGKILL on cancellation/timeout.
    pub termination_grace: Duration,
}

impl ProcessLimits {
    pub const fn new(max_stdout_bytes: usize, max_stderr_bytes: usize, timeout: Duration) -> Self {
        Self {
            max_stdout_bytes,
            max_stderr_bytes,
            timeout,
            termination_grace: Duration::from_secs(5),
        }
    }
}

/// One captured stream, bounded to a hard byte cap. Truncation is a field,
/// never a silently shortened string: a caller that only checks `.text` and
/// ignores `.truncated` still gets a correct (merely incomplete) value, but
/// nothing can *represent* "this is the whole stream" when it is not.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CapturedOutput {
    pub text: String,
    pub truncated: bool,
    pub bytes_dropped: u64,
    pub total_bytes_seen: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessExit {
    Exited(i32),
    #[cfg(unix)]
    Signaled(i32),
    /// The process did not exit within `ProcessLimits::timeout` and was
    /// killed (its whole group, on Unix).
    TimedOut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessResult {
    pub exit: ProcessExit,
    pub stdout: CapturedOutput,
    pub stderr: CapturedOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelOutcome {
    /// The group acknowledged SIGTERM within the grace period.
    Stopped,
    /// The group did not stop from SIGTERM alone and was sent SIGKILL.
    Killed,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProcessError {
    #[error("process working directory escapes its declared workspace root")]
    WorkspaceEscape,
    #[error("process could not be spawned")]
    Spawn,
    #[error("process io failed")]
    Io,
    #[error("process signal delivery failed")]
    Signal,
}

/// A spawned child under group-aware supervision. Constructed only via
/// [`ProcessSpec::spawn`].
pub struct SupervisedProcess {
    child: Child,
    pid: u32,
}

impl ProcessSpec {
    /// Validates workspace confinement, then spawns the child in its own
    /// process group (Unix). Confinement is checked with the same
    /// canonicalize-then-`starts_with` pattern `workspace.rs` uses for
    /// cleanup, for the same reason: a symlink or `..` component must be
    /// resolved before comparison, not after.
    pub async fn spawn(&self) -> Result<SupervisedProcess, ProcessError> {
        let root = self
            .workspace_root
            .canonicalize()
            .map_err(|_| ProcessError::WorkspaceEscape)?;
        let working_directory = self
            .working_directory
            .canonicalize()
            .map_err(|_| ProcessError::WorkspaceEscape)?;
        if working_directory != root && !working_directory.starts_with(&root) {
            return Err(ProcessError::WorkspaceEscape);
        }

        let mut command = Command::new(&self.program);
        command
            .args(&self.args)
            .current_dir(&working_directory)
            .env_clear()
            .envs(&self.env)
            .kill_on_drop(true)
            .stdin(if self.stdin.is_some() {
                std::process::Stdio::piped()
            } else {
                std::process::Stdio::null()
            })
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        #[cfg(unix)]
        {
            // A new group whose id equals the child's own pid, independent
            // of the runner process's group.
            command.process_group(0);
        }

        let mut child = command.spawn().map_err(|_| ProcessError::Spawn)?;
        let pid = child.id().ok_or(ProcessError::Spawn)?;

        if let Some(input) = &self.stdin {
            use tokio::io::AsyncWriteExt;
            if let Some(mut stdin) = child.stdin.take() {
                // Best-effort: a harness that exits before reading stdin
                // (e.g. the `failure` fake-binary mode) makes this a broken
                // pipe, which is not itself a spawn failure — the exit/wait
                // path below is the authoritative outcome.
                let _ = stdin.write_all(input).await;
                drop(stdin);
            }
        }

        Ok(SupervisedProcess { child, pid })
    }
}

impl SupervisedProcess {
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Runs the child to completion (or timeout), capturing stdout/stderr
    /// under `limits`' byte caps and scrubbing `secrets` from whatever text
    /// is retained. Reader tasks keep draining each pipe even past the cap
    /// (dropping bytes instead of storing them) so a chatty child can never
    /// deadlock writing into a full OS pipe buffer while this future is
    /// waiting on something else.
    pub async fn wait_with_capture(
        mut self,
        limits: &ProcessLimits,
        secrets: &SecretMaterial,
    ) -> Result<ProcessResult, ProcessError> {
        let mut stdout_pipe = self.child.stdout.take().ok_or(ProcessError::Io)?;
        let mut stderr_pipe = self.child.stderr.take().ok_or(ProcessError::Io)?;
        let stdout_cap = limits.max_stdout_bytes;
        let stderr_cap = limits.max_stderr_bytes;

        let stdout_task =
            tokio::spawn(async move { capture_bounded(&mut stdout_pipe, stdout_cap).await });
        let stderr_task =
            tokio::spawn(async move { capture_bounded(&mut stderr_pipe, stderr_cap).await });

        let exit = match time::timeout(limits.timeout, self.child.wait()).await {
            Ok(Ok(status)) => status_to_exit(status),
            Ok(Err(_)) => return Err(ProcessError::Io),
            Err(_) => {
                kill_tree(self.pid, &mut self.child, limits.termination_grace).await?;
                ProcessExit::TimedOut
            }
        };

        let stdout_raw = stdout_task.await.map_err(|_| ProcessError::Io)?;
        let stderr_raw = stderr_task.await.map_err(|_| ProcessError::Io)?;
        Ok(ProcessResult {
            exit,
            stdout: finalize_capture(stdout_raw, secrets),
            stderr: finalize_capture(stderr_raw, secrets),
        })
    }

    /// Requests cancellation: SIGTERM to the whole group, then SIGKILL after
    /// `grace` if the group has not stopped. This kills descendants the
    /// child itself spawned, not only the direct child — see the module
    /// docs. Consumes `self` because a cancelled process must never be
    /// waited on again by the caller; [`Self::wait_with_capture`] already
    /// reaps it as part of killing the tree.
    pub async fn cancel(mut self, grace: Duration) -> Result<CancelOutcome, ProcessError> {
        kill_tree(self.pid, &mut self.child, grace).await
    }
}

async fn kill_tree(
    pid: u32,
    child: &mut Child,
    grace: Duration,
) -> Result<CancelOutcome, ProcessError> {
    #[cfg(unix)]
    {
        unix::signal_group(pid, unix::SIGTERM).map_err(|_| ProcessError::Signal)?;
        if time::timeout(grace, child.wait()).await.is_ok() {
            return Ok(CancelOutcome::Stopped);
        }
        unix::signal_group(pid, unix::SIGKILL).map_err(|_| ProcessError::Signal)?;
        let _ = time::timeout(Duration::from_secs(5), child.wait()).await;
        Ok(CancelOutcome::Killed)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        let _ = grace;
        // Documented limitation: no portable process-group primitive here,
        // so only the direct child is targeted.
        let _ = child.start_kill();
        let _ = child.wait().await;
        Ok(CancelOutcome::Killed)
    }
}

/// Reads `pipe` to EOF, retaining at most `cap` bytes but always continuing
/// to drain past the cap so the writer end never blocks on a full pipe.
async fn capture_bounded(pipe: &mut (impl tokio::io::AsyncRead + Unpin), cap: usize) -> RawCapture {
    let mut buffer = Vec::with_capacity(cap.min(64 * 1024));
    let mut total: u64 = 0;
    let mut chunk = [0u8; 32 * 1024];
    loop {
        match pipe.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => {
                total += n as u64;
                if buffer.len() < cap {
                    let room = cap - buffer.len();
                    buffer.extend_from_slice(&chunk[..n.min(room)]);
                }
                // Bytes beyond `cap` are intentionally never stored.
            }
            Err(_) => break,
        }
    }
    RawCapture { buffer, total }
}

struct RawCapture {
    buffer: Vec<u8>,
    total: u64,
}

fn finalize_capture(raw: RawCapture, secrets: &SecretMaterial) -> CapturedOutput {
    let truncated = raw.total > raw.buffer.len() as u64;
    let bytes_dropped = raw.total - raw.buffer.len() as u64;
    let text = secrets.scrub(&String::from_utf8_lossy(&raw.buffer));
    CapturedOutput {
        text,
        truncated,
        bytes_dropped,
        total_bytes_seen: raw.total,
    }
}

fn status_to_exit(status: std::process::ExitStatus) -> ProcessExit {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return ProcessExit::Signaled(signal);
        }
    }
    ProcessExit::Exited(status.code().unwrap_or(-1))
}

/// Returns whether a process with this pid currently exists, via `kill(pid,
/// 0)` (sends no signal; only checks deliverability). Used by tests to prove
/// a cancelled descendant is actually gone rather than merely unresponsive.
#[cfg(unix)]
pub fn process_alive(pid: u32) -> bool {
    unix::process_alive(pid)
}

#[cfg(unix)]
mod unix {
    use std::io;

    // A bare `extern "C"` block avoids adding the `libc` crate as a new
    // `tack-runner` dependency for the one POSIX syscall this module needs. `libc` is already resolved
    // transitively in `Cargo.lock` (tokio depends on it), but Cargo does not
    // let a crate call into a dependency it has not declared directly, so
    // that transitive presence cannot be relied on here. `kill(2)`'s
    // signature is part of the stable, decades-old POSIX ABI linked into
    // every Unix Rust binary already (via the platform's libc), so this
    // declaration is not itself a new dependency in any practical sense.
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }

    pub const SIGTERM: i32 = 15;
    pub const SIGKILL: i32 = 9;

    /// POSIX ESRCH ("no such process"). Stable at 3 across Linux, macOS and
    /// the BSDs, the only platforms this `#[cfg(unix)]` module targets.
    const ESRCH: i32 = 3;

    /// Sends `sig` to the process **group** led by `pid` (a negative pid
    /// argument to `kill(2)` targets the whole group). `pid` must be a group
    /// leader this runner itself spawned with `process_group(0)` — never an
    /// arbitrary or externally supplied pid.
    pub fn signal_group(pid: u32, sig: i32) -> io::Result<()> {
        let result = unsafe { kill(-(pid as i32), sig) };
        if result == 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ESRCH) {
            // The whole group is already gone: cancelling an
            // already-terminated attempt is idempotent, not an error.
            return Ok(());
        }
        Err(error)
    }

    pub fn process_alive(pid: u32) -> bool {
        unsafe { kill(pid as i32, 0) == 0 }
    }
}

#[cfg(test)]
#[path = "process/tests.rs"]
mod tests;
