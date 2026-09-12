//! Safe, streamed artifact-content storage.
//!
//! Server-side counterpart to `tack-runner`'s `harness::artifact::ArtifactStager`
//! (the local half — see that module's own doc comment: "no artifact-upload
//! method yet ... this module is the local half only"). This module is the
//! remote half: it receives whatever bytes a runner PUTs against
//! `/api/runner/v1/attempts/{attempt_id}/artifacts/{artifact_id}/content`
//! and commits them to `TACK_STORAGE_DIR` only after they are proven to
//! match the manifest's declared `size_bytes`/`sha256` — a mismatch of
//! either kind stages nothing (no blob, no `content_reference`).
//!
//! Three properties this module exists to guarantee, all proved by its own
//! test suite below:
//!
//! - **Bounded memory.** [`ArtifactStorage::store_streaming`] never holds
//!   more than one chunk plus a running SHA-256 state at a time — it writes
//!   and hashes each chunk as it arrives and drops it, rather than
//!   collecting the stream into a `Vec<Bytes>` first. `store_streaming`
//!   aborts (deletes its temp file, returns
//!   [`ArtifactContentError::OversizeStream`]) the instant more bytes have
//!   arrived than the manifest declared — this is what defeats a
//!   "compression bomb"-style attack (a small claimed size, an arbitrarily
//!   large or unbounded actual body): the cap is enforced *while consuming*
//!   the stream, never after fully materializing it.
//! - **No path traversal, no symlink escape.** `attempt_id` and
//!   `artifact_id` are both runner-supplied, opaque strings this module must
//!   never trust as literal path components. Every path is built from a
//!   hex-encoding of the id (mirroring `tack-runner`'s own `encode_id`
//!   convention in `harness/artifact.rs`) — a `..`, `/`, or NUL byte in an id
//!   becomes two harmless hex digits, so traversal via id content is
//!   structurally impossible, not merely rejected by a string check. Every
//!   temp file is opened with `create_new(true)` (refuses to follow an
//!   existing symlink or overwrite an existing file), and every directory is
//!   canonicalized and checked to still be contained within the
//!   canonicalized storage root before any write — defeating a
//!   symlink-swapped attempt directory, not just a string pattern.
//! - **Checksum/size mismatch stages nothing.** The temp file is only ever
//!   renamed into its final, content-addressed location after both checks
//!   pass; any failure path deletes the temp file and returns before the
//!   caller ever calls `set_execution_artifact_content_reference`.

use std::path::{Path, PathBuf};

