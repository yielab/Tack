use std::fs;

use super::*;
use crate::client::{AttemptId, AttemptState, FencingToken, RunnerId, Timestamp};

#[derive(Default)]
struct FakeProvisioner;

#[async_trait]
impl WorktreeProvisioner for FakeProvisioner {
    async fn provision(
        &self,
        _workspace: &Workspace,
        _repository: &RepositorySpec,
    ) -> Result<(), WorkspaceError> {
        Ok(())
    }
}

/// A scratch directory that removes itself, and everything written under
/// it, when the returned guard drops — including when an assertion panics
/// first.
fn root() -> tempfile::TempDir {
    tempfile::tempdir().expect("temporary directory")
}

fn lease(id: &str) -> AttemptLease {
    AttemptLease {
        attempt_id: AttemptId::new(id),
        runner_id: RunnerId::new("runner"),
        fencing_token: FencingToken(1),
        attempt_number: 1,
        state: AttemptState::Leased,
        issued_at: Timestamp::new("2026-08-06T12:20:00Z"),
        expires_at: Timestamp::new("2026-08-06T12:21:00Z"),
    }
}

fn repository() -> RepositorySpec {
    RepositorySpec {
        remote: "https://example.invalid/repository.git".into(),
        base_revision: "base".into(),
    }
}

#[tokio::test]
async fn attempts_receive_distinct_deterministic_workspaces() {
    let root_dir = root();
    let root = root_dir.path();
    let manager = WorkspaceManager::new(root, FakeProvisioner);
    let first = manager
        .prepare(&lease("attempt-one"), &repository())
        .await
        .expect("first");
    let second = manager
        .prepare(&lease("attempt-two"), &repository())
        .await
        .expect("second");
    let repeated = manager
        .prepare(&lease("attempt-one"), &repository())
        .await
        .expect("repeat");

    assert_ne!(first.path, second.path);
    assert_eq!(first.path, repeated.path);
    fs::remove_dir_all(root).expect("remove temporary workspace root");
}

#[tokio::test]
async fn cleanup_refuses_root_and_unresolved_paths() {
    let root_dir = root();
    let root = root_dir.path();
    let manager = WorkspaceManager::new(root, FakeProvisioner);
    let workspace = manager
        .prepare(&lease("attempt-one"), &repository())
        .await
        .expect("workspace");
    let root_workspace = Workspace {
        attempt_id: workspace.attempt_id.clone(),
        id: workspace.id.clone(),
        path: root.to_path_buf(),
        base_revision: workspace.base_revision.clone(),
    };
    let unresolved = Workspace {
        attempt_id: workspace.attempt_id.clone(),
        id: workspace.id.clone(),
        path: root.join("not-created"),
        base_revision: workspace.base_revision.clone(),
    };

    assert_eq!(
        manager.cleanup(&root_workspace).expect("refuse root"),
        CleanupResult::Refused
    );
    assert!(matches!(
        manager.cleanup(&unresolved),
        Err(WorkspaceError::UnsafePath)
    ));
    assert_eq!(
        manager.cleanup(&workspace).expect("cleanup workspace"),
        CleanupResult::Deleted
    );
    fs::remove_dir_all(root).expect("remove temporary workspace root");
}

#[cfg(unix)]
#[tokio::test]
async fn cleanup_refuses_a_symlink_before_resolving_its_target() {
    use std::os::unix::fs::symlink;

    let root_dir = root();
    let root = root_dir.path();
    let manager = WorkspaceManager::new(root, FakeProvisioner);
    let workspace = manager
        .prepare(&lease("attempt-one"), &repository())
        .await
        .expect("workspace");
    let link = root.join("link-to-workspace");
    symlink(&workspace.path, &link).expect("create symlink");
    let linked_workspace = Workspace {
        path: link,
        ..workspace.clone()
    };

    assert_eq!(
        manager.cleanup(&linked_workspace).expect("refuse symlink"),
        CleanupResult::Refused
    );
    assert!(workspace.path.exists(), "symlink target is preserved");
    fs::remove_dir_all(root).expect("remove temporary workspace root");
}

#[cfg(unix)]
#[tokio::test]
async fn provision_rejects_an_existing_attempt_path_symlink() {
    use std::os::unix::fs::symlink;

    let root_dir = root();
    let root = root_dir.path();
    let manager = WorkspaceManager::new(root, FakeProvisioner);
    let planned = manager
        .plan(&lease("attempt-one"), &repository())
        .expect("plan");
    fs::create_dir_all(root.join("outside")).expect("outside");
    symlink(root.join("outside"), &planned.path).expect("attempt path symlink");

    assert!(matches!(
        manager.provision(&planned, &repository()).await,
        Err(WorkspaceError::UnsafePath)
    ));
    fs::remove_dir_all(root).expect("remove temporary workspace root");
}

