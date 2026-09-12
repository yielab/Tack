use super::*;
use object_store::memory::InMemory;

fn make_store() -> Arc<InMemory> {
    Arc::new(InMemory::new())
}

// ── manifest serialization ────────────────────────────────────────────────

#[test]
fn manifest_roundtrips_json() {
    let m = BackupManifest {
        format_version: 1,
        created_at: "2026-06-12T00:00:00+00:00".into(),
        migration_version: 16,
        db_sha256: "abc123".into(),
        install_id: "install-uuid".into(),
        item_count: 42,
        object_key: "tack/tack-backup-test.tar.zst".into(),
        bundle_size_bytes: 1024,
        generation: 0,
    };
    let json = serde_json::to_string(&m).unwrap();
    let m2: BackupManifest = serde_json::from_str(&json).unwrap();
    assert_eq!(m.migration_version, m2.migration_version);
    assert_eq!(m.db_sha256, m2.db_sha256);
    assert_eq!(m.item_count, m2.item_count);
}

// ── build_tar round-trip ──────────────────────────────────────────────────

#[test]
fn build_tar_contains_database_and_manifest() {
    let db_bytes = b"SQLite format 3\x00fake_database_content";
    let tar_bytes = build_tar(
        db_bytes,
        "/nonexistent_storage_dir",
        "2026-06-12T00:00:00+00:00",
        16,
        5,
        "test-install-id",
        "deadbeef",
        0,
    )
    .unwrap();

    // Decompress would fail here (not compressed yet), check raw tar entries.
    let mut archive = tar::Archive::new(Cursor::new(&tar_bytes));
    let paths: Vec<String> = archive
        .entries()
        .unwrap()
        .map(|e| e.unwrap().path().unwrap().to_string_lossy().to_string())
        .collect();

    assert!(
        paths.contains(&"database.db".to_string()),
        "missing database.db in tar"
    );
    assert!(
        paths.contains(&"manifest.json".to_string()),
        "missing manifest.json in tar"
    );
}

// ── upload → list → download ──────────────────────────────────────────────

#[tokio::test]
async fn upload_list_download_roundtrip() {
    let store = make_store();
    let manifest = BackupManifest {
        format_version: 1,
        created_at: "2026-06-12T01:00:00+00:00".into(),
        migration_version: 16,
        db_sha256: "aa".into(),
        install_id: "id".into(),
        item_count: 1,
        object_key: "tack/tack-backup-test.tar.zst".into(),
        bundle_size_bytes: 4,
        generation: 0,
    };
    let bundle_data = b"test".to_vec();

    upload(store.as_ref(), &manifest, bundle_data.clone())
        .await
        .unwrap();

    let listed = list(store.as_ref(), "tack").await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].object_key, manifest.object_key);
    assert_eq!(listed[0].migration_version, 16);

    let downloaded = download(store.as_ref(), &manifest.object_key)
        .await
        .unwrap();
    assert_eq!(downloaded, bundle_data);
}

// ── prune keeps newest N ──────────────────────────────────────────────────

#[tokio::test]
async fn prune_keeps_newest() {
    let store = make_store();

    for i in 0..5u32 {
        let m = BackupManifest {
            format_version: 1,
            created_at: format!("2026-06-{:02}T00:00:00+00:00", i + 1),
            migration_version: 16,
            db_sha256: "x".into(),
            install_id: "id".into(),
            item_count: 0,
            object_key: format!("tack/tack-backup-{i:02}.tar.zst"),
            bundle_size_bytes: 1,
            generation: 0,
        };
        upload(store.as_ref(), &m, b"x".to_vec()).await.unwrap();
    }

    let deleted = prune(store.as_ref(), "tack", 3).await.unwrap();
    assert_eq!(deleted, 2);

    let remaining = list(store.as_ref(), "tack").await.unwrap();
    assert_eq!(remaining.len(), 3);
    // Newest 3 should be days 05, 04, 03 (sorted newest-first)
    assert!(remaining[0].created_at.contains("06-05"));
    assert!(remaining[2].created_at.contains("06-03"));
}

