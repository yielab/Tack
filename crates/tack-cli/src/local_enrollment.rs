//! Self-provisions a one-time enrollment token in-process for `tack serve
//! --with-runner`'s zero-touch local case, where the operator and the
//! runner are the same person on the same machine (see
//! `docs/adr/0058-standalone-single-binary-runner.md`).
//!
//! This module only ever creates the *pending* runner and mints its
//! one-time token — the administrative half of enrollment, the same
//! operation a human triggers by hand via `POST /api/runners/enrollment`.
//! Redeeming that token for a durable credential is unchanged: it still
//! happens over real runner-v1 HTTP inside `tack_runner::bootstrap::run`,
//! exactly like any remote runner. Nothing here calls a runner-protocol
//! route or reaches into `crates/tack-runner/src/transport.rs`.

use std::path::Path;

use tack_api::handlers::runner_admin;
use tack_runner::EnrollmentCredential;

/// Filename `tack_runner::transport` persists the durable session under,
/// inside a runner's `state_dir`. That module's own constant (`SESSION_FILE`)
/// is private, so this is a light, documented coupling to a stable-looking
/// name rather than a shared constant — chosen over adding a new public
/// method to `transport.rs` for the sake of one boolean check.
const SESSION_FILE_NAME: &str = "session.json";

/// Whether `state_dir` already holds a durable runner session from a
/// previous enrollment redemption. When true, the embedded runner should
/// reuse it unchanged rather than self-provisioning a new one — manual
/// enrollment or an earlier self-provisioned run both leave the same file.
pub fn has_stored_session(state_dir: &Path) -> bool {
    state_dir.join(SESSION_FILE_NAME).is_file()
}

/// Whether the session already on disk under `state_dir` names a runner id
/// that `database_url` — the exact database this server just opened — has
/// no row for at all. This is what separates "this credential belongs to a
/// database that was replaced" from "the database is momentarily
/// unreachable": by the time this runs, the caller's own server has already
/// opened `database_url` successfully, so there is no unreachable case left
/// to confuse this with. A session this function cannot even identify a
/// runner id for (missing, or unparseable) is reported as not orphaned —
/// there is nothing here to positively pin on a replaced database, so the
/// caller falls back to whatever it already does for that file.
pub async fn stored_session_orphaned(state_dir: &Path, database_url: &str) -> anyhow::Result<bool> {
    let Some(runner_id) = tack_runner::client::persisted_session_runner_id(state_dir) else {
        return Ok(false);
    };
    let exists = runner_admin::local_runner_id_exists(database_url, &runner_id)
        .await
        .map_err(|err| anyhow::anyhow!("checking the stored session's runner id failed: {err}"))?;
    Ok(!exists)
}

/// Stands in for `enrollment_credential` when [`has_stored_session`] is
/// true, so the caller does not have to touch the config's real credential
/// (and does not have to self-provision, which would mint an unused token
/// and a second `pending_enrollment` runner row) just to restart against an
/// already-enrolled `state_dir`.
///
/// This exists to satisfy `tack_runner::bootstrap::build_runtime`, which
/// requires *some* `enrollment_credential` before it looks at `state_dir` at
/// all — `build_runtime` itself does not check
/// for a stored session first. The placeholder is
/// never transmitted on a normal restart: `establish_session` in
/// `crates/tack-runner/src/transport.rs` tries the stored session's
/// `refresh` first and only reads `enrollment_credential` if that refresh is
/// rejected, at which point failing loudly — the session was invalid and no
/// real token was supplied — is the correct outcome, not a silent recovery.
/// [`EnrollmentCredential`]'s `Debug`/`Display` are unconditionally redacted
/// regardless of content, so this value is exactly as safe to hold as a real
/// one even though it is never real.
pub(crate) fn stored_session_placeholder() -> EnrollmentCredential {
    EnrollmentCredential::new("stored-session-on-disk-no-token-needed")
}

/// Self-provisions a single local runner and returns its one-time
/// enrollment token as a redacted [`EnrollmentCredential`] — never logged,
/// printed, or written anywhere by this function. The caller hands it
/// directly to `tack_runner::bootstrap::run`, which redeems it over
/// loopback HTTP through the ordinary protocol path, identically to a
/// manually issued token.
pub async fn self_provision(database_url: &str) -> anyhow::Result<EnrollmentCredential> {
    let response = runner_admin::provision_local_runner(database_url)
        .await
        .map_err(|err| anyhow::anyhow!("self-provisioning a local runner failed: {err}"))?;
    tracing::info!(
        runner_id = %response.runner_id,
        token_id = %response.token_id,
        expires_at = %response.expires_at,
        "self-provisioned a local runner for the embedded runner to redeem"
    );
    Ok(EnrollmentCredential::new(response.enrollment_token))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_stored_session_is_false_for_an_empty_directory() {
        let guard = tempfile::tempdir().expect("temporary directory");
        let dir = guard.path();

        assert!(!has_stored_session(dir));

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn has_stored_session_is_true_once_session_json_exists() {
        let guard = tempfile::tempdir().expect("temporary directory");
        let dir = guard.path();
        std::fs::write(dir.join("session.json"), "{}").unwrap();

        assert!(has_stored_session(dir));

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn has_stored_session_is_false_when_state_dir_does_not_exist_yet() {
        let guard = tempfile::tempdir().expect("temporary directory");
        // A path under the guard that was never created: `has_stored_session`
        // must answer for a state directory that does not exist at all.
        let dir = guard.path().join("absent");

        assert!(!has_stored_session(&dir));
    }

    /// The two branches `stored_session_orphaned` settles without ever
    /// reaching the database at all: nothing on disk to name a runner id for
    /// in the first place. Neither is "the wrong database" — there is
    /// nothing here to positively pin on one — so both report `false`
    /// rather than guessing, and a real (unreachable) `database_url` proves
    /// neither branch tries to open it.
    #[tokio::test]
    async fn stored_session_orphaned_is_false_with_nothing_on_disk_to_check() {
        let guard = tempfile::tempdir().expect("temporary directory");
        let dir = guard.path();

        assert!(
            !stored_session_orphaned(dir, "not-a-real-database-url")
                .await
                .expect("no session on disk must never fail, let alone reach a database")
        );
    }

    #[tokio::test]
    async fn stored_session_orphaned_is_false_for_an_unparseable_session() {
        let guard = tempfile::tempdir().expect("temporary directory");
        let dir = guard.path();
        std::fs::write(dir.join(SESSION_FILE_NAME), b"not json").expect("write malformed session");

        assert!(
            !stored_session_orphaned(dir, "not-a-real-database-url")
                .await
                .expect("a session this function cannot even identify a runner id for must never fail")
        );
    }
}
