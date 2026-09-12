//! Owner-only local attempt journal.
//!
//! A record is made durable before an adapter is allowed to start a process.
//! It contains no runner credential or raw harness environment.

use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use thiserror::Error;

use super::{AttemptId, AttemptLease, Checkpoint, FencingToken, RunnerId, WorkspaceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalState {
    Prepared,
    ProcessObservedRunning,
    CancellationRequested,
    TerminalReportPending,
    RecoveryObserved,
    Reported,
    Quarantined,
}

impl JournalState {
    pub const fn is_unresolved(self) -> bool {
        matches!(
            self,
            Self::Prepared
                | Self::ProcessObservedRunning
                | Self::CancellationRequested
                | Self::TerminalReportPending
        )
    }
}

/// The exact canonical JSON terminal payload that must be replayed after a
/// crash. It is intentionally independent from recovery observations: a
/// terminal report is never replaced with a different recovery request.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingTerminalReportKind {
    Completion,
    Cancellation,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PendingTerminalReport {
    pub kind: PendingTerminalReportKind,
    pub canonical_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceJournal {
    pub workspace_id: WorkspaceId,
    pub path: PathBuf,
    pub base_revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AttemptJournal {
    pub attempt_id: AttemptId,
    pub runner_id: RunnerId,
    pub fencing_token: FencingToken,
    pub workspace: WorkspaceJournal,
    pub state: JournalState,
    pub process_id: Option<String>,
    pub last_event_checkpoint: Option<Checkpoint>,
    /// Absent in old journals; present only after exact terminal payload fsync.
    #[serde(default)]
    pub pending_terminal_report: Option<PendingTerminalReport>,
}

impl AttemptJournal {
    pub fn prepared(lease: &AttemptLease, workspace: WorkspaceJournal) -> Self {
        Self {
            attempt_id: lease.attempt_id.clone(),
            runner_id: lease.runner_id.clone(),
            fencing_token: lease.fencing_token,
            workspace,
            state: JournalState::Prepared,
            process_id: None,
            last_event_checkpoint: None,
            pending_terminal_report: None,
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum JournalError {
    #[error("runner journal could not be initialized")]
    Initialization,
    #[error("runner journal record already exists")]
    AlreadyExists,
    #[error("runner journal record is missing")]
    Missing,
    #[error("runner journal could not be serialized")]
    Serialization,
    #[error("runner journal is malformed")]
    Malformed,
    #[error("runner journal operation failed")]
    Io,
}

/// Durable storage rooted in local runner state. File names are hex-encoded
/// opaque attempt IDs, so input cannot traverse outside the journal directory.
#[derive(Debug, Clone)]
pub struct OwnerOnlyJournal {
    root: PathBuf,
    #[cfg(test)]
    fail_next_update: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl OwnerOnlyJournal {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            #[cfg(test)]
            fail_next_update: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn journal_path(&self, attempt_id: &AttemptId) -> PathBuf {
        self.journal_dir()
            .join(format!("{}.toml", encode_id(attempt_id.as_str())))
    }

    pub fn persist_before_spawn(&self, record: &AttemptJournal) -> Result<(), JournalError> {
        self.ensure_layout()?;
        if self.quarantine_path(&record.attempt_id).exists() {
            return Err(JournalError::AlreadyExists);
        }
        let bytes = toml::to_string(record).map_err(|_| JournalError::Serialization)?;
        atomic_create_private(&self.journal_path(&record.attempt_id), bytes.as_bytes())
    }

    pub fn update(&self, record: &AttemptJournal) -> Result<(), JournalError> {
        #[cfg(test)]
        if self
            .fail_next_update
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            return Err(JournalError::Io);
        }
        self.ensure_layout()?;
        let path = self.journal_path(&record.attempt_id);
        if !path.exists() {
            return Err(JournalError::Missing);
        }
        let bytes = toml::to_string(record).map_err(|_| JournalError::Serialization)?;
        atomic_replace_private(&path, bytes.as_bytes())
    }

    #[cfg(test)]
    pub(crate) fn fail_next_update_for_test(&self) {
        self.fail_next_update
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn load(&self, attempt_id: &AttemptId) -> Result<AttemptJournal, JournalError> {
        self.load_path(&self.journal_path(attempt_id))
    }

    pub fn unresolved(&self) -> Result<Vec<AttemptJournal>, JournalError> {
        self.ensure_layout()?;
        let mut records = Vec::new();
        for entry in fs::read_dir(self.journal_dir()).map_err(|_| JournalError::Io)? {
            let entry = entry.map_err(|_| JournalError::Io)?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("toml") {
                continue;
            }
            if fs::symlink_metadata(&path)
                .map_err(|_| JournalError::Io)?
                .file_type()
                .is_symlink()
            {
                return Err(JournalError::Malformed);
            }
            let record = self.load_path(&path)?;
            if record.state.is_unresolved() {
                records.push(record);
            }
        }
        records.sort_by(|left, right| left.attempt_id.as_str().cmp(right.attempt_id.as_str()));
        Ok(records)
    }

    /// Moves only server-acknowledged ambiguous evidence out of restart scans.
    /// The source is deliberately left untouched until the non-overwriting move
    /// is durable, so a failed delivery remains retryable on the next restart.
    pub fn quarantine(&self, record: &AttemptJournal) -> Result<(), JournalError> {
        self.ensure_layout()?;
        let source = self.journal_path(&record.attempt_id);
        let destination = self.quarantine_path(&record.attempt_id);
        if destination.exists() {
            return Err(JournalError::AlreadyExists);
        }
        fs::rename(&source, &destination).map_err(|_| JournalError::Io)?;
        // `journal` and `quarantine` are sibling directories. Sync both so a
        // cross-directory rename survives a crash without silently losing the
        // last recoverable record.
        sync_directory(&self.journal_dir())?;
        sync_directory(&self.quarantine_dir())
    }

    fn load_path(&self, path: &Path) -> Result<AttemptJournal, JournalError> {
        let contents = fs::read_to_string(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                JournalError::Missing
            } else {
                JournalError::Io
            }
        })?;
        let record: AttemptJournal =
            toml::from_str(&contents).map_err(|_| JournalError::Malformed)?;
        let expected_name = format!("{}.toml", encode_id(record.attempt_id.as_str()));
        if path.file_name().and_then(|name| name.to_str()) != Some(expected_name.as_str()) {
            return Err(JournalError::Malformed);
        }
        Ok(record)
    }

    fn journal_dir(&self) -> PathBuf {
        self.root.join("journal")
    }

    fn quarantine_dir(&self) -> PathBuf {
        self.root.join("quarantine")
    }

    fn quarantine_path(&self, attempt_id: &AttemptId) -> PathBuf {
        self.quarantine_dir()
            .join(format!("{}.toml", encode_id(attempt_id.as_str())))
    }

    fn ensure_layout(&self) -> Result<(), JournalError> {
        create_secure_directory(&self.root)?;
        create_secure_directory(&self.journal_dir())?;
        create_secure_directory(&self.quarantine_dir())?;
        Ok(())
    }
}

fn create_secure_directory(path: &Path) -> Result<(), JournalError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(JournalError::Initialization);
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(|_| JournalError::Initialization)?;
        }
        Err(_) => return Err(JournalError::Initialization),
    }
    make_owner_only(path).map_err(|_| JournalError::Initialization)
}

fn encode_id(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn atomic_create_private(path: &Path, bytes: &[u8]) -> Result<(), JournalError> {
    let temporary = write_private_temporary(path, bytes)?;
    match fs::hard_link(&temporary, path) {
        Ok(()) => {
            fs::remove_file(&temporary).map_err(|_| JournalError::Io)?;
            sync_directory(path.parent().ok_or(JournalError::Io)?)
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                Err(JournalError::AlreadyExists)
            } else {
                Err(JournalError::Io)
            }
        }
    }
}

fn atomic_replace_private(path: &Path, bytes: &[u8]) -> Result<(), JournalError> {
    let temporary = write_private_temporary(path, bytes)?;
    fs::rename(&temporary, path).map_err(|_| JournalError::Io)?;
    make_owner_only(path).map_err(|_| JournalError::Io)?;
    sync_directory(path.parent().ok_or(JournalError::Io)?)
}

fn write_private_temporary(path: &Path, bytes: &[u8]) -> Result<PathBuf, JournalError> {
    let name = path.file_name().ok_or(JournalError::Io)?;
    let temporary = path.with_file_name(format!(
        ".{}.{}.tmp",
        name.to_string_lossy(),
        std::process::id()
    ));
    let mut file = open_private_new(&temporary)?;
    file.write_all(bytes).map_err(|_| JournalError::Io)?;
    file.sync_all().map_err(|_| JournalError::Io)?;
    Ok(temporary)
}

#[cfg(unix)]
fn open_private_new(path: &Path) -> Result<File, JournalError> {
    use std::os::unix::fs::OpenOptionsExt;

    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| JournalError::Io)
}

#[cfg(not(unix))]
fn open_private_new(path: &Path) -> Result<File, JournalError> {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| JournalError::Io)?;
    make_owner_only(path).map_err(|_| JournalError::Io)?;
    Ok(file)
}

fn sync_directory(path: &Path) -> Result<(), JournalError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| JournalError::Io)
}

#[cfg(unix)]
fn make_owner_only(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(
        path,
        fs::Permissions::from_mode(if path.is_file() { 0o600 } else { 0o700 }),
    )
}

#[cfg(not(unix))]
fn make_owner_only(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
#[path = "journal/tests.rs"]
mod tests;
