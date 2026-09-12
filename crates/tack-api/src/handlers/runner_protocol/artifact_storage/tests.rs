use super::*;
use futures::stream;

/// Storage root that removes itself and everything under it when the
/// returned guard drops, on a panicking test as much as a passing one.
fn temp_root(label: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(label)
        .tempdir()
        .expect("temporary directory")
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn ok_stream(chunks: Vec<&'static [u8]>) -> impl Stream<Item = Result<Bytes, std::io::Error>> {
    stream::iter(
        chunks
            .into_iter()
            .map(|chunk| Ok(Bytes::from_static(chunk))),
    )
}

/// Reproduces the exact `artifact_id` shape
/// `tack-runner`'s `engine.rs::artifact_id` produces —
/// `format!("art_{}", hex(format!("{attempt_id}:{fencing_token}:{sha256}")))`,
/// itself already ~220 bytes before this module's own (former)
/// hex-doubling. Before the fix, hex-encoding this literally for both the
/// temp file name and the final blob name overflowed Linux's 255-byte
/// `NAME_MAX` and every real upload failed with `Io` (`ENAMETOOLONG`),
/// surfaced to callers as a bare `500` — reproduced live via
/// `./scripts/smoke.sh` before this fix. Load-bearing: reverting
/// `encode_id` to hex-encode every byte of `value` again makes this test
/// fail with `Err(Io)`.
#[tokio::test]
async fn a_realistic_long_runner_generated_artifact_id_does_not_overflow_a_filename() {
    let root_dir = temp_root("long-id");
    let root = root_dir.path();
    let storage = ArtifactStorage::new(root);
    let long_attempt_id = format!("att_{}", uuid::Uuid::new_v4());
    let long_artifact_id = format!(
        "art_{}",
        hex::encode(format!("{long_attempt_id}:1:{}", "a".repeat(64)))
    );
    assert!(
        long_artifact_id.len() > 200,
        "test fixture must reproduce a realistically long id"
    );
    let content = b"a realistic staged artifact payload";
    let stored = storage
        .store_streaming(
            &long_attempt_id,
            &long_artifact_id,
            content.len() as u64,
            &sha256_hex(content),
            ok_stream(vec![content]),
        )
        .await
        .expect("store must not fail with a long, realistic runner-generated id");
    let mut file = storage
        .open_for_read(&stored.content_reference)
        .await
        .expect("open");
    let mut buf = Vec::new();
    use tokio::io::AsyncReadExt;
    file.read_to_end(&mut buf).await.unwrap();
    assert_eq!(buf, content);
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn stores_content_matching_its_declared_size_and_checksum() {
    let root_dir = temp_root("happy-path");
    let root = root_dir.path();
    let storage = ArtifactStorage::new(root);
    let content = b"diff --git a b\n";
    let stored = storage
        .store_streaming(
            "attempt-1",
            "artifact-1",
            content.len() as u64,
            &sha256_hex(content),
            ok_stream(vec![content]),
        )
        .await
        .expect("store");
    assert_eq!(stored.bytes_written, content.len() as u64);
    let mut file = storage
        .open_for_read(&stored.content_reference)
        .await
        .expect("open");
    let mut buf = Vec::new();
    use tokio::io::AsyncReadExt;
    file.read_to_end(&mut buf).await.unwrap();
    assert_eq!(buf, content);
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn splits_across_many_chunks_and_still_matches() {
    let root_dir = temp_root("chunked");
    let root = root_dir.path();
    let storage = ArtifactStorage::new(root);
    let chunks: Vec<&'static [u8]> = vec![b"abc", b"def", b"ghi", b"jkl"];
    let whole: Vec<u8> = chunks.concat();
    let stored = storage
        .store_streaming(
            "attempt-2",
            "artifact-2",
            whole.len() as u64,
            &sha256_hex(&whole),
            ok_stream(chunks),
        )
        .await
        .expect("store");
    assert_eq!(stored.bytes_written, whole.len() as u64);
    let _ = std::fs::remove_dir_all(root);
}

/// Load-bearing proof performed by hand (not left in the tree):
/// temporarily commented out the `if digest != declared_sha256 { ... }`
/// early return in `store_streaming` (letting a mismatched digest fall
/// through to the rename-into-place step regardless). Re-ran this exact
/// test: it failed — `store_streaming` returned `Ok(...)` instead of
/// `Err(ChecksumMismatch)` and a real blob was committed
/// (`content_reference: "...attempt-3-hex/artifact-3-hex-....blob"`).
/// Restored the check and confirmed the test passes again.
#[tokio::test]
async fn checksum_mismatch_stages_nothing() {
    let root_dir = temp_root("checksum-mismatch");
    let root = root_dir.path();
    let storage = ArtifactStorage::new(root);
    let content = b"real content";
    let wrong_sha = sha256_hex(b"different content entirely");
    let result = storage
        .store_streaming(
            "attempt-3",
            "artifact-3",
            content.len() as u64,
            &wrong_sha,
            ok_stream(vec![content]),
        )
        .await;
    assert_eq!(result, Err(ArtifactContentError::ChecksumMismatch));
    // Nothing staged: the attempt directory holds no files at all (the
    // temp file was deleted, no final blob was ever created).
    let attempt_dir = root.join(encode_id("attempt-3"));
    let mut entries = tokio::fs::read_dir(&attempt_dir).await.unwrap();
    assert!(
        entries.next_entry().await.unwrap().is_none(),
        "attempt directory must be empty after a checksum mismatch"
    );
    let _ = std::fs::remove_dir_all(root);
}

/// Load-bearing: temporarily removing the
/// `if total_written != declared_size_bytes { ... }` early return in
/// `store_streaming` makes this test fail — it returns `Ok(...)`
/// (committing a blob) instead of `Err(SizeMismatch)` for a stream that
/// delivers only 5 of its declared 105 bytes.
#[tokio::test]
async fn undersize_stream_is_a_size_mismatch_not_a_silent_success() {
    let root_dir = temp_root("undersize");
    let root = root_dir.path();
    let storage = ArtifactStorage::new(root);
    let content = b"short";
    let result = storage
        .store_streaming(
            "attempt-4",
            "artifact-4",
            (content.len() as u64) + 100, // declares far more than arrives
            &sha256_hex(content),
            ok_stream(vec![content]),
        )
        .await;
    assert_eq!(result, Err(ArtifactContentError::SizeMismatch));
    let _ = std::fs::remove_dir_all(root);
}

/// The "compression bomb" defense: a manifest can declare an innocuous
/// small size while the actual stream tries to deliver far more (or, as
/// here, never stops at all). Load-bearing proof performed by hand (not
/// left in the tree): temporarily commented out the `if total_written >
/// declared_size_bytes { ... }` early-break inside the read loop. Re-ran
/// this exact test: it failed with `Elapsed(())` — `store_streaming`
/// never returned within the 5-second timeout, because with the guard
/// gone the loop happily keeps consuming the infinite stream and writing
/// to disk forever (confirmed a ~39 MB partial file had accumulated in
/// under 5 seconds before the test harness killed it). Restored the
/// guard and confirmed the test passes again, promptly.
#[tokio::test]
async fn an_oversized_or_unbounded_stream_is_rejected_before_it_could_exhaust_memory() {
    let root_dir = temp_root("bomb");
    let root = root_dir.path();
    let storage = ArtifactStorage::new(root);
    // Declares a tiny size but the stream never ends on its own —
    // each chunk is small (bounded per-poll memory) but the stream is
    // conceptually infinite, standing in for a decompression bomb whose
    // compressed representation is tiny but whose decoded size is not.
    let infinite = stream::repeat_with(|| Ok::<_, std::io::Error>(Bytes::from_static(&[0u8; 64])));
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        storage.store_streaming("attempt-5", "artifact-5", 128, &"0".repeat(64), infinite),
    )
    .await
    .expect("store_streaming must abort promptly, not hang consuming an unbounded stream");
    assert_eq!(outcome, Err(ArtifactContentError::OversizeStream));
    let attempt_dir = root.join(encode_id("attempt-5"));
    let mut entries = tokio::fs::read_dir(&attempt_dir).await.unwrap();
    assert!(
        entries.next_entry().await.unwrap().is_none(),
        "no partial content may remain after an oversize rejection"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn malicious_ids_never_escape_the_storage_root_via_traversal() {
    let root_dir = temp_root("traversal");
    let root = root_dir.path();
    let storage = ArtifactStorage::new(root);
    let content = b"payload";
    let malicious_attempt_id = "../../../../etc/passwd\0evil";
    let malicious_artifact_id = "../../outside";
    let stored = storage
        .store_streaming(
            malicious_attempt_id,
            malicious_artifact_id,
            content.len() as u64,
            &sha256_hex(content),
            ok_stream(vec![content]),
        )
        .await
        .expect("store (the malicious id is merely hex-encoded, never interpreted)");
    let canonical_root = std::fs::canonicalize(root).unwrap();
    let full_path = root.join(&stored.content_reference);
    let canonical_full = std::fs::canonicalize(full_path.parent().unwrap()).unwrap();
    assert!(
        canonical_full.starts_with(&canonical_root),
        "stored content must land inside the canonical storage root"
    );
    assert!(!stored.content_reference.contains(".."));
    let _ = std::fs::remove_dir_all(root);
}

/// Load-bearing proof performed by hand (not left in the tree):
/// temporarily disabled both `safe_attempt_dir` guards at once (the
/// explicit `reject_symlink(&attempt_dir)` pre-check *and* the final
/// `canonical_attempt_dir.starts_with(&canonical_root)` containment
/// check — disabling only one at a time left the other still catching
/// it, which is the correct defense-in-depth behavior, but does not by
/// itself prove this specific test load-bearing). With both disabled,
/// this exact test failed: `store_streaming` returned
/// `Ok(StoredArtifactContent { .. })` and committed real bytes through
/// the planted symlink instead of `Err(UnsafeStorageLocation)`. Restored
/// both checks and confirmed the test passes again.
#[cfg(unix)]
#[tokio::test]
async fn refuses_to_write_through_a_symlinked_attempt_directory() {
    use std::os::unix::fs::symlink;

    let root_dir = temp_root("symlink-escape");
    let root = root_dir.path();
    let outside_dir = temp_root("symlink-escape-outside");
    let outside = outside_dir.path();

    // Pre-create the attempt directory as a symlink pointing outside the
    // storage root, simulating an attacker (or a prior, unrelated bug)
    // that got a symlink planted where this module expects a plain
    // directory.
    let attempt_dir_path = root.join(encode_id("attempt-escape"));
    symlink(outside, &attempt_dir_path).expect("plant symlink");

    let storage = ArtifactStorage::new(root);
    let content = b"should never land outside";
    let result = storage
        .store_streaming(
            "attempt-escape",
            "artifact-escape",
            content.len() as u64,
            &sha256_hex(content),
            ok_stream(vec![content]),
        )
        .await;
    assert_eq!(result, Err(ArtifactContentError::UnsafeStorageLocation));

    // Nothing was written into the symlink target.
    let mut entries = tokio::fs::read_dir(&outside).await.unwrap();
    assert!(
        entries.next_entry().await.unwrap().is_none(),
        "symlink target must remain untouched"
    );
    let _ = std::fs::remove_file(&attempt_dir_path);
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(outside);
}

#[tokio::test]
async fn a_stream_error_mid_upload_stages_nothing() {
    let root_dir = temp_root("stream-error");
    let root = root_dir.path();
    let storage = ArtifactStorage::new(root);
    let failing = stream::iter(vec![
        Ok::<_, std::io::Error>(Bytes::from_static(b"partial-")),
        Err(std::io::Error::other("connection reset")),
    ]);
    let result = storage
        .store_streaming("attempt-6", "artifact-6", 100, &"0".repeat(64), failing)
        .await;
    assert_eq!(result, Err(ArtifactContentError::StreamRead));
    let attempt_dir = root.join(encode_id("attempt-6"));
    let mut entries = tokio::fs::read_dir(&attempt_dir).await.unwrap();
    assert!(entries.next_entry().await.unwrap().is_none());
    let _ = std::fs::remove_dir_all(root);
}
