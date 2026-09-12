use std::fs;

use super::*;
use crate::client::{AttemptState, Timestamp};

/// A scratch directory that removes itself, and everything written under
/// it, when the returned guard drops — including when an assertion panics
/// first.
fn temporary_root() -> tempfile::TempDir {
    tempfile::tempdir().expect("temporary directory")
}

fn record() -> AttemptJournal {
    let lease = AttemptLease {
        attempt_id: AttemptId::new("att/opaque"),
        runner_id: RunnerId::new("runner"),
        fencing_token: FencingToken(7),
        attempt_number: 1,
        state: AttemptState::Leased,
        issued_at: Timestamp::new("2026-08-06T12:20:00Z"),
        expires_at: Timestamp::new("2026-08-06T12:21:00Z"),
    };
    AttemptJournal::prepared(
        &lease,
        WorkspaceJournal {
            workspace_id: WorkspaceId::new("ws"),
            path: PathBuf::from("workspace"),
            base_revision: "revision".into(),
        },
    )
}

#[test]
fn pre_spawn_journal_is_atomic_owner_only_and_recoverable() {
    let root_dir = temporary_root();
    let root = root_dir.path();
    let journal = OwnerOnlyJournal::new(root);
    let record = record();
    journal
        .persist_before_spawn(&record)
        .expect("persist journal");

    assert_eq!(
        journal.load(&record.attempt_id).expect("load journal"),
        record
    );
    assert_eq!(
        journal.unresolved().expect("recovery scan"),
        vec![record.clone()]
    );
    assert!(matches!(
        journal.persist_before_spawn(&record),
        Err(JournalError::AlreadyExists)
    ));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(journal.journal_path(&record.attempt_id))
            .expect("journal metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "journal is owner-only");
    }
    fs::remove_dir_all(root).expect("remove temporary journal root");
}

#[test]
fn legacy_journal_without_pending_terminal_report_remains_readable() {
    let record = record();
    let encoded = toml::to_string(&record).expect("encode journal");
    let legacy = encoded
        .lines()
        .filter(|line| !line.starts_with("pending_terminal_report"))
        .collect::<Vec<_>>()
        .join("\n");
    let decoded: AttemptJournal = toml::from_str(&legacy).expect("decode legacy journal");
    assert_eq!(decoded.pending_terminal_report, None);
    assert_eq!(decoded.state, JournalState::Prepared);
}

#[test]
fn load_and_recovery_reject_a_filename_that_disagrees_with_the_record_attempt() {
    let root_dir = temporary_root();
    let root = root_dir.path();
    let journal = OwnerOnlyJournal::new(root);
    let record = record();
    journal
        .persist_before_spawn(&record)
        .expect("persist journal");
    let substituted_name = AttemptId::new("another-attempt");
    fs::rename(
        journal.journal_path(&record.attempt_id),
        journal.journal_path(&substituted_name),
    )
    .expect("tamper filename");

    assert!(matches!(
        journal.load(&substituted_name),
        Err(JournalError::Malformed)
    ));
    assert!(matches!(journal.unresolved(), Err(JournalError::Malformed)));
    fs::remove_dir_all(root).expect("remove temporary journal root");
}

#[test]
fn quarantined_attempt_cannot_be_persisted_for_a_second_spawn() {
    let root_dir = temporary_root();
    let root = root_dir.path();
    let journal = OwnerOnlyJournal::new(root);
    let record = record();
    journal
        .persist_before_spawn(&record)
        .expect("persist journal");
    journal.quarantine(&record).expect("quarantine journal");

    assert!(matches!(
        journal.quarantine(&record),
        Err(JournalError::AlreadyExists)
    ));
    assert!(matches!(
        journal.persist_before_spawn(&record),
        Err(JournalError::AlreadyExists)
    ));
    fs::remove_dir_all(root).expect("remove temporary journal root");
}

#[cfg(unix)]
#[test]
fn symlinked_journal_directories_are_rejected() {
    use std::os::unix::fs::symlink;

    for name in ["root", "journal", "quarantine"] {
        let guard = temporary_root();
        // `root` must not exist yet: the "root" case symlinks the path
        // itself, and `target` has to be a sibling it can point at.
        let root = guard.path().join(name);
        let target = guard.path().join(format!("{name}-target"));
        fs::create_dir_all(&target).expect("target directory");
        match name {
            "root" => symlink(&target, &root).expect("root symlink"),
            "journal" => {
                fs::create_dir(&root).expect("root directory");
                symlink(&target, root.join("journal")).expect("journal symlink");
            }
            "quarantine" => {
                fs::create_dir_all(root.join("journal")).expect("journal directory");
                symlink(&target, root.join("quarantine")).expect("quarantine symlink");
            }
            _ => unreachable!(),
        }
        let journal = OwnerOnlyJournal::new(&root);
        assert!(matches!(
            journal.persist_before_spawn(&record()),
            Err(JournalError::Initialization)
        ));
        let _ = fs::remove_file(&root);
        let _ = fs::remove_dir_all(&root);
        fs::remove_dir_all(target).expect("remove target");
    }
}
