//! Corrupt-journal adversarial coverage for the local runner journal:
//! tests and the audit report only, no production source touched here.
//!
//! `OwnerOnlyJournal`'s in-module tests already cover a symlinked
//! journal/quarantine directory and a mismatched filename. This file
//! covers what those don't: a journal file corrupted on disk outside any
//! journal API call (bit rot, a truncated write after a crash mid-`fsync`,
//! an operator mistake), and what that does to *other, healthy* attempts'
//! recoverability, not just to itself.

use std::path::PathBuf;

use tack_runner::client::{
    AttemptId, AttemptJournal, AttemptLease, AttemptState, FencingToken, JournalError,
    JournalState, OwnerOnlyJournal, RunnerId, Timestamp, WorkspaceId, WorkspaceJournal,
};

mod common;
use common::temp_dir as temporary_root;

fn record(attempt_id: &str, fencing_token: u64) -> AttemptJournal {
    let lease = AttemptLease {
        attempt_id: AttemptId::new(attempt_id),
        runner_id: RunnerId::new("runner-g2-journal"),
        fencing_token: FencingToken(fencing_token),
        attempt_number: 1,
        state: AttemptState::Leased,
        issued_at: Timestamp::new("2026-08-19T12:00:00Z"),
        expires_at: Timestamp::new("2026-08-19T12:01:00Z"),
    };
    AttemptJournal::prepared(
        &lease,
        WorkspaceJournal {
            workspace_id: WorkspaceId::new("ws-g2"),
            path: PathBuf::from("workspace"),
            base_revision: "revision".into(),
        },
    )
}

// =======================================================================
// 1. A single bit-rotted journal file degrades to a typed `Malformed`
//    error, not a panic, when loaded directly by id.
// =======================================================================
#[test]
fn a_bit_rotted_journal_file_is_malformed_not_a_panic() {
    let root_dir = temporary_root("bitrot-single");
    let root = root_dir.path();
    let journal = OwnerOnlyJournal::new(root);
    let good = record("attempt-g2-good", 1);
    journal
        .persist_before_spawn(&good)
        .expect("persist healthy journal");

    // Simulate corruption written directly to disk, outside any journal
    // API — the scenario a real bit-rot or partial-write-after-crash event
    // produces.
    std::fs::write(
        journal.journal_path(&good.attempt_id),
        b"this is not valid TOML {{{ \x00\x01\x02 garbage",
    )
    .expect("corrupt the journal file directly");

    let loaded = journal.load(&good.attempt_id);
    assert!(
        matches!(loaded, Err(JournalError::Malformed)),
        "expected a typed Malformed error, got {loaded:?}"
    );

    let _ = std::fs::remove_dir_all(root);
}

// =======================================================================
// 2. Finding: one corrupted journal file among several currently blocks
//    the restart recovery scan (`unresolved()`) for *every* attempt, not
//    just the corrupted one — `unresolved()`'s `?` on `load_path` inside
//    its `fs::read_dir` loop propagates the very first `Malformed`/`Io`
//    error it meets, discarding whatever it had already found. This test
//    proves the behavior exists (a safe, documented state — no panic, no
//    blind respawn, no silent data loss) but flags it as an audit finding:
//    a single corrupted attempt's journal can currently deny recovery to
//    every other, healthy, unresolved attempt on the same runner restart.
//    Not fixed here — this file's scope is tests/audit only.
// =======================================================================
#[test]
fn a_corrupted_journal_file_blocks_recovery_of_other_attempts() {
    let root_dir = temporary_root("bitrot-batch");
    let root = root_dir.path();
    let journal = OwnerOnlyJournal::new(root);

    let healthy_a = record("attempt-g2-healthy-a", 1);
    let healthy_b = record("attempt-g2-healthy-b", 1);
    let corrupted = record("attempt-g2-corrupted", 1);
    journal
        .persist_before_spawn(&healthy_a)
        .expect("persist healthy a");
    journal
        .persist_before_spawn(&healthy_b)
        .expect("persist healthy b");
    journal
        .persist_before_spawn(&corrupted)
        .expect("persist the attempt that will be corrupted");

    // Before corruption: all three are correctly recoverable.
    let before = journal
        .unresolved()
        .expect("recovery scan before corruption");
    assert_eq!(before.len(), 3);

    std::fs::write(
        journal.journal_path(&corrupted.attempt_id),
        b"not valid toml {{{",
    )
    .expect("corrupt one journal file");

    let after = journal.unresolved();
    // Documented, safe (non-panicking, non-fabricating) but noteworthy:
    // the whole scan fails, not just the corrupted entry.
    assert!(
        matches!(after, Err(JournalError::Malformed)),
        "expected the batch scan to surface a typed error, got {after:?}"
    );

    // The two healthy journal files are themselves completely untouched by
    // this — proving the *data* survives even though the *scan* currently
    // cannot see past the corrupted entry. A future fix that makes
    // `unresolved()` skip-and-report per-file rather than abort-on-first
    // would find this data intact and recoverable.
    let still_readable_a = journal.load(&healthy_a.attempt_id);
    let still_readable_b = journal.load(&healthy_b.attempt_id);
    assert_eq!(still_readable_a.as_ref(), Ok(&healthy_a));
    assert_eq!(still_readable_b.as_ref(), Ok(&healthy_b));
    assert_eq!(
        still_readable_a.unwrap().state,
        JournalState::Prepared,
        "the healthy record's own state is untouched by its sibling's corruption"
    );

    let _ = std::fs::remove_dir_all(root);
}

// =======================================================================
// 3. A truncated (zero-length) journal file — the shape a crash exactly at
//    the moment of `create_new` before any bytes are flushed could leave —
//    is also a typed `Malformed` error, not a panic and not silently
//    treated as "no record" (which would wrongly let a second spawn
//    proceed as if this attempt had never been journaled).
// =======================================================================
#[test]
fn a_truncated_journal_file_is_malformed_not_missing() {
    let root_dir = temporary_root("truncated");
    let root = root_dir.path();
    let journal = OwnerOnlyJournal::new(root);
    let record = record("attempt-g2-truncated", 1);
    journal
        .persist_before_spawn(&record)
        .expect("persist journal");

    std::fs::write(journal.journal_path(&record.attempt_id), b"").expect("truncate to zero bytes");

    let loaded = journal.load(&record.attempt_id);
    assert!(
        matches!(loaded, Err(JournalError::Malformed)),
        "a truncated journal must not be confused with `Missing` (which would wrongly permit a second spawn): got {loaded:?}"
    );

    let _ = std::fs::remove_dir_all(root);
}