// ── version-guard rejects newer snapshots ─────────────────────────────────

#[tokio::test]
async fn stage_restore_rejects_newer_schema() {
    let manifest = BackupManifest {
        format_version: 1,
        created_at: "2026-06-12T00:00:00+00:00".into(),
        migration_version: 20, // ahead of local
        db_sha256: "x".into(),
        install_id: "id".into(),
        item_count: 0,
        object_key: "key".into(),
        bundle_size_bytes: 0,
        generation: 0,
    };

    let result = stage_restore(
        b"dummy".to_vec(),
        &manifest,
        16, // local has 16 migrations
        std::path::Path::new("/tmp/tack_test.db"),
        "/tmp/tack_storage_test",
    )
    .await;

    assert!(
        matches!(
            result,
            Err(BackupError::SchemaTooNew {
                snapshot: 20,
                local: 16
            })
        ),
        "expected SchemaTooNew error, got: {result:?}"
    );
}

// ── 27.3: tar extraction rejects path traversal ───────────────────────────
#[test]
fn parse_bundle_rejects_path_traversal() {
    let mut ar = tar::Builder::new(Vec::new());

    // A legitimate database entry, processed first.
    let db = b"SQLite format 3\x00";
    let mut h = tar::Header::new_gnu();
    h.set_size(db.len() as u64);
    h.set_mode(0o644);
    h.set_cksum();
    ar.append_data(&mut h, "database.db", Cursor::new(db))
        .unwrap();

    // A malicious entry that tries to escape the staging directory. The tar
    // writer refuses `..` via set_path, so write the raw name into the header
    // directly — exactly what a hand-crafted malicious archive would do.
    let evil = b"pwned";
    let mut h2 = tar::Header::new_gnu();
    h2.set_size(evil.len() as u64);
    h2.set_mode(0o644);
    let evil_name = b"attachments/../../x";
    h2.as_gnu_mut().unwrap().name[..evil_name.len()].copy_from_slice(evil_name);
    h2.set_cksum();
    ar.append(&h2, Cursor::new(evil)).unwrap();

    let tar_bytes = ar.into_inner().unwrap();
    let bundle = zstd::encode_all(Cursor::new(&tar_bytes), 3).unwrap();

    let res = parse_bundle(&bundle, Path::new("/tmp/tack-traversal-test.restore"));
    assert!(
        matches!(res, Err(BackupError::UnsafePath(_))),
        "expected UnsafePath, got: {res:?}"
    );
}

// ── 27.4: restore verifies db_sha256 and format_version ────────────────────
#[tokio::test]
async fn stage_restore_rejects_tampered_db() {
    let db = b"SQLite format 3\x00 tamper test payload";
    let real_sha = hex::encode(Sha256::digest(db));
    let tar_bytes = build_tar(
        db,
        "/nonexistent_storage_dir",
        "2026-06-12T00:00:00+00:00",
        1,
        0,
        "id",
        &real_sha,
        0,
    )
    .unwrap();
    let bundle = zstd::encode_all(Cursor::new(&tar_bytes), 3).unwrap();

    let manifest = BackupManifest {
        format_version: 1,
        created_at: "2026-06-12T00:00:00+00:00".into(),
        migration_version: 1,
        db_sha256: "deadbeefdeadbeef".into(), // does NOT match the real DB
        install_id: "id".into(),
        item_count: 0,
        object_key: "key".into(),
        bundle_size_bytes: 0,
        generation: 0,
    };

    let dir = tempfile::tempdir().expect("temporary directory");
    let db_path = dir.path().join("tamper.db");
    let storage = dir.path().join("storage");
    let res = stage_restore(
        bundle,
        &manifest,
        16,
        &db_path,
        storage.to_string_lossy().as_ref(),
    )
    .await;

    assert!(
        matches!(res, Err(BackupError::IntegrityMismatch)),
        "expected IntegrityMismatch, got: {res:?}"
    );
    // Nothing should have been staged.
    assert!(!PathBuf::from(format!("{}.restore", db_path.to_string_lossy())).exists());
}

