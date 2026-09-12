//! The real [`WorktreeProvisioner`]: a private, attempt-scoped git checkout.
//!
//! # Why a private clone and not `git worktree add`
//!
//! The trait is named `WorktreeProvisioner` because the *product* requirement
//! is an isolated working tree per attempt, not because git's `worktree`
//! subcommand must be the mechanism. Two facts rule that subcommand out here:
//!
//! 1. `git worktree add` keeps administrative state (`worktrees/<name>`, a
//!    lock file and a `gitdir` pointer) inside one shared repository. Two
//!    attempts provisioning at once contend on that repository's index lock,
//!    and a runner killed mid-add leaves a registered-but-absent worktree that
//!    a later attempt inherits — exactly the two failure modes a private
//!    clone avoids. Recovering from it needs `git worktree prune` against shared
//!    mutable state that another live attempt may be using at that moment.
//! 2. `git worktree add` refuses a target directory that already exists and is
//!    not empty. Every attempt directory already carries the `.tack-attempt`
//!    marker that [`super::WorkspaceManager`] writes *before* provisioning, so
//!    the target is never empty by construction.
//!
//! `git init` in place has neither problem: it accepts a non-empty directory,
//! every attempt owns 100% of its own repository state, and cleanup is a plain
//! recursive delete of a directory this runner stamped — which
//! [`super::WorkspaceManager::cleanup`] already implements and guards.
//!
//! # Restart safety
//!
//! Provisioning is not atomic — a checkout is thousands of files. A runner
//! killed halfway leaves a directory that *looks* like a checkout but is not
//! one. The completion sentinel [`CHECKOUT_MARKER`] is written (and fsynced)
//! only after `checkout` returns, and it records the exact resolved commit. On
//! a restart the provisioner either finds a sentinel that agrees with the live
//! repository — and reuses the checkout — or it discards everything under the
//! attempt directory and provisions again. A half-made checkout is therefore
//! never inherited, by this attempt or any later one.
//!
//! # What never reaches a log
//!
//! A remote URL can embed credentials (`https://user:token@host/repo.git`) and
//! a query string. Git echoes the remote back in most of its error messages,
//! so raw git output is treated as tainted: it is scrubbed through
//! [`SecretMaterial`] (seeded with the remote, its userinfo and its password)
//! and [`redact_query`] before it can reach a tracing field, and the typed
//! errors this module returns carry no remote, path or git text at all.

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use async_trait::async_trait;
use tokio::process::Command;

use super::{Workspace, WorkspaceError, WorktreeProvisioner};
use crate::{
    client::RepositorySpec,
    harness::redact::{SecretMaterial, redact_query},
};

/// Written only after a checkout completed; contains the resolved commit.
pub const CHECKOUT_MARKER: &str = ".tack-checkout";
/// Written by `WorkspaceManager` before provisioning; must survive a purge.
const ATTEMPT_MARKER: &str = ".tack-attempt";

/// Default wall-clock ceiling for one git invocation. Cloning a large
/// repository over a slow link is legitimately slow, so this is generous; its
/// job is to turn a hung `git` (an auth prompt, a black-holed TCP connection)
/// into a typed failure instead of an attempt that never reports anything.
pub const DEFAULT_GIT_TIMEOUT: Duration = Duration::from_secs(600);

/// Provisions each attempt its own git checkout using the local `git` binary.
#[derive(Debug, Clone)]
pub struct GitWorktreeProvisioner {
    program: PathBuf,
    timeout: Duration,
}

impl Default for GitWorktreeProvisioner {
    fn default() -> Self {
        Self::new("git", DEFAULT_GIT_TIMEOUT)
    }
}

impl GitWorktreeProvisioner {
    pub fn new(program: impl Into<PathBuf>, timeout: Duration) -> Self {
        Self {
            program: program.into(),
            timeout,
        }
    }