#[tokio::test]
async fn cleanup_refuses_a_marker_that_does_not_match_the_workspace_identity() {
    let root_dir = root();
    let root = root_dir.path();
    let manager = WorkspaceManager::new(root, FakeProvisioner);
    let workspace = manager
        .prepare(&lease("attempt-one"), &repository())
        .await
        .expect("workspace");
    fs::write(workspace.path.join(".tack-attempt"), "other-attempt").expect("alter marker");

    assert_eq!(
        manager.cleanup(&workspace).expect("refuse marker"),
        CleanupResult::Refused
    );
    assert!(workspace.path.exists());
    fs::remove_dir_all(root).expect("remove temporary workspace root");
}

/// `ensure_safe_root` has two distinct refusals: "candidate equals root"
/// (covered above) and "this root is itself a real repository", detected
/// by a `.git` entry directly under it. Both `plan` and `cleanup` share
/// this guard; a workspace root that has been pointed at an actual
/// checkout must never be planned into or cleaned up, because cleanup
/// deletes directories.
#[tokio::test]
async fn cleanup_refuses_a_git_repository_root() {
    let root_dir = root();
    let root = root_dir.path();
    fs::create_dir_all(root.join(".git")).expect("simulate a real git repository");
    let canary = root.join("canary-attempt");
    fs::create_dir_all(&canary).expect("canary attempt directory");
    fs::write(canary.join(".tack-attempt"), "attempt-one").expect("canary marker");
    fs::write(canary.join("work.txt"), "do not delete").expect("canary file");

    let manager = WorkspaceManager::new(root, FakeProvisioner);
    let workspace = Workspace {
        attempt_id: AttemptId::new("attempt-one"),
        id: WorkspaceId::new("ws_canary"),
        path: canary.clone(),
        base_revision: "base".into(),
    };

    assert!(matches!(
        manager.cleanup(&workspace),
        Err(WorkspaceError::UnsafeRoot)
    ));
    // The guard is shared: `plan` must refuse the same root too, not only
    // `cleanup`.
    assert!(matches!(
        manager.plan(&lease("attempt-two"), &repository()),
        Err(WorkspaceError::UnsafeRoot)
    ));
    assert!(
        canary.exists(),
        "candidate directory survives a refused cleanup"
    );
    assert!(canary.join(".tack-attempt").exists());
    assert!(canary.join("work.txt").exists());
    fs::remove_dir_all(root).expect("remove temporary workspace root");
}

/// `cleanup` resolves the candidate with `canonicalize` and refuses
/// anything outside `root`. A literal `..` component is one way an
/// (accidentally or maliciously) constructed `Workspace` could try to
/// point outside the dedicated root without the final path component
/// itself being a symlink.
#[tokio::test]
async fn cleanup_refuses_a_dot_dot_traversal_outside_the_root() {
    let root_dir = root();
    let root = root_dir.path();
    let victim_dir = self::root();
    let victim = victim_dir.path();
    fs::create_dir_all(root).expect("workspace root");
    fs::create_dir_all(victim).expect("sibling victim directory");
    fs::write(victim.join("important.txt"), "do not delete").expect("victim file");

    let manager = WorkspaceManager::new(root, FakeProvisioner);
    let escaping_path = root
        .join("..")
        .join(victim.file_name().expect("victim directory name"));
    let workspace = Workspace {
        attempt_id: AttemptId::new("attempt-one"),
        id: WorkspaceId::new("ws_dotdot"),
        path: escaping_path,
        base_revision: "base".into(),
    };

    assert_eq!(
        manager.cleanup(&workspace).expect("refuse dot-dot escape"),
        CleanupResult::Refused
    );
    assert!(
        victim.exists(),
        "sibling directory reached via .. survives a refused cleanup"
    );
    assert!(victim.join("important.txt").exists());
}

/// The existing symlink test proves a symlink *as the final path
/// component* is refused before it is ever resolved. This proves the
/// complementary case: an intermediate path component that is a symlink
/// pointing outside `root`, where the final component itself is an
/// ordinary directory. `symlink_metadata` on the full path only inspects
/// the last component, so this can only be caught by the
/// `canonicalize` + `starts_with(root)` check, not the symlink check.
#[cfg(unix)]
#[tokio::test]
async fn cleanup_refuses_traversal_through_a_symlinked_intermediate_directory() {
    use std::os::unix::fs::symlink;

    let root_dir = root();
    let root = root_dir.path();
    let outside_dir = self::root();
    let outside = outside_dir.path();
    fs::create_dir_all(root).expect("workspace root");
    let victim = outside.join("victim");
    fs::create_dir_all(&victim).expect("victim directory");
    fs::write(victim.join("important.txt"), "do not delete").expect("victim file");

    let manager = WorkspaceManager::new(root, FakeProvisioner);
    let escape_link = root.join("escape");
    symlink(outside, &escape_link).expect("create escaping symlink");

    let workspace = Workspace {
        attempt_id: AttemptId::new("attempt-one"),
        id: WorkspaceId::new("ws_escape"),
        path: escape_link.join("victim"),
        base_revision: "base".into(),
    };

    assert_eq!(
        manager
            .cleanup(&workspace)
            .expect("refuse symlinked escape"),
        CleanupResult::Refused
    );
    assert!(
        victim.exists(),
        "victim directory outside root survives a refused cleanup"
    );
    assert!(victim.join("important.txt").exists());
    fs::remove_dir_all(outside).expect("remove temporary outside root");
}
