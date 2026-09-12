use super::*;

/// A scratch directory that removes itself, and everything written under
/// it, when the returned guard drops — including when an assertion panics
/// first.
fn temp_dir(label: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(label)
        .tempdir()
        .expect("temporary directory")
}

#[test]
fn stages_a_file_with_a_correct_checksum_and_size() {
    let workspace_dir = temp_dir("workspace");
    let workspace = workspace_dir.path();
    let staging_dir = temp_dir("staging");
    let staging = staging_dir.path();
    fs::write(workspace.join("changes.patch"), b"diff --git a b\n").expect("write source");

    let stager = ArtifactStager::new(staging);
    let staged = stager
        .stage_file(
            "attempt-one",
            workspace,
            Path::new("changes.patch"),
            "patch",
            "text/x-diff",
        )
        .expect("stage file");

    assert_eq!(staged.name, "changes.patch");
    assert_eq!(staged.size_bytes, b"diff --git a b\n".len() as u64);
    assert_eq!(staged.sha256, sha256_hex(b"diff --git a b\n"));
    assert_eq!(
        fs::read(&staged.staged_path).expect("read staged file"),
        b"diff --git a b\n"
    );
    fs::remove_dir_all(workspace).expect("cleanup");
    fs::remove_dir_all(staging).expect("cleanup");
}

#[test]
fn refuses_a_source_outside_its_workspace_root() {
    let workspace_dir = temp_dir("workspace-escape");
    let workspace = workspace_dir.path();
    let staging_dir = temp_dir("staging-escape");
    let staging = staging_dir.path();
    let outside_dir = temp_dir("outside");
    let outside = outside_dir.path();
    fs::write(outside.join("secret.txt"), b"not part of this workspace").expect("write");

    let stager = ArtifactStager::new(staging);
    let escaping_relative = Path::new("..")
        .join(
            outside
                .file_name()
                .expect("outside dir name")
                .to_str()
                .expect("utf8"),
        )
        .join("secret.txt");

    let result = stager.stage_file(
        "attempt-one",
        workspace,
        &escaping_relative,
        "log",
        "text/plain",
    );
    assert!(matches!(result, Err(ArtifactError::WorkspaceEscape)));
    fs::remove_dir_all(workspace).expect("cleanup");
    fs::remove_dir_all(staging).expect("cleanup");
    fs::remove_dir_all(outside).expect("cleanup");
}

#[cfg(unix)]
#[test]
fn refuses_a_symlinked_source() {
    use std::os::unix::fs::symlink;

    let workspace_dir = temp_dir("workspace-symlink");
    let workspace = workspace_dir.path();
    let staging_dir = temp_dir("staging-symlink");
    let staging = staging_dir.path();
    let outside_dir = temp_dir("outside-symlink");
    let outside = outside_dir.path();
    fs::write(outside.join("real.txt"), b"outside content").expect("write outside file");
    symlink(outside.join("real.txt"), workspace.join("link.txt")).expect("symlink");

    let stager = ArtifactStager::new(staging);
    let result = stager.stage_file(
        "attempt-one",
        workspace,
        Path::new("link.txt"),
        "log",
        "text/plain",
    );
    assert!(matches!(result, Err(ArtifactError::SourceUnavailable)));
    fs::remove_dir_all(workspace).expect("cleanup");
    fs::remove_dir_all(staging).expect("cleanup");
    fs::remove_dir_all(outside).expect("cleanup");
}

/// Reinforces "adapters cannot cross-read each other's workspaces" on the
/// staging side: two attempts staging a same-named file land in
/// independent directories with independent content, never colliding.
#[test]
fn distinct_attempts_get_isolated_staging_directories() {
    let workspace_a_dir = temp_dir("workspace-a");
    let workspace_a = workspace_a_dir.path();
    let workspace_b_dir = temp_dir("workspace-b");
    let workspace_b = workspace_b_dir.path();
    let staging_dir = temp_dir("staging-shared");
    let staging = staging_dir.path();
    fs::write(workspace_a.join("out.txt"), b"attempt-a-content").expect("write a");
    fs::write(workspace_b.join("out.txt"), b"attempt-b-content").expect("write b");

    let stager = ArtifactStager::new(staging);
    let staged_a = stager
        .stage_file(
            "attempt-a",
            workspace_a,
            Path::new("out.txt"),
            "log",
            "text/plain",
        )
        .expect("stage a");
    let staged_b = stager
        .stage_file(
            "attempt-b",
            workspace_b,
            Path::new("out.txt"),
            "log",
            "text/plain",
        )
        .expect("stage b");

    assert_ne!(staged_a.staged_path, staged_b.staged_path);
    assert_eq!(
        fs::read(&staged_a.staged_path).unwrap(),
        b"attempt-a-content"
    );
    assert_eq!(
        fs::read(&staged_b.staged_path).unwrap(),
        b"attempt-b-content"
    );
    fs::remove_dir_all(workspace_a).expect("cleanup");
    fs::remove_dir_all(workspace_b).expect("cleanup");
    fs::remove_dir_all(staging).expect("cleanup");
}

#[cfg(unix)]
#[test]
fn staged_files_and_directories_are_owner_only() {
    use std::os::unix::fs::PermissionsExt;

    let workspace_dir = temp_dir("workspace-perms");
    let workspace = workspace_dir.path();
    let staging_dir = temp_dir("staging-perms");
    let staging = staging_dir.path();
    fs::write(workspace.join("out.txt"), b"content").expect("write");

    let stager = ArtifactStager::new(staging);
    let staged = stager
        .stage_file(
            "attempt-one",
            workspace,
            Path::new("out.txt"),
            "log",
            "text/plain",
        )
        .expect("stage");

    let file_mode = fs::metadata(&staged.staged_path)
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(file_mode & 0o077, 0, "staged file is owner-only");
    let dir_mode = fs::metadata(staged.staged_path.parent().unwrap())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(
        dir_mode & 0o077,
        0,
        "attempt staging directory is owner-only"
    );
    fs::remove_dir_all(workspace).expect("cleanup");
    fs::remove_dir_all(staging).expect("cleanup");
}