    /// One bounded `git` invocation inside `directory`.
    ///
    /// The child inherits the operator's ambient git configuration on purpose:
    /// the runner-v1 contract has no channel for repository credentials, so a
    /// runner-local `~/.gitconfig`, credential helper or SSH agent is the only
    /// way a private remote can ever work. What it must *not* inherit is
    /// repository-selecting state (`GIT_DIR` and friends): a runner started
    /// from inside a git repository, or under a git hook, would otherwise
    /// silently operate on that repository instead of the attempt's.
    async fn git(
        &self,
        directory: &Path,
        args: &[&str],
        secrets: &SecretMaterial,
    ) -> Result<GitOutput, WorkspaceError> {
        let mut command = Command::new(&self.program);
        command
            .current_dir(directory)
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_OBJECT_DIRECTORY")
            .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
            .env_remove("GIT_CEILING_DIRECTORIES")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let child = command.spawn().map_err(|error| {
            // `NotFound` at spawn has two causes — the program is not on
            // `PATH`, or the working directory no longer exists — and telling
            // an operator "git is not installed" when the attempt directory
            // vanished underneath the runner would send them to the wrong
            // place entirely.
            match (error.kind(), directory.is_dir()) {
                (std::io::ErrorKind::NotFound, true) => WorkspaceError::GitUnavailable,
                (std::io::ErrorKind::NotFound, false) => WorkspaceError::UnsafePath,
                _ => WorkspaceError::Io,
            }
        })?;
        // `kill_on_drop` turns the timeout into a real kill: dropping the
        // future drops the child, which sends SIGKILL. A hung `git` therefore
        // cannot outlive the attempt that spawned it.
        let output = match tokio::time::timeout(self.timeout, child.wait_with_output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(_)) => return Err(WorkspaceError::Io),
            Err(_) => return Err(WorkspaceError::GitTimeout),
        };
        let result = GitOutput {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        };
        if !result.success {
            // Only the subcommand name and scrubbed stderr — never the full
            // argument list, which carries the remote URL verbatim.
            tracing::debug!(
                subcommand = args.first().copied().unwrap_or("git"),
                detail = %result.redacted_stderr(secrets),
                "git command failed"
            );
        }
        Ok(result)
    }

    async fn git_ok(
        &self,
        directory: &Path,
        args: &[&str],
        secrets: &SecretMaterial,
    ) -> Result<GitOutput, WorkspaceError> {
        let output = self.git(directory, args, secrets).await?;
        if output.success {
            Ok(output)
        } else {
            Err(WorkspaceError::Git)
        }
    }

    /// True when this directory already holds a completed checkout of exactly
    /// this revision. Three independent facts must agree — the sentinel, a
    /// live `.git`, and the commit `HEAD` actually points at — because any one
    /// of them alone can survive a kill that invalidated the others.
    async fn already_provisioned(
        &self,
        path: &Path,
        requested: &str,
        secrets: &SecretMaterial,
    ) -> bool {
        let Ok(recorded) = fs::read_to_string(path.join(CHECKOUT_MARKER)) else {
            return false;
        };
        let recorded = recorded.trim().to_owned();
        if recorded.is_empty() || !path.join(".git").exists() {
            return false;
        }
        let Ok(head) = self
            .git(path, &["rev-parse", "--verify", "HEAD"], secrets)
            .await
        else {
            return false;
        };
        if !head.success || head.stdout != recorded {
            return false;
        }
        // A sentinel from a *different* requested revision must not be reused:
        // the same attempt directory is only ever re-provisioned for the same
        // attempt, but a caller could still hand a changed `base_revision`.
        match self
            .git(
                path,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("{requested}^{{commit}}"),
                ],
                secrets,
            )
            .await
        {
            Ok(resolved) if resolved.success => resolved.stdout == recorded,
            _ => false,
        }
    }

    /// Removes every entry under the attempt directory except the attempt
    /// marker, which identifies the directory this runner is allowed to touch
    /// and must therefore outlive the purge.
    ///
    /// The caller has already proven `path` is a non-symlink directory holding
    /// a marker that matches this attempt; nothing outside it is reachable,
    /// because entries are removed by direct `read_dir` handle, never by a
    /// path assembled from untrusted input.
    fn purge_partial_checkout(path: &Path) -> Result<(), WorkspaceError> {
        for entry in fs::read_dir(path).map_err(|_| WorkspaceError::Io)? {
            let entry = entry.map_err(|_| WorkspaceError::Io)?;
            if entry.file_name() == ATTEMPT_MARKER {
                continue;
            }
            let file_type = entry.file_type().map_err(|_| WorkspaceError::Io)?;
            let outcome = if file_type.is_dir() {
                fs::remove_dir_all(entry.path())
            } else {
                // A symlink is removed as a link; `remove_dir_all` would be
                // refused on it anyway, and neither call follows it.
                fs::remove_file(entry.path())
            };
            outcome.map_err(|_| WorkspaceError::Io)?;
        }
        Ok(())
    }

    /// Fetches `revision` as cheaply as the remote allows, then leaves the
    /// working tree detached at the exact commit. Returns the resolved commit.
    async fn fetch_and_checkout(
        &self,
        path: &Path,
        remote: &str,
        revision: &str,
        secrets: &SecretMaterial,
    ) -> Result<String, WorkspaceError> {
        self.git_ok(path, &["init", "--quiet"], secrets).await?;
        // `set-url` covers the re-provision case where `origin` already exists
        // from a purged-but-not-quite attempt; `add` covers the fresh case.
        if self
            .git_ok(path, &["remote", "add", "origin", remote], secrets)
            .await
            .is_err()
        {
            self.git_ok(path, &["remote", "set-url", "origin", remote], secrets)
                .await?;
        }

        // Fetching the single requested commit is by far the cheapest path,
        // but it only works when the server allows it (`uploadpack.allowAny*`;
        // most forges do, a plain HTTP dumb remote does not) and when the
        // revision is a commit id rather than a branch name. Its failure is
        // expected and is not an error — it falls back to a full fetch.
        let shallow = self
            .git(
                path,
                &["fetch", "--no-tags", "--depth", "1", "origin", revision],
                secrets,
            )
            .await?;
        let resolved = if shallow.success {
            let head = self
                .git_ok(path, &["rev-parse", "--verify", "FETCH_HEAD"], secrets)
                .await?;
            head.stdout
        } else {
            self.git_ok(path, &["fetch", "--no-tags", "origin"], secrets)
                .await
                .map_err(|error| match error {
                    // A failed full fetch after a failed narrow fetch is the
                    // "cannot reach or read this remote" case, reported as
                    // itself rather than as a generic git failure.
                    WorkspaceError::Git => WorkspaceError::RepositoryUnreachable,
                    other => other,
                })?;
            self.resolve_revision(path, revision, secrets).await?
        };

        self.git_ok(
            path,
            &[
                "-c",
                "advice.detachedHead=false",
                "checkout",
                "--detach",
                &resolved,
            ],
            secrets,
        )
        .await?;

        // The requested revision governs, not what was fetched. If the caller
        // named a full commit id, the checked-out commit must be that exact
        // commit — otherwise the attempt would run against code nobody asked
        // for while reporting the requested `base_revision` to the server.
        let head = self
            .git_ok(path, &["rev-parse", "--verify", "HEAD"], secrets)
            .await?
            .stdout;
        if head != resolved || (is_full_commit_id(revision) && !head.eq_ignore_ascii_case(revision))
        {
            return Err(WorkspaceError::RevisionUnavailable);
        }
        Ok(head)
    }

    /// Maps a requested revision onto a commit that now exists locally.
    /// Ordered deliberately: an exact object id first, then a remote-tracking
    /// branch, then a tag. `origin/<name>` is tried before a bare `<name>`
    /// because after `git init` a bare branch name resolves to nothing, while
    /// a *local* name colliding with a fetched one cannot exist yet.
    async fn resolve_revision(
        &self,
        path: &Path,
        revision: &str,
        secrets: &SecretMaterial,
    ) -> Result<String, WorkspaceError> {
        for candidate in [
            revision.to_owned(),
            format!("origin/{revision}"),
            format!("refs/tags/{revision}"),
        ] {
            let output = self
                .git(
                    path,
                    &[
                        "rev-parse",
                        "--verify",
                        "--quiet",
                        &format!("{candidate}^{{commit}}"),
                    ],
                    secrets,
                )
                .await?;
            if output.success && !output.stdout.is_empty() {
                return Ok(output.stdout);
            }
        }
        Err(WorkspaceError::RevisionUnavailable)
    }
}