use axum::body::Bytes;
use futures::{Stream, StreamExt};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ArtifactContentError {
    #[error("the storage root or attempt directory is not a safe location to write to")]
    UnsafeStorageLocation,
    #[error("could not perform a required filesystem operation")]
    Io,
    #[error("more bytes arrived than the manifest declared")]
    OversizeStream,
    #[error("the stream ended with fewer bytes than the manifest declared")]
    SizeMismatch,
    #[error("the uploaded content's checksum does not match the manifest")]
    ChecksumMismatch,
    #[error("the upload stream reported an error before completing")]
    StreamRead,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredArtifactContent {
    /// Relative to [`ArtifactStorage`]'s own root — portable, the same
    /// convention `attachments.rs` already uses for `storage_path`. This is
    /// the exact value a caller should persist as
    /// `execution_artifacts.content_reference`.
    pub content_reference: String,
    pub bytes_written: u64,
}

/// SHA-256-hashes `value` and hex-encodes the fixed-size digest, so the
/// result can never be interpreted as a path separator, a `..` traversal
/// component or a NUL terminator (same defense `tack-runner`'s
/// `harness/artifact.rs#encode_id` uses on the local side) **and** is always
/// exactly 64 bytes regardless of `value`'s own length.
///
/// This must hash rather than hex-encode `value` literally: `tack-runner`'s
/// own `engine.rs::artifact_id` derives an `artifact_id` by hex-encoding
/// `"{attempt_id}:{fencing_token}:{sha256}"` (already ~220 bytes), and this
/// value is used twice per filename (once for the temp name, once for the
/// final blob name). A literal hex-encode of that id blows past Linux's
/// 255-byte `NAME_MAX`, so every write fails with `ENAMETOOLONG` inside
/// `Io`, surfaced to the caller as a bare `500`. Hashing bounds the length
/// unconditionally; content is never read back by literal id
/// (`open_for_read` takes the already-produced `content_reference`), so
/// losing reversibility costs nothing.
fn encode_id(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

/// Roots every stored artifact under `root/<hex(attempt_id)>/...`, mirroring
/// `tack-runner`'s `ArtifactStager` layout on the server side.
#[derive(Debug, Clone)]
pub struct ArtifactStorage {
    root: PathBuf,
}

impl ArtifactStorage {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Creates (if needed) and returns the canonicalized, containment-checked
    /// attempt directory. Refuses to follow a pre-existing symlink at either
    /// the root or the attempt-directory path.
    async fn safe_attempt_dir(&self, attempt_id: &str) -> Result<PathBuf, ArtifactContentError> {
        tokio::fs::create_dir_all(&self.root)
            .await
            .map_err(|_| ArtifactContentError::Io)?;
        reject_symlink(&self.root).await?;
        let canonical_root = tokio::fs::canonicalize(&self.root)
            .await
            .map_err(|_| ArtifactContentError::Io)?;

        let attempt_dir = self.root.join(encode_id(attempt_id));
        // Inspect *before* creating: `create_dir_all` on a path that is
        // already a symlink-to-a-directory silently succeeds and every
        // subsequent write follows the symlink to wherever it points.
        if path_exists(&attempt_dir).await {
            reject_symlink(&attempt_dir).await?;
        }
        tokio::fs::create_dir_all(&attempt_dir)
            .await
            .map_err(|_| ArtifactContentError::Io)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ =
                tokio::fs::set_permissions(&attempt_dir, std::fs::Permissions::from_mode(0o700))
                    .await;
        }
        let canonical_attempt_dir = tokio::fs::canonicalize(&attempt_dir)
            .await
            .map_err(|_| ArtifactContentError::Io)?;
        if canonical_attempt_dir != canonical_root
            && !canonical_attempt_dir.starts_with(&canonical_root)
        {
            return Err(ArtifactContentError::UnsafeStorageLocation);
        }
        Ok(canonical_attempt_dir)
    }

    /// Streams `body` to a temp file inside `attempt_id`'s safe directory,
    /// hashing as it writes, and only commits (renames into its final,
    /// content-addressed path) once the total byte count and the final
    /// SHA-256 both match the manifest's declared values exactly. Any
    /// mismatch — oversize, short, or wrong checksum — deletes the temp file
    /// and returns before anything is committed.
    ///
    /// Bounded memory: only the current chunk and the running hash state are
    /// ever held; nothing is buffered whole. See this module's own doc
    /// comment for how the "compression bomb" defense follows directly from
    /// this.
    pub async fn store_streaming<S, E>(
        &self,
        attempt_id: &str,
        artifact_id: &str,
        declared_size_bytes: u64,
        declared_sha256: &str,
        mut body: S,
    ) -> Result<StoredArtifactContent, ArtifactContentError>
    where
        S: Stream<Item = Result<Bytes, E>> + Unpin,
    {
        let attempt_dir = self.safe_attempt_dir(attempt_id).await?;
        let temp_name = format!("{}.tmp-{}", encode_id(artifact_id), uuid::Uuid::new_v4());
        let temp_path = attempt_dir.join(&temp_name);

        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true) // Never follows an existing symlink or file.
            .open(&temp_path)
            .await
            .map_err(|_| ArtifactContentError::Io)?;

        let mut hasher = Sha256::new();
        let mut total_written: u64 = 0;
        let mut failure: Option<ArtifactContentError> = None;

        while let Some(chunk) = body.next().await {
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(_) => {
                    failure = Some(ArtifactContentError::StreamRead);
                    break;
                }
            };
            total_written = total_written.saturating_add(chunk.len() as u64);
            if total_written > declared_size_bytes {
                failure = Some(ArtifactContentError::OversizeStream);
                break;
            }
            hasher.update(&chunk);
            if file.write_all(&chunk).await.is_err() {
                failure = Some(ArtifactContentError::Io);
                break;
            }
        }

        if let Some(error) = failure {
            drop(file);
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(error);
        }
        if file.flush().await.is_err() {
            drop(file);
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(ArtifactContentError::Io);
        }
        drop(file);

        if total_written != declared_size_bytes {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(ArtifactContentError::SizeMismatch);
        }
        let digest = hex::encode(hasher.finalize());
        if digest != declared_sha256 {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(ArtifactContentError::ChecksumMismatch);
        }

        let final_name = format!("{}-{}.blob", encode_id(artifact_id), &digest[..16]);
        let final_path = attempt_dir.join(&final_name);
        if tokio::fs::rename(&temp_path, &final_path).await.is_err() {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(ArtifactContentError::Io);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = tokio::fs::set_permissions(&final_path, std::fs::Permissions::from_mode(0o600))
                .await;
        }

        let content_reference = format!("{}/{}", encode_id(attempt_id), final_name);
        Ok(StoredArtifactContent {
            content_reference,
            bytes_written: total_written,
        })
    }

    /// Opens a previously stored blob for streamed reading. `content_reference`
    /// must be a value this module itself produced (persisted verbatim in
    /// `execution_artifacts.content_reference`) — never a caller-supplied
    /// path. Still re-validates containment defensively: a `content_reference`
    /// value that somehow resolves outside the storage root is refused
    /// rather than opened.
    pub async fn open_for_read(
        &self,
        content_reference: &str,
    ) -> Result<tokio::fs::File, ArtifactContentError> {
        let candidate = self.root.join(content_reference);
        reject_symlink(&candidate).await?;
        let canonical_root = tokio::fs::canonicalize(&self.root)
            .await
            .map_err(|_| ArtifactContentError::Io)?;
        let canonical_candidate = tokio::fs::canonicalize(&candidate)
            .await
            .map_err(|_| ArtifactContentError::Io)?;
        if canonical_candidate != canonical_root
            && !canonical_candidate.starts_with(&canonical_root)
        {
            return Err(ArtifactContentError::UnsafeStorageLocation);
        }
        tokio::fs::File::open(&canonical_candidate)
            .await
            .map_err(|_| ArtifactContentError::Io)
    }

    /// Best-effort blob removal for a swept artifact row.
    /// "Best effort" — a blob that is already gone (or was never written,
    /// `content_reference: None`) is not an error; the caller is purging a
    /// DB row either way.
    pub async fn remove_blob(&self, content_reference: &str) {
        let candidate = self.root.join(content_reference);
        let _ = tokio::fs::remove_file(&candidate).await;
    }
}

async fn path_exists(path: &Path) -> bool {
    tokio::fs::symlink_metadata(path).await.is_ok()
}

async fn reject_symlink(path: &Path) -> Result<(), ArtifactContentError> {
    match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(ArtifactContentError::UnsafeStorageLocation)
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
#[path = "artifact_storage/tests.rs"]
mod tests;