#[tokio::test]
async fn stage_restore_rejects_wrong_format_version() {
    let manifest = BackupManifest {
        format_version: 2, // unknown format
        created_at: "2026-06-12T00:00:00+00:00".into(),
        migration_version: 1,
        db_sha256: "x".into(),
        install_id: "id".into(),
        item_count: 0,
        object_key: "key".into(),
        bundle_size_bytes: 0,
        generation: 0,
    };

    let res = stage_restore(
        b"dummy".to_vec(),
        &manifest,
        16,
        Path::new("/tmp/tack_fmt_test.db"),
        "/tmp/tack_fmt_storage",
    )
    .await;

    assert!(
        matches!(res, Err(BackupError::UnsupportedFormat(2))),
        "expected UnsupportedFormat(2), got: {res:?}"
    );
}

// ── 27.6: snapshots ship no secrets or install identity ────────────────────
#[tokio::test]
async fn scrub_removes_secrets_from_snapshot() {
    use sqlx::ConnectOptions;
    use sqlx::Connection;
    use sqlx::sqlite::SqliteConnectOptions;

    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("scrub.db");
    let secret = "SUPER-SECRET-S3-KEY-9f8e7d6c5b4a";

    {
        let mut conn = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .connect()
            .await
            .unwrap();
        sqlx::query("CREATE TABLE app_meta (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL)")
            .execute(&mut conn)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO app_meta (key, value) VALUES ('install_id', 'source-install-uuid')",
        )
        .execute(&mut conn)
        .await
        .unwrap();
        sqlx::query("INSERT INTO app_meta (key, value) VALUES ('backup_config', ?)")
            .bind(format!(r#"{{"secret_key":"{secret}"}}"#))
            .execute(&mut conn)
            .await
            .unwrap();
        conn.close().await.unwrap();
    }

    scrub_snapshot_secrets(&path).await.unwrap();

    // The sensitive rows are gone.
    let mut conn = SqliteConnectOptions::new()
        .filename(&path)
        .connect()
        .await
        .unwrap();
    let remaining: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM app_meta WHERE key IN ('install_id', 'backup_config')",
    )
    .fetch_one(&mut conn)
    .await
    .unwrap();
    conn.close().await.unwrap();
    assert_eq!(remaining, 0, "sensitive app_meta rows survived scrub");

    // And the secret bytes are physically absent from the file (post-VACUUM).
    let bytes = std::fs::read(&path).unwrap();
    assert!(
        !bytes.windows(secret.len()).any(|w| w == secret.as_bytes()),
        "secret string still present in snapshot bytes"
    );

    std::fs::remove_file(&path).ok();
}