#[async_trait]
impl WorktreeProvisioner for GitWorktreeProvisioner {
    async fn provision(
        &self,
        workspace: &Workspace,
        repository: &RepositorySpec,
    ) -> Result<(), WorkspaceError> {
        let path = workspace.path.as_path();
        // Independent of `WorkspaceManager`'s own guard on purpose: this impl
        // deletes files, so it re-proves for itself that the directory is a
        // real directory this runner stamped for this exact attempt.
        let metadata = fs::symlink_metadata(path).map_err(|_| WorkspaceError::UnsafePath)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(WorkspaceError::UnsafePath);
        }
        let marker = fs::read_to_string(path.join(ATTEMPT_MARKER))
            .map_err(|_| WorkspaceError::UnsafePath)?;
        if marker != workspace.attempt_id.as_str() {
            return Err(WorkspaceError::AttemptMismatch);
        }

        let secrets = remote_secrets(&repository.remote);
        if self
            .already_provisioned(path, &repository.base_revision, &secrets)
            .await
        {
            tracing::debug!(
                attempt_id = workspace.attempt_id.as_str(),
                workspace_id = workspace.id.as_str(),
                "reusing the existing attempt checkout"
            );
            return Ok(());
        }
        // Either nothing was provisioned yet, or a previous provision was
        // interrupted. Both are discarded rather than repaired: a partial
        // checkout has no trustworthy state to repair from.
        Self::purge_partial_checkout(path)?;

