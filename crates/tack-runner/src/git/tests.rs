use std::process::Command as SyncCommand;

use super::*;
use crate::client::{AttemptId, WorkspaceId};

/// Resolves `git` to an absolute path instead of relying on `PATH`.
///
/// Not paranoia: `harness::claude_code`'s discovery test overwrites the
/// process-wide `PATH` for the duration of its assertion, so resolving a
/// bare program name here would depend on whether that test is running.
/// An absolute path is independent of it. This showed up as a
/// one-in-many-runs "binary not found" in
/// `a_checkout_of_a_different_revision_is_never_reused` back when the
/// whole binary's tests shared one process.
fn git_program() -> PathBuf {
    for candidate in [
        "/usr/bin/git",
        "/bin/git",
        "/usr/local/bin/git",
        "/opt/homebrew/bin/git",
    ] {
        if Path::new(candidate).is_file() {
            return PathBuf::from(candidate);
        }
    }
    PathBuf::from("git")
}

/// A scratch directory that removes itself, and everything written under
/// it, when the returned guard drops — including when an assertion panics
/// first.
fn temp_dir(label: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(label)
        .tempdir()
        .expect("temporary directory")
}

fn run_git(directory: &Path, args: &[&str]) -> String {
    let output = SyncCommand::new(git_program())
        .current_dir(directory)
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// A real git repository with two commits — never a fake: these
/// operations must be proven against real git.
struct SourceRepository {
    first_commit: String,
    second_commit: String,
    /// Declared last: struct fields drop in declaration order, and the
    /// repository's directory must outlive whatever still reads `path()`.
    dir: tempfile::TempDir,
}

impl SourceRepository {
    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn create() -> Self {
        let path_dir = temp_dir("source");
        let path = path_dir.path();
        run_git(path, &["-c", "init.defaultBranch=main", "init", "--quiet"]);
        run_git(path, &["config", "user.email", "runner@example.invalid"]);
        run_git(path, &["config", "user.name", "Tack Runner Test"]);
        fs::write(path.join("README.md"), "first\n").expect("write");
        run_git(path, &["add", "."]);
        run_git(path, &["commit", "--quiet", "-m", "first"]);
        let first_commit = run_git(path, &["rev-parse", "HEAD"]);
        fs::write(path.join("README.md"), "second\n").expect("write");
        fs::write(path.join("added.txt"), "only in the second commit\n").expect("write");
        run_git(path, &["add", "."]);
        run_git(path, &["commit", "--quiet", "-m", "second"]);
        let second_commit = run_git(path, &["rev-parse", "HEAD"]);
        Self {
            first_commit,
            second_commit,
            dir: path_dir,
        }
    }

    fn spec(&self, revision: &str) -> RepositorySpec {
        RepositorySpec {
            remote: self.path().to_string_lossy().into_owned(),
            base_revision: revision.to_owned(),
        }
    }
}

/// Builds the attempt directory exactly as `WorkspaceManager::provision`
/// leaves it before it calls the provisioner: an owner-only directory
/// holding the attempt marker and nothing else.
fn attempt_workspace(root: &Path, attempt_id: &str, revision: &str) -> Workspace {
    let path = root.join(attempt_id);
    fs::create_dir_all(&path).expect("attempt directory");
    super::super::owner_only(&path).expect("owner-only attempt directory");
    fs::write(path.join(ATTEMPT_MARKER), attempt_id).expect("attempt marker");
    Workspace {
        attempt_id: AttemptId::new(attempt_id),
        id: WorkspaceId::new(format!("ws_{attempt_id}")),
        path,
        base_revision: revision.to_owned(),
    }
}

fn head_of(workspace: &Workspace) -> String {
    run_git(&workspace.path, &["rev-parse", "HEAD"])
}

// -----------------------------------------------------------------
// The capability itself: a claimed attempt gets a real checkout.
// -----------------------------------------------------------------

#[tokio::test]
async fn an_attempt_receives_a_real_checkout_of_the_requested_commit() {
    let source = SourceRepository::create();
    let root_dir = temp_dir("root");
    let root = root_dir.path();
    let workspace = attempt_workspace(root, "attempt-one", &source.first_commit);

    GitWorktreeProvisioner::new(git_program(), DEFAULT_GIT_TIMEOUT)
        .provision(&workspace, &source.spec(&source.first_commit))
        .await
        .expect("the attempt is checked out");

    // The working tree is the requested commit, not merely "some clone".
    assert_eq!(head_of(&workspace), source.first_commit);
    assert_eq!(
        fs::read_to_string(workspace.path.join("README.md")).expect("tracked file"),
        "first\n"
    );
    assert!(
        !workspace.path.join("added.txt").exists(),
        "a file added by a later commit must not be present"
    );
    // Detached: no branch is checked out, so nothing the harness does can
    // move a ref the next attempt would inherit.
    assert_eq!(
        run_git(&workspace.path, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "HEAD",
        "the checkout must be detached: nothing the harness does may move a branch"
    );
    assert_eq!(
        fs::read_to_string(workspace.path.join(CHECKOUT_MARKER)).expect("sentinel"),
        source.first_commit
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[tokio::test]
async fn a_branch_name_resolves_through_the_remote_tracking_ref() {
    let source = SourceRepository::create();
    let root_dir = temp_dir("root");
    let root = root_dir.path();
    let workspace = attempt_workspace(root, "attempt-branch", "main");

    GitWorktreeProvisioner::new(git_program(), DEFAULT_GIT_TIMEOUT)
        .provision(&workspace, &source.spec("main"))
        .await
        .expect("a branch name is a valid base revision");

    assert_eq!(head_of(&workspace), source.second_commit);
    fs::remove_dir_all(root).expect("cleanup");
}

#[tokio::test]
async fn two_concurrent_attempts_cannot_see_each_others_files() {
    let source = SourceRepository::create();
    let root_dir = temp_dir("root");
    let root = root_dir.path();
    let first = attempt_workspace(root, "attempt-one", &source.first_commit);
    let second = attempt_workspace(root, "attempt-two", &source.second_commit);
    let provisioner = GitWorktreeProvisioner::new(git_program(), DEFAULT_GIT_TIMEOUT);

    let first_spec = source.spec(&source.first_commit);
    let second_spec = source.spec(&source.second_commit);
    let (one, two) = tokio::join!(
        provisioner.provision(&first, &first_spec),
        provisioner.provision(&second, &second_spec),
    );
    one.expect("first attempt");
    two.expect("second attempt");

    assert_ne!(first.path, second.path);
    assert_eq!(head_of(&first), source.first_commit);
    assert_eq!(head_of(&second), source.second_commit);

    // A file one attempt's harness writes is invisible to the other, and
    // neither repository's git state is shared: the second attempt's
    // `added.txt` exists only there.
    fs::write(first.path.join("scratch.txt"), "work in progress").expect("harness write");
    assert!(!second.path.join("scratch.txt").exists());
    assert!(second.path.join("added.txt").exists());
    assert!(!first.path.join("added.txt").exists());
    assert_ne!(
        fs::canonicalize(first.path.join(".git")).expect("first git dir"),
        fs::canonicalize(second.path.join(".git")).expect("second git dir"),
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[tokio::test]
async fn the_attempt_checkout_stays_owner_only() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let source = SourceRepository::create();
        let root_dir = temp_dir("root");
        let root = root_dir.path();
        let workspace = attempt_workspace(root, "attempt-one", &source.first_commit);

        GitWorktreeProvisioner::new(git_program(), DEFAULT_GIT_TIMEOUT)
            .provision(&workspace, &source.spec(&source.first_commit))
            .await
            .expect("checkout");

        let mode = fs::metadata(&workspace.path)
            .expect("attempt directory")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode, 0o700,
            "provisioning must not widen the attempt directory"
        );
        let sentinel = fs::metadata(workspace.path.join(CHECKOUT_MARKER))
            .expect("sentinel")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(sentinel, 0o600);
        fs::remove_dir_all(root).expect("cleanup");
    }
}

// -----------------------------------------------------------------
// Crash safety: no half-made checkout is ever inherited.
// -----------------------------------------------------------------

/// A genuine kill mid-provision: a one-millisecond budget makes the timeout
/// fire while `git` is still working, and `kill_on_drop` SIGKILLs it — the
/// same state a killed runner leaves behind. The restart must then produce
/// a correct checkout rather than inheriting the wreckage.
#[tokio::test]
async fn a_runner_killed_mid_provision_leaves_nothing_a_restart_inherits() {
    let source = SourceRepository::create();
    let root_dir = temp_dir("root");
    let root = root_dir.path();
    let workspace = attempt_workspace(root, "attempt-one", &source.first_commit);

    let killed = GitWorktreeProvisioner::new(git_program(), Duration::from_micros(1))
        .provision(&workspace, &source.spec(&source.first_commit))
        .await;
    assert!(
        killed.is_err(),
        "the interrupted provision must not report success"
    );
    assert!(
        !workspace.path.join(CHECKOUT_MARKER).exists(),
        "an interrupted provision must never leave a completion sentinel"
    );
    assert!(
        workspace.path.join(ATTEMPT_MARKER).exists(),
        "the attempt marker identifies the directory and must survive"
    );

    GitWorktreeProvisioner::new(git_program(), DEFAULT_GIT_TIMEOUT)
        .provision(&workspace, &source.spec(&source.first_commit))
        .await
        .expect("the restart provisions a usable checkout");
    assert_eq!(head_of(&workspace), source.first_commit);
    assert_eq!(
        fs::read_to_string(workspace.path.join("README.md")).expect("tracked file"),
        "first\n"
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[tokio::test]
async fn a_partial_checkout_without_a_sentinel_is_discarded_not_reused() {
    let source = SourceRepository::create();
    let root_dir = temp_dir("root");
    let root = root_dir.path();
    let workspace = attempt_workspace(root, "attempt-one", &source.first_commit);
    // The debris a kill can leave: a half-written git directory, a stray
    // file, and a nested directory — but no sentinel.
    fs::create_dir_all(workspace.path.join(".git/objects")).expect("partial git dir");
    fs::write(workspace.path.join(".git/HEAD"), "garbage").expect("partial HEAD");
    fs::write(workspace.path.join("stale.txt"), "left by a dead attempt").expect("stale file");

    GitWorktreeProvisioner::new(git_program(), DEFAULT_GIT_TIMEOUT)
        .provision(&workspace, &source.spec(&source.first_commit))
        .await
        .expect("a partial checkout is replaced, not repaired");

    assert_eq!(head_of(&workspace), source.first_commit);
    assert!(
        !workspace.path.join("stale.txt").exists(),
        "debris from an interrupted attempt must not survive into the new checkout"
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[tokio::test]
async fn a_sentinel_that_disagrees_with_head_is_not_trusted() {
    let source = SourceRepository::create();
    let root_dir = temp_dir("root");
    let root = root_dir.path();
    let workspace = attempt_workspace(root, "attempt-one", &source.first_commit);
    let provisioner = GitWorktreeProvisioner::new(git_program(), DEFAULT_GIT_TIMEOUT);
    provisioner
        .provision(&workspace, &source.spec(&source.first_commit))
        .await
        .expect("first checkout");

    // A sentinel naming a commit the working tree is not on is the exact
    // shape of a torn write. Re-provisioning must rebuild rather than
    // believe the file.
    fs::write(workspace.path.join(CHECKOUT_MARKER), &source.second_commit).expect("tear");
    fs::write(workspace.path.join("stale.txt"), "from the torn state").expect("stale");
    provisioner
        .provision(&workspace, &source.spec(&source.first_commit))
        .await
        .expect("rebuild");

    assert_eq!(head_of(&workspace), source.first_commit);
    assert!(!workspace.path.join("stale.txt").exists());
    assert_eq!(
        fs::read_to_string(workspace.path.join(CHECKOUT_MARKER)).expect("sentinel"),
        source.first_commit
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[tokio::test]
async fn a_completed_checkout_is_reused_on_restart_instead_of_refetched() {
    let source = SourceRepository::create();
    let root_dir = temp_dir("root");
    let root = root_dir.path();
    let workspace = attempt_workspace(root, "attempt-one", &source.first_commit);
    let provisioner = GitWorktreeProvisioner::new(git_program(), DEFAULT_GIT_TIMEOUT);
    provisioner
        .provision(&workspace, &source.spec(&source.first_commit))
        .await
        .expect("first checkout");
    // Work in progress from before the restart. Reuse is asserted by its
    // survival, not by timing: a re-provision would purge it.
    fs::write(workspace.path.join("in-progress.txt"), "harness output").expect("write");

    provisioner
        .provision(&workspace, &source.spec(&source.first_commit))
        .await
        .expect("restart reuses the existing checkout");

    assert!(workspace.path.join("in-progress.txt").exists());
    assert_eq!(head_of(&workspace), source.first_commit);
    fs::remove_dir_all(root).expect("cleanup");
}

#[tokio::test]
async fn a_checkout_of_a_different_revision_is_never_reused() {
    let source = SourceRepository::create();
    let root_dir = temp_dir("root");
    let root = root_dir.path();
    let workspace = attempt_workspace(root, "attempt-one", &source.first_commit);
    let provisioner = GitWorktreeProvisioner::new(git_program(), DEFAULT_GIT_TIMEOUT);
    provisioner
        .provision(&workspace, &source.spec(&source.first_commit))
        .await
        .expect("first checkout");

    provisioner
        .provision(&workspace, &source.spec(&source.second_commit))
        .await
        .expect("second revision");

    assert_eq!(head_of(&workspace), source.second_commit);
    fs::remove_dir_all(root).expect("cleanup");
}

// -----------------------------------------------------------------
// Typed failures. Unsupported is typed; nothing is faked as success.
// -----------------------------------------------------------------

#[tokio::test]
async fn a_revision_that_does_not_exist_is_typed_and_writes_no_sentinel() {
    let source = SourceRepository::create();
    let root_dir = temp_dir("root");
    let root = root_dir.path();
    let missing = "0123456789abcdef0123456789abcdef01234567";
    let workspace = attempt_workspace(root, "attempt-one", missing);

    let error = GitWorktreeProvisioner::new(git_program(), DEFAULT_GIT_TIMEOUT)
        .provision(&workspace, &source.spec(missing))
        .await
        .expect_err("a missing revision cannot succeed");

    assert_eq!(error, WorkspaceError::RevisionUnavailable);
    assert!(!workspace.path.join(CHECKOUT_MARKER).exists());
    assert!(
        !workspace.path.join(".git").exists(),
        "a failed provision leaves no repository a later attempt could mistake for a checkout"
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[tokio::test]
async fn an_unreachable_repository_is_typed_as_unreachable() {
    let root_dir = temp_dir("root");
    let root = root_dir.path();
    let workspace = attempt_workspace(root, "attempt-one", "main");
    let repository = RepositorySpec {
        remote: root
            .join("no-such-repository")
            .to_string_lossy()
            .into_owned(),
        base_revision: "main".into(),
    };

    let error = GitWorktreeProvisioner::new(git_program(), DEFAULT_GIT_TIMEOUT)
        .provision(&workspace, &repository)
        .await
        .expect_err("an absent remote cannot succeed");

    assert_eq!(error, WorkspaceError::RepositoryUnreachable);
    assert!(!workspace.path.join(CHECKOUT_MARKER).exists());
    fs::remove_dir_all(root).expect("cleanup");
}

#[tokio::test]
async fn a_missing_git_binary_is_typed_not_a_generic_io_failure() {
    let source = SourceRepository::create();
    let root_dir = temp_dir("root");
    let root = root_dir.path();
    let workspace = attempt_workspace(root, "attempt-one", &source.first_commit);

    let error =
        GitWorktreeProvisioner::new("tack-runner-no-such-git-binary", Duration::from_secs(5))
            .provision(&workspace, &source.spec(&source.first_commit))
            .await
            .expect_err("a missing git binary cannot succeed");

    assert_eq!(error, WorkspaceError::GitUnavailable);
    fs::remove_dir_all(root).expect("cleanup");
}

/// Writes an executable stand-in and waits until it is actually
/// executable. The wait is not superstition: another test thread forking
/// while this file is open for writing inherits that descriptor, and the
/// exec then fails with `ETXTBSY`. Probing until a trivial invocation
/// succeeds removes a race that otherwise makes this test flaky under
/// parallel load — which is how it was found.
#[cfg(unix)]
fn stand_in_program(directory: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let program = directory.join(name);
    fs::write(
        &program,
        format!("#!/bin/sh\n[ \"$1\" = \"--probe\" ] && exit 0\n{body}\n"),
    )
    .expect("stand-in program");
    fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).expect("executable");
    for _ in 0..200 {
        match SyncCommand::new(&program).arg("--probe").status() {
            Ok(status) if status.success() => return program,
            _ => std::thread::sleep(Duration::from_millis(10)),
        }
    }
    panic!("stand-in program never became executable");
}

/// Deterministic proof of the timeout, independent of how fast real git
/// is: a stand-in `git` that never returns. Without `kill_on_drop` this
/// test would hang instead of failing.
#[cfg(unix)]
#[tokio::test]
async fn a_hanging_git_is_killed_and_reported_as_a_timeout() {
    let root_dir = temp_dir("root");
    let root = root_dir.path();
    let bin_dir = temp_dir("bin");
    let bin = bin_dir.path();
    let program = stand_in_program(bin, "git", "sleep 30");
    let workspace = attempt_workspace(root, "attempt-one", "main");
    let repository = RepositorySpec {
        remote: "https://example.invalid/repository.git".into(),
        base_revision: "main".into(),
    };

    let started = std::time::Instant::now();
    let error = GitWorktreeProvisioner::new(&program, Duration::from_millis(200))
        .provision(&workspace, &repository)
        .await
        .expect_err("a hanging git cannot succeed");

    assert_eq!(error, WorkspaceError::GitTimeout);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the timeout must bound the call, not wait for the child"
    );
    fs::remove_dir_all(root).expect("cleanup");
    fs::remove_dir_all(bin).expect("cleanup");
}

// -----------------------------------------------------------------
// Refusals. The provisioner deletes files, so it re-proves ownership.
// -----------------------------------------------------------------

#[tokio::test]
async fn a_directory_marked_for_another_attempt_is_refused_and_untouched() {
    let source = SourceRepository::create();
    let root_dir = temp_dir("root");
    let root = root_dir.path();
    let workspace = attempt_workspace(root, "attempt-one", &source.first_commit);
    fs::write(workspace.path.join(ATTEMPT_MARKER), "a-different-attempt").expect("marker");
    fs::write(
        workspace.path.join("evidence.txt"),
        "belongs to another attempt",
    )
    .expect("evidence");

    let error = GitWorktreeProvisioner::new(git_program(), DEFAULT_GIT_TIMEOUT)
        .provision(&workspace, &source.spec(&source.first_commit))
        .await
        .expect_err("another attempt's directory is refused");

    assert_eq!(error, WorkspaceError::AttemptMismatch);
    // The claim under test is "writes nothing", so absence is asserted
    // directly: the refusal must not have deleted the other attempt's work.
    assert_eq!(
        fs::read_to_string(workspace.path.join("evidence.txt")).expect("evidence survives"),
        "belongs to another attempt"
    );
    assert_eq!(
        fs::read_dir(&workspace.path).expect("read").count(),
        2,
        "no entry was created or removed by the refusal"
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[tokio::test]
async fn a_directory_with_no_marker_is_refused_and_untouched() {
    let source = SourceRepository::create();
    let root_dir = temp_dir("root");
    let root = root_dir.path();
    let path = root.join("unmarked");
    fs::create_dir_all(&path).expect("directory");
    fs::write(path.join("evidence.txt"), "not ours").expect("evidence");
    let workspace = Workspace {
        attempt_id: AttemptId::new("attempt-one"),
        id: WorkspaceId::new("ws_unmarked"),
        path: path.clone(),
        base_revision: source.first_commit.clone(),
    };

    let error = GitWorktreeProvisioner::new(git_program(), DEFAULT_GIT_TIMEOUT)
        .provision(&workspace, &source.spec(&source.first_commit))
        .await
        .expect_err("an unmarked directory is refused");

    assert_eq!(error, WorkspaceError::UnsafePath);
    assert!(path.join("evidence.txt").exists());
    fs::remove_dir_all(root).expect("cleanup");
}

#[cfg(unix)]
#[tokio::test]
async fn a_symlinked_attempt_path_is_refused_before_anything_is_written() {
    use std::os::unix::fs::symlink;

    let source = SourceRepository::create();
    let root_dir = temp_dir("root");
    let root = root_dir.path();
    let victim = root.join("victim");
    fs::create_dir_all(&victim).expect("victim");
    fs::write(victim.join(ATTEMPT_MARKER), "attempt-one").expect("marker");
    fs::write(victim.join("important.txt"), "do not delete").expect("important");
    let link = root.join("attempt-one");
    symlink(&victim, &link).expect("symlink");
    let workspace = Workspace {
        attempt_id: AttemptId::new("attempt-one"),
        id: WorkspaceId::new("ws_link"),
        path: link,
        base_revision: source.first_commit.clone(),
    };

    let error = GitWorktreeProvisioner::new(git_program(), DEFAULT_GIT_TIMEOUT)
        .provision(&workspace, &source.spec(&source.first_commit))
        .await
        .expect_err("a symlinked attempt path is refused");

    assert_eq!(error, WorkspaceError::UnsafePath);
    assert!(victim.join("important.txt").exists());
    fs::remove_dir_all(root).expect("cleanup");
}

// -----------------------------------------------------------------
// Rule 12: a credential embedded in the remote never reaches a log.
// -----------------------------------------------------------------

// Capturing what actually reached a log line needs a *global* subscriber,
// not a scoped one: `tracing` caches per-callsite interest process-wide,
// so a callsite first evaluated by another test (with no subscriber
// installed) is cached as "never interested" and a later thread-local
// subscriber never sees it. That is exactly how this assertion silently
// passed on nothing when the suite ran in parallel. Installing the
// subscriber globally rebuilds the interest cache; a thread-local buffer
// then keeps this test's captured output separate from every other
// test's, so the assertion stays about this call and nothing else.
thread_local! {
    static CAPTURED: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
}

struct ThreadLocalCapture;

impl std::io::Write for ThreadLocalCapture {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        CAPTURED.with(|captured| captured.borrow_mut().extend_from_slice(buffer));
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl tracing_subscriber::fmt::MakeWriter<'_> for ThreadLocalCapture {
    type Writer = Self;

    fn make_writer(&self) -> Self::Writer {
        ThreadLocalCapture
    }
}

fn install_log_capture() {
    static INSTALLED: std::sync::Once = std::sync::Once::new();
    INSTALLED.call_once(|| {
        tracing_subscriber::fmt()
            .with_writer(ThreadLocalCapture)
            .with_max_level(tracing::Level::DEBUG)
            .with_ansi(false)
            .init();
    });
    CAPTURED.with(|captured| captured.borrow_mut().clear());
}

#[tokio::test]
async fn a_credential_in_the_remote_url_never_reaches_a_log_line() {
    const PASSWORD: &str = "canary-git-password-9f3a";
    let root_dir = temp_dir("root");
    let root = root_dir.path();
    let workspace = attempt_workspace(root, "attempt-one", "main");
    let remote = format!("https://tack-user:{PASSWORD}@127.0.0.1:1/org/repo.git?token={PASSWORD}");
    let repository = RepositorySpec {
        remote: remote.clone(),
        base_revision: "main".into(),
    };

    install_log_capture();
    let error = GitWorktreeProvisioner::new(git_program(), Duration::from_secs(20))
        .provision(&workspace, &repository)
        .await
        .expect_err("an unreachable remote cannot succeed");

    assert_eq!(error, WorkspaceError::RepositoryUnreachable);
    let captured = CAPTURED
        .with(|captured| String::from_utf8(captured.borrow().clone()))
        .expect("utf-8");
    assert!(
        captured.contains("git command failed") && captured.contains("attempt checkout failed"),
        "the test is only load-bearing if git's failure was actually logged: {captured:?}"
    );
    assert!(
        !captured.contains(PASSWORD),
        "a credential embedded in the remote reached a log line: {captured}"
    );
    assert!(
        !captured.contains("tack-user"),
        "the remote's user reached a log line: {captured}"
    );
    assert!(
        !format!("{error}").contains(PASSWORD),
        "the typed error carries credential material"
    );
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn redacted_output_strips_userinfo_and_query_strings() {
    let remote = "https://user:secret-token@example.invalid/org/repo.git?key=secret-token";
    let output = GitOutput {
        success: false,
        stdout: String::new(),
        stderr: format!(
            "fatal: could not read from {remote}\nfatal: authentication failed for 'https://user:secret-token@example.invalid/org/repo.git'"
        ),
    };

    let redacted = output.redacted_stderr(&remote_secrets(remote));

    assert!(!redacted.contains("secret-token"));
    assert!(!redacted.contains("user:secret-token"));
    assert!(!redacted.contains("?key="));
    assert!(redacted.contains("fatal"), "the diagnosis itself survives");
}

#[test]
fn a_remote_without_userinfo_yields_no_spurious_secrets() {
    assert!(url_secrets("https://example.invalid/org/repo.git").is_empty());
    assert!(url_secrets("/var/lib/repositories/repo.git").is_empty());
    assert_eq!(
        url_secrets("https://user:pass@example.invalid/repo.git"),
        vec!["user:pass".to_owned(), "user".to_owned(), "pass".to_owned()]
    );
}

#[test]
fn only_a_full_commit_id_is_treated_as_one() {
    assert!(is_full_commit_id(
        "0123456789abcdef0123456789abcdef01234567"
    ));
    assert!(!is_full_commit_id("main"));
    assert!(!is_full_commit_id("0123456"));
    assert!(!is_full_commit_id(
        "z123456789abcdef0123456789abcdef01234567"
    ));
}