// ── control_planes.token must be scrubbed too — the same class of leak
// as the S3 secret key, but for migration 019's new table.
// Goes through the *real* backup path — create_bundle end-to-end (snapshot,
// scrub, tar, zstd) — then extracts database.db back out exactly as a
// restore would, and checks the raw extracted bytes. Mirrors
// `scrub_removes_secrets_from_snapshot` above, which is the existing
// raw-bytes regression test for the S3 secret key (`app_meta.backup_config`).
#[tokio::test]
async fn scrub_removes_control_plane_token_from_snapshot() {
    use sqlx::ConnectOptions;
    use sqlx::Connection;
    use sqlx::sqlite::SqliteConnectOptions;

    let (pool, cfg, dir) = file_backed().await;
    let repo = tack_db::repo::Repository::new(pool.clone());

    let secret_token = "DOCKET-BEARER-TOKEN-9f8e7d6c5b4a3f2e1d0c";
    let plane = repo
        .create_control_plane(tack_db::repo::orch::CreateControlPlane {
            name: "prod-docket".into(),
            kind: None,
            base_url: "https://docket.example.com".into(),
            token: Some(secret_token.to_string()),
        })
        .await
        .unwrap();

    let (bundle, _manifest) = create_bundle(&pool, &cfg).await.unwrap();

    // Decompress + extract database.db exactly as a restore would.
    let (db_bytes, _attachments) = tokio::task::spawn_blocking(move || {
        parse_bundle(&bundle, Path::new("/nonexistent-cp-scrub-test"))
    })
    .await
    .unwrap()
    .unwrap();

    assert!(
        !db_bytes
            .windows(secret_token.len())
            .any(|w| w == secret_token.as_bytes()),
        "control plane token still present in snapshot bytes"
    );

    // The row itself must still exist (scrubbing nulls the token, it must
    // not delete the row — a restored backup should still know which
    // planes were registered, just forget their credentials).
    let extracted_path = dir.path().join("extracted-check.db");
    tokio::fs::write(&extracted_path, &db_bytes).await.unwrap();
    let mut conn = SqliteConnectOptions::new()
        .filename(&extracted_path)
        .connect()
        .await
        .unwrap();
    let (row_count, token_null_count): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), SUM(CASE WHEN token IS NULL THEN 1 ELSE 0 END) \
         FROM control_planes WHERE id = ?",
    )
    .bind(plane.id.to_string())
    .fetch_one(&mut conn)
    .await
    .unwrap();
    conn.close().await.unwrap();

    assert_eq!(row_count, 1, "control_planes row must survive the scrub");
    assert_eq!(
        token_null_count, 1,
        "control_planes.token must be nulled by the scrub"
    );

    pool.close().await;
}

// ── control_planes.secrets (migration 033) must be scrubbed too —
// the write-only provider-credentials blob a GitHub Actions plane packs its
// API credential and webhook signing secret into. Same shape as the token
// test above, and the same reason it matters: a test that only checked
// `secrets IS NULL` would still pass against an implementation that forgot
// the VACUUM, because a plain UPDATE leaves the old bytes sitting in freed
// pages. This asserts the secret string is physically absent from the raw
// snapshot bytes, which only a real VACUUM (not just the UPDATE) achieves.
#[tokio::test]
async fn scrub_removes_control_plane_secrets_from_snapshot() {
    use sqlx::ConnectOptions;
    use sqlx::Connection;
    use sqlx::sqlite::SqliteConnectOptions;

    let (pool, cfg, dir) = file_backed().await;
    let repo = tack_db::repo::Repository::new(pool.clone());

    // `CreateControlPlane` has no `secrets` field yet (that lands with the
    // registry, a different card) — seed the column directly, exactly as an
    // adapter's write path will once it exists.
    let secret_blob = r#"{"api_token":"GHA-PAT-9f8e7d6c5b4a3f2e1d0c","webhook_secret":"WHSEC-1a2b3c4d5e6f7a8b9c0d"}"#;
    let plane = repo
        .create_control_plane(tack_db::repo::orch::CreateControlPlane {
            name: "gha-prod".into(),
            kind: Some("github_actions".into()),
            base_url: "https://api.github.com".into(),
            token: None,
        })
        .await
        .unwrap();
    sqlx::query("UPDATE control_planes SET secrets = ? WHERE id = ?")
        .bind(secret_blob)
        .bind(plane.id.to_string())
        .execute(&pool)
        .await
        .unwrap();

    let (bundle, _manifest) = create_bundle(&pool, &cfg).await.unwrap();

    // Decompress + extract database.db exactly as a restore would.
    let (db_bytes, _attachments) = tokio::task::spawn_blocking(move || {
        parse_bundle(&bundle, Path::new("/nonexistent-cp-secrets-scrub-test"))
    })
    .await
    .unwrap()
    .unwrap();

    // Neither secret substring — nor the whole blob — survives in the raw
    // snapshot bytes. Checking both halves guards against an implementation
    // that scrubs one key of a two-secret JSON blob and not the other.
    for needle in ["GHA-PAT-9f8e7d6c5b4a3f2e1d0c", "WHSEC-1a2b3c4d5e6f7a8b9c0d"] {
        assert!(
            !db_bytes
                .windows(needle.len())
                .any(|w| w == needle.as_bytes()),
            "control plane secret material ({needle}) still present in snapshot bytes"
        );
    }

    // The row itself must still exist (scrubbing nulls secrets, it must not
    // delete the row — a restored backup should still show which planes
    // were registered, just forget their credentials).
    let extracted_path = dir.path().join("extracted-secrets-check.db");
    tokio::fs::write(&extracted_path, &db_bytes).await.unwrap();
    let mut conn = SqliteConnectOptions::new()
        .filename(&extracted_path)
        .connect()
        .await
        .unwrap();
    let (row_count, secrets_null_count): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), SUM(CASE WHEN secrets IS NULL THEN 1 ELSE 0 END) \
         FROM control_planes WHERE id = ?",
    )
    .bind(plane.id.to_string())
    .fetch_one(&mut conn)
    .await
    .unwrap();
    conn.close().await.unwrap();

    assert_eq!(row_count, 1, "control_planes row must survive the scrub");
    assert_eq!(
        secrets_null_count, 1,
        "control_planes.secrets must be nulled by the scrub"
    );

    pool.close().await;
}