        let result = self
            .fetch_and_checkout(
                path,
                &repository.remote,
                &repository.base_revision,
                &secrets,
            )
            .await;
        let resolved = match result {
            Ok(resolved) => resolved,
            Err(error) => {
                // Leave nothing that a later provision could mistake for a
                // usable checkout. The sentinel was never written, so this is
                // belt-and-braces; it also keeps a failed attempt's directory
                // small instead of holding a half-fetched object store.
                let _ = Self::purge_partial_checkout(path);
                tracing::warn!(
                    attempt_id = workspace.attempt_id.as_str(),
                    workspace_id = workspace.id.as_str(),
                    failure = %error,
                    "attempt checkout failed"
                );
                return Err(error);
            }
        };

        write_checkout_marker(&path.join(CHECKOUT_MARKER), &resolved)?;
        tracing::info!(
            attempt_id = workspace.attempt_id.as_str(),
            workspace_id = workspace.id.as_str(),
            "attempt checkout ready"
        );
        Ok(())
    }
}

struct GitOutput {
    success: bool,
    stdout: String,
    stderr: String,
}

impl GitOutput {
    /// The only sanctioned way to surface git's own text. Both scrubbing
    /// passes are applied: the exact remote (and its userinfo/password, which
    /// git may print on its own) is replaced wholesale, and any surviving
    /// URL-shaped query string is dropped.
    fn redacted_stderr(&self, secrets: &SecretMaterial) -> String {
        secrets
            .scrub(&self.stderr)
            .split_whitespace()
            .map(redact_query)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Every value that must never survive into a log line for this remote.
fn remote_secrets(remote: &str) -> SecretMaterial {
    let mut material = SecretMaterial::new();
    material.register(remote);
    for secret in url_secrets(remote) {
        material.register(secret);
    }
    material
}

/// Extracts the userinfo of a URL — `user`, `password` and `user:password` —
/// so each can be scrubbed even when git prints only one of them.
fn url_secrets(remote: &str) -> Vec<String> {
    let Some(rest) = remote.split_once("://").map(|(_, rest)| rest) else {
        return Vec::new();
    };
    let Some((userinfo, _)) = rest.split_once('@') else {
        return Vec::new();
    };
    let mut secrets = vec![userinfo.to_owned()];
    if let Some((user, password)) = userinfo.split_once(':') {
        secrets.push(user.to_owned());
        secrets.push(password.to_owned());
    }
    secrets.retain(|secret| !secret.is_empty());
    secrets
}

fn is_full_commit_id(revision: &str) -> bool {
    revision.len() == 40
        && revision
            .chars()
            .all(|character| character.is_ascii_hexdigit())
}

fn write_checkout_marker(path: &Path, commit: &str) -> Result<(), WorkspaceError> {
    let mut marker = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(|_| WorkspaceError::Io)?;
    marker
        .write_all(commit.as_bytes())
        .map_err(|_| WorkspaceError::Io)?;
    // Durable before it is trusted: an unsynced sentinel after a power loss
    // would claim a checkout that the filesystem never finished writing.
    marker.sync_all().map_err(|_| WorkspaceError::Io)?;
    super::owner_only(path).map_err(|_| WorkspaceError::Io)
}

#[cfg(test)]
#[path = "git/tests.rs"]
mod tests;