// ── Scrubbing must not fail against a snapshot whose control_planes
// table predates migration 033 (has the table, not yet the `secrets`
// column). Without the pragma_table_info guard, the UPDATE below would be a
// hard sqlx error ("no such column: secrets") that aborts the whole
// function before the VACUUM runs — meaning a database that has not yet
// been migrated to 033 could never produce a scrubbed backup at all.
#[tokio::test]
async fn scrub_tolerates_a_control_planes_table_without_the_secrets_column() {
    use sqlx::ConnectOptions;
    use sqlx::Connection;
    use sqlx::sqlite::SqliteConnectOptions;

    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("scrub-pre033.db");

    {
        let mut conn = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .connect()
            .await
            .unwrap();
        // Migration 019's shape, deliberately without 033's `secrets` column.
        sqlx::query(
            "CREATE TABLE control_planes (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL DEFAULT 'docket',
                base_url TEXT NOT NULL,
                token TEXT,
                health TEXT NOT NULL DEFAULT 'unknown',
                consecutive_failures INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
        )
        .execute(&mut conn)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO control_planes
                (id, name, kind, base_url, token, health, consecutive_failures, created_at, updated_at)
             VALUES ('p1', 'legacy', 'docket', 'https://example.com', 'still-here', 'unknown', 0, '2026-01-01', '2026-01-01')",
        )
        .execute(&mut conn)
        .await
        .unwrap();
        conn.close().await.unwrap();
    }

    // Must not error, and must still reach the token scrub and the VACUUM.
    scrub_snapshot_secrets(&path).await.unwrap();

    let mut conn = SqliteConnectOptions::new()
        .filename(&path)
        .connect()
        .await
        .unwrap();
    let token: Option<String> =
        sqlx::query_scalar("SELECT token FROM control_planes WHERE id = 'p1'")
            .fetch_one(&mut conn)
            .await
            .unwrap();
    conn.close().await.unwrap();
    assert_eq!(
        token, None,
        "the token block must still run even when the secrets column is absent"
    );

    std::fs::remove_file(&path).ok();
}

// ── prune is a no-op when below retention ─────────────────────────────────

#[tokio::test]
async fn prune_noop_when_under_retention() {
    let store = make_store();
    let m = BackupManifest {
        format_version: 1,
        created_at: "2026-06-12T00:00:00+00:00".into(),
        migration_version: 16,
        db_sha256: "x".into(),
        install_id: "id".into(),
        item_count: 0,
        object_key: "tack/tack-backup-only.tar.zst".into(),
        bundle_size_bytes: 1,
        generation: 0,
    };
    upload(store.as_ref(), &m, b"x".to_vec()).await.unwrap();
    let deleted = prune(store.as_ref(), "tack", 10).await.unwrap();
    assert_eq!(deleted, 0);
}

// ── 28.2: generation counter + conflict detection ─────────────────────────

fn manifest_at(gen_val: u64, install: &str, key: &str) -> BackupManifest {
    BackupManifest {
        format_version: 1,
        created_at: "2026-06-12T00:00:00+00:00".into(),
        migration_version: 1,
        db_sha256: "x".into(),
        install_id: install.into(),
        item_count: 0,
        object_key: key.into(),
        bundle_size_bytes: 1,
        generation: gen_val,
    }
}

#[test]
fn restore_conflicts_guards_newer_local_work() {
    // Local ahead of the snapshot → conflict unless forced.
    assert!(restore_conflicts(5, 3, false));
    assert!(!restore_conflicts(5, 3, true)); // force overrides
    // Local at/behind the snapshot → never a conflict.
    assert!(!restore_conflicts(3, 3, false));
    assert!(!restore_conflicts(2, 3, false));
}

#[tokio::test]
async fn upload_conflict_only_for_other_device_that_is_ahead() {
    let store = make_store();
    // Remote head from another device at generation 5.
    upload(
        store.as_ref(),
        &manifest_at(5, "device-a", "tack/a.tar.zst"),
        b"x".to_vec(),
    )
    .await
    .unwrap();

    // We are "device-b" about to write generation 5 → conflict (other device ≥ us).
    let c = upload_conflict(store.as_ref(), "tack", 5, "device-b")
        .await
        .unwrap();
    assert!(
        c.is_some(),
        "expected a conflict when another device is ≥ our generation"
    );

    // If we are strictly ahead (prospective 6 > remote 5) → no conflict.
    let c = upload_conflict(store.as_ref(), "tack", 6, "device-b")
        .await
        .unwrap();
    assert!(
        c.is_none(),
        "no conflict when we are ahead of the remote head"
    );

    // Same install id → our own older backup, never a conflict.
    let c = upload_conflict(store.as_ref(), "tack", 5, "device-a")
        .await
        .unwrap();
    assert!(c.is_none(), "our own backups never conflict with us");
}

// Build a file-backed pool + config for full backup-path tests.
async fn file_backed() -> (SqlitePool, AppConfig, tempfile::TempDir) {
    let temp = tempfile::tempdir().expect("temporary directory");
    let dir = temp.path();
    let db_path = dir.join("tack.db");
    let db_url = format!("sqlite:{}?mode=rwc", db_path.to_string_lossy());
    let pool = tack_db::init_pool(&db_url).await.unwrap();
    tack_db::migrations::run_all(&pool).await.unwrap();
    let cfg = AppConfig {
        database_url: db_url,
        storage_dir: dir.join("storage").to_string_lossy().into_owned(),
        backup_prefix: "tack".into(),
        backup_retention: 10,
        ..AppConfig::default()
    };
    (pool, cfg, temp)
}

#[tokio::test]
async fn perform_backup_bumps_generation_and_enforces_conflict() {
    let store = make_store();
    let (pool, cfg, _dir) = file_backed().await;

    // First backup: generation 0 → 1.
    let m1 = perform_backup(&pool, &cfg, store.as_ref(), false)
        .await
        .unwrap();
    assert_eq!(m1.generation, 1);
    assert_eq!(generation(&pool).await.unwrap(), 1);

    // Second backup from the same device: 1 → 2, no conflict.
    let m2 = perform_backup(&pool, &cfg, store.as_ref(), false)
        .await
        .unwrap();
    assert_eq!(m2.generation, 2);

    // Simulate another device uploading newer work at generation 5.
    upload(
        store.as_ref(),
        &manifest_at(5, "other-device", "tack/other.tar.zst"),
        b"x".to_vec(),
    )
    .await
    .unwrap();

    // Non-forced backup must be rejected and must NOT advance our generation.
    let err = perform_backup(&pool, &cfg, store.as_ref(), false)
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            BackupError::GenerationConflict {
                remote_generation: 5,
                ..
            }
        ),
        "expected GenerationConflict, got {err:?}"
    );
    assert_eq!(
        generation(&pool).await.unwrap(),
        2,
        "conflict must not bump generation"
    );

    // Forcing overrides the conflict and proceeds.
    let forced = perform_backup(&pool, &cfg, store.as_ref(), true)
        .await
        .unwrap();
    assert_eq!(forced.generation, 3);
    assert_eq!(generation(&pool).await.unwrap(), 3);

    pool.close().await;
}

// ── 28.5: verify_bundle validates without staging ─────────────────────────

#[tokio::test]
async fn verify_bundle_accepts_valid_and_rejects_tampered() {
    let db = b"SQLite format 3\x00 verify payload";
    let real_sha = hex::encode(Sha256::digest(db));
    let tar_bytes = build_tar(
        db,
        "/nonexistent",
        "2026-06-12T00:00:00+00:00",
        1,
        0,
        "id",
        &real_sha,
        7,
    )
    .unwrap();
    let bundle = zstd::encode_all(Cursor::new(&tar_bytes), 3).unwrap();

    let mut manifest = manifest_at(7, "id", "key");
    manifest.db_sha256 = real_sha.clone();
    manifest.migration_version = 1;

    // Valid bundle verifies OK against a running binary with ≥1 migrations.
    verify_bundle(bundle.clone(), &manifest, 16).await.unwrap();

    // Tampered sha → IntegrityMismatch.
    let mut bad = manifest.clone();
    bad.db_sha256 = "deadbeef".into();
    assert!(matches!(
        verify_bundle(bundle.clone(), &bad, 16).await,
        Err(BackupError::IntegrityMismatch)
    ));

    // Newer schema than local → SchemaTooNew.
    let mut newer = manifest.clone();
    newer.migration_version = 99;
    assert!(matches!(
        verify_bundle(bundle, &newer, 16).await,
        Err(BackupError::SchemaTooNew { .. })
    ));
}

// ── 28.4: prune reconciles an orphaned bundle (no sidecar) ─────────────────

#[tokio::test]
async fn prune_reconciles_orphaned_bundle() {
    let store = make_store();

    // A healthy backup (bundle + sidecar).
    upload(
        store.as_ref(),
        &manifest_at(1, "id", "tack/good.tar.zst"),
        b"x".to_vec(),
    )
    .await
    .unwrap();

    // An orphan: a bundle object with NO sidecar (a failed sidecar PUT).
    store
        .put(
            &OsPath::from("tack/orphan.tar.zst"),
            object_store::PutPayload::from(b"junk".to_vec()),
        )
        .await
        .unwrap();

    let deleted = prune(store.as_ref(), "tack", 10).await.unwrap();
    assert_eq!(deleted, 1, "the orphan bundle should be reconciled/deleted");

    // The orphan is gone; the healthy backup remains.
    assert!(
        orphan_bundles(store.as_ref(), "tack")
            .await
            .unwrap()
            .is_empty()
    );
    let remaining = list(store.as_ref(), "tack").await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].object_key, "tack/good.tar.zst");
}

// ── 28.4: multipart path uploads and round-trips a large bundle ────────────

#[tokio::test]
async fn upload_multipart_roundtrips_large_bundle() {
    let store = make_store();
    // Above MULTIPART_THRESHOLD so the multipart branch is exercised.
    let big = vec![0xABu8; MULTIPART_THRESHOLD + 1024];
    let manifest = manifest_at(1, "id", "tack/big.tar.zst");

    upload(store.as_ref(), &manifest, big.clone())
        .await
        .unwrap();

    let got = download(store.as_ref(), "tack/big.tar.zst").await.unwrap();
    assert_eq!(
        got, big,
        "multipart-uploaded bundle must round-trip byte-for-byte"
    );
}
