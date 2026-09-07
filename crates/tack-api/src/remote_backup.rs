//! Remote cloud backup — S3-compatible object storage (Cloudflare R2, Backblaze B2, AWS S3,
//! MinIO, or any other S3-compatible endpoint).
//!
//! # Bundle format
//! Each backup is a `tack-backup-<UTC-timestamp>.tar.zst` archive containing:
//! - `database.db`      — VACUUM INTO SQLite snapshot
//! - `attachments/…`    — copy of TACK_STORAGE_DIR (omitted when empty / dir absent)
//! - `manifest.json`    — metadata (format_version, timestamps, migration count, sha256, etc.)
//!
//! A sidecar `<archive>.manifest.json` is stored alongside each bundle so `list()` can
//! return metadata without downloading the full archive.

use std::io::{Cursor, Read as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use object_store::ObjectStore;
// object_store 0.13 moved the put/get/delete convenience methods to this extension trait.
use object_store::ObjectStoreExt;
use object_store::aws::AmazonS3Builder;
use object_store::path::Path as OsPath;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use thiserror::Error;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::config::AppConfig;

// ── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("remote backup is not configured (set TACK_BACKUP_BUCKET, _ACCESS_KEY, _SECRET_KEY)")]
    NotConfigured,
    #[error("backup requires a file-based database (not in-memory)")]
    InMemoryDb,
    #[error("object store error: {0}")]
    ObjectStore(#[from] object_store::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("zstd error: {0}")]
    Zstd(String),
    #[error(
        "restore rejected: snapshot migration_version ({snapshot}) is ahead of the running binary ({local}); upgrade Tack before restoring"
    )]
    SchemaTooNew { snapshot: u32, local: u32 },
    #[error("bundle is corrupt or unrecognised format")]
    CorruptBundle,
    #[error(
        "restore rejected: unsupported bundle format_version {0} (this binary understands version 1)"
    )]
    UnsupportedFormat(u32),
    #[error(
        "restore rejected: database integrity check failed (manifest sha256 does not match the extracted database)"
    )]
    IntegrityMismatch,
    #[error("restore rejected: bundle contains an unsafe path '{0}' (path traversal attempt)")]
    UnsafePath(String),
    #[error(
        "upload rejected: another device uploaded newer work (remote generation {remote_generation} ≥ local {local_generation}) — restore first or force"
    )]
    GenerationConflict {
        local_generation: u64,
        remote_generation: u64,
        /// The remote head manifest that would be clobbered (for the 409 body).
        remote: Box<BackupManifest>,
    },
    #[error(
        "restore rejected: local generation ({local_generation}) is ahead of the snapshot ({snapshot_generation}); this device has newer work — force to overwrite"
    )]
    RestoreWouldLoseWork {
        local_generation: u64,
        snapshot_generation: u64,
    },
}

// ── Manifest ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    /// Always 1 for this implementation.
    pub format_version: u32,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
    /// Number of rows in the `_migrations` table at backup time.
    pub migration_version: u32,
    /// Hex-encoded SHA-256 of the raw `database.db` bytes.
    pub db_sha256: String,
    /// Stable install identifier (UUID v4, generated once per install).
    pub install_id: String,
    /// Total number of items in the database at backup time.
    pub item_count: u64,
    /// Object key of the `.tar.zst` archive in the bucket.
    pub object_key: String,
    /// Approximate bundle size in bytes.
    pub bundle_size_bytes: u64,
    /// Monotonic sync generation this snapshot represents. Bumped once per
    /// successful backup; used for cross-device conflict detection.
    /// Defaults to 0 for older sidecars that predate the field.
    #[serde(default)]
    pub generation: u64,
}

// ── Store construction ────────────────────────────────────────────────────────

/// Build an S3-compatible `ObjectStore` from application config.
pub fn store_from_config(cfg: &AppConfig) -> Result<Arc<dyn ObjectStore>, BackupError> {
    if !cfg.remote_backup_enabled() {
        return Err(BackupError::NotConfigured);
    }

    let bucket = cfg.backup_bucket.as_deref().unwrap();
    let access_key = cfg.backup_access_key.as_deref().unwrap();
    let secret_key = cfg.backup_secret_key.as_deref().unwrap();

    let mut builder = AmazonS3Builder::new()
        .with_bucket_name(bucket)
        .with_region(&cfg.backup_region)
        .with_access_key_id(access_key)
        .with_secret_access_key(secret_key);

    if let Some(endpoint) = &cfg.backup_endpoint {
        builder = builder.with_endpoint(endpoint);
        // Allow plain HTTP for local MinIO / test setups
        if endpoint.starts_with("http://") {
            builder = builder.with_allow_http(true);
        }
    }

    let store = builder.build()?;
    Ok(Arc::new(store))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Query the current migration version (count of applied rows in `_migrations`).
async fn migration_version(pool: &SqlitePool) -> Result<u32, BackupError> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _migrations")
        .fetch_one(pool)
        .await?;
    Ok(count as u32)
}

/// Query the total item count.
async fn item_count(pool: &SqlitePool) -> Result<u64, BackupError> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM items")
        .fetch_one(pool)
        .await?;
    Ok(count as u64)
}

/// Get or create a stable install ID stored in the `app_meta` table.
pub async fn install_id(pool: &SqlitePool) -> Result<String, BackupError> {
    // Create the table if it doesn't exist (idempotent).
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS app_meta (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL)",
    )
    .execute(pool)
    .await?;

    let existing: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_meta WHERE key = 'install_id'")
            .fetch_optional(pool)
            .await?;

    if let Some(id) = existing {
        return Ok(id);
    }

    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO app_meta (key, value) VALUES ('install_id', ?)")
        .bind(&id)
        .execute(pool)
        .await?;
    Ok(id)
}

/// Read the monotonic sync generation counter (`app_meta.generation`).
///
/// Defaults to 0 when the row is absent — a brand-new install. This value is
/// carried inside the DB snapshot (it is *not* scrubbed), so a device that
/// restores another device's bundle adopts that bundle's generation.
pub async fn generation(pool: &SqlitePool) -> Result<u64, BackupError> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS app_meta (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL)",
    )
    .execute(pool)
    .await?;

    let existing: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_meta WHERE key = 'generation'")
            .fetch_optional(pool)
            .await?;

    Ok(existing.and_then(|s| s.parse::<u64>().ok()).unwrap_or(0))
}

/// Persist the sync generation counter.
pub async fn set_generation(pool: &SqlitePool, value: u64) -> Result<(), BackupError> {
    sqlx::query(
        "INSERT INTO app_meta (key, value) VALUES ('generation', ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(value.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

/// The remote head: the manifest with the highest generation (ties broken by
/// newest `created_at`). `None` when the bucket has no backups yet.
pub async fn remote_head(
    store: &dyn ObjectStore,
    prefix: &str,
) -> Result<Option<BackupManifest>, BackupError> {
    let manifests = list(store, prefix).await?;
    Ok(manifests.into_iter().max_by(|a, b| {
        a.generation
            .cmp(&b.generation)
            .then(a.created_at.cmp(&b.created_at))
    }))
}

/// Conflict guard for uploads. Returns `Some(remote_head)` when uploading at
/// `prospective_generation` would clobber newer remote work from *another*
/// install — i.e. the remote head's generation is `>= prospective_generation`
/// and it belongs to a different install. `None` means it is safe to upload.
pub async fn upload_conflict(
    store: &dyn ObjectStore,
    prefix: &str,
    prospective_generation: u64,
    local_install: &str,
) -> Result<Option<BackupManifest>, BackupError> {
    let head = remote_head(store, prefix).await?;
    Ok(head.filter(|h| h.generation >= prospective_generation && h.install_id != local_install))
}

/// Whether restoring a snapshot at `snapshot_generation` onto a device at
/// `local_generation` would discard newer local work. `force` overrides it.
/// (28.2, restore direction.)
pub fn restore_conflicts(local_generation: u64, snapshot_generation: u64, force: bool) -> bool {
    !force && local_generation > snapshot_generation
}

/// `app_meta` keys that must never leave the machine inside a backup: the S3
/// secret key is stored (JSON-encoded) under `backup_config`, and `install_id`
/// is this install's identity — restoring it elsewhere would clone the identity.
///
/// This list only covers `app_meta`. **This is not the only thing
/// [`scrub_snapshot_secrets`] scrubs** — any other table that grows a
/// secret-bearing column (like `control_planes.token`, migration 019, or
/// `control_planes.secrets`, migration 033 — a GitHub Actions plane's API
/// credential and webhook signing secret, packed into one JSON blob) needs its
/// own dedicated block in that function, following the same
/// null-before-VACUUM shape. Read that function's doc comment before adding a
/// new secret column anywhere in the schema.
const SENSITIVE_META_KEYS: &[&str] = &["backup_config", "install_id"];

/// Strip machine-local secrets/identity from a freshly-created snapshot DB file
/// so they never ship inside a downloadable or uploadable bundle.
///
/// This is the single chokepoint for scrubbing secrets out of a backup
/// snapshot — every table with a secret-bearing column must be handled here.
/// Currently that's:
/// - `app_meta`: the keys in [`SENSITIVE_META_KEYS`] (S3 backup secret key,
///   install identity) are deleted outright.
/// - `control_planes.token` (migration 019, the docket Bearer credential): set
///   to `NULL` rather than deleting the row, so a restored backup still knows
///   which control planes were registered — the operator just re-enters the
///   token afterwards. See `crates/tack-db/src/repo/orch.rs` for why the token
///   never leaves the DB layer in a read DTO either.
/// - `control_planes.secrets` (migration 033, write-only provider-credentials
///   JSON — for a GitHub Actions plane, an API credential *and* a webhook
///   signing secret in one blob): same treatment as `token` and for the same
///   reason — `NULL`, not row deletion, so a restore still shows which planes
///   were registered and the operator re-enters both secrets.
///
/// Because `install_id` is removed, a restored database has no identity row and
/// [`install_id`] regenerates a fresh UUID on first use — restores no longer
/// adopt the source install's identity.
///
/// **Add new secret columns here, not just to this doc comment** — this
/// function is the chokepoint, and it must run before the trailing `VACUUM`
/// (below) so the freed bytes are actually dropped from the file, not just
/// unreferenced in the freelist.
pub async fn scrub_snapshot_secrets(db_file: &Path) -> Result<(), BackupError> {
    use sqlx::ConnectOptions;
    use sqlx::sqlite::SqliteConnectOptions;

    let mut conn = SqliteConnectOptions::new()
        .filename(db_file)
        .create_if_missing(false)
        .connect()
        .await?;

    // The table may be absent on a brand-new DB — create defensively so the
    // DELETE below is always valid.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS app_meta (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL)",
    )
    .execute(&mut conn)
    .await?;

    for key in SENSITIVE_META_KEYS {
        sqlx::query("DELETE FROM app_meta WHERE key = ?")
            .bind(key)
            .execute(&mut conn)
            .await?;
    }

    // control_planes (migration 019) may not exist in a snapshot taken from a
    // pre-019 database — guard with sqlite_master rather than assuming the
    // table is there, same defensive posture as the app_meta CREATE above.
    // Null the token only; the row itself (name, base_url, health, …) must
    // survive so a restore still shows which planes were registered.
    let has_control_planes: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'control_planes'",
    )
    .fetch_optional(&mut conn)
    .await?;
    if has_control_planes.is_some() {
        sqlx::query("UPDATE control_planes SET token = NULL WHERE token IS NOT NULL")
            .execute(&mut conn)
            .await?;
    }

    // control_planes.secrets (migration 033) is newer than the table itself
    // (migration 019), so `has_control_planes` alone is not a sufficient guard:
    // a snapshot taken from a pre-033 database has the table but not yet this
    // column, and an UPDATE naming an absent column is a hard sqlx error, not a
    // no-op — it would abort the whole scrub function before the VACUUM below
    // ever runs, which would leave the app_meta secrets deleted above but the
    // freed pages never rewritten. Check the column's presence via
    // pragma_table_info before touching it, same defensive posture as guarding
    // the table's presence via sqlite_master above. Null, not delete, for the
    // same reason as token: the row must survive so a restore still shows which
    // planes were registered — the operator re-enters both secrets afterwards.
    if has_control_planes.is_some() {
        let has_secrets_column: Option<String> = sqlx::query_scalar(
            "SELECT name FROM pragma_table_info('control_planes') WHERE name = 'secrets'",
        )
        .fetch_optional(&mut conn)
        .await?;
        if has_secrets_column.is_some() {
            sqlx::query("UPDATE control_planes SET secrets = NULL WHERE secrets IS NOT NULL")
                .execute(&mut conn)
                .await?;
        }
    }

    // A plain DELETE/UPDATE leaves the secret bytes in freed/overwritten pages
    // (the SQLite freelist), so a hex-dump of the snapshot would still reveal
    // them. VACUUM rewrites the file and physically drops that content — it
    // must run after every scrub step above, not before.
    sqlx::query("VACUUM").execute(&mut conn).await?;

    use sqlx::Connection;
    conn.close().await?;
    Ok(())
}

/// Create a VACUUM INTO snapshot and return the raw bytes of the DB file.
async fn snapshot_db(pool: &SqlitePool, db_path: &Path) -> Result<Vec<u8>, BackupError> {
    // Checkpoint WAL into the main file first.
    sqlx::query("PRAGMA wal_checkpoint(FULL)")
        .execute(pool)
        .await?;

    let temp = std::env::temp_dir().join(format!("tack-snap-{}.db", Uuid::new_v4()));
    let temp_str = temp.to_string_lossy().replace('\'', "''");

    sqlx::query(sqlx::AssertSqlSafe(format!("VACUUM INTO '{temp_str}'")))
        .execute(pool)
        .await?;

    // From here the snapshot file exists and still holds every secret the live
    // database does, so no path may return without removing it: a `?` between
    // here and the removal would leave an unscrubbed copy of the whole
    // database in a shared temporary directory.
    let scrubbed: Result<Vec<u8>, BackupError> = async {
        scrub_snapshot_secrets(&temp).await?;
        Ok(tokio::fs::read(&temp).await?)
    }
    .await;
    let _ = tokio::fs::remove_file(&temp).await;

    let bytes = scrubbed?;
    debug!(db = %db_path.display(), bytes = bytes.len(), "DB snapshot complete");
    Ok(bytes)
}

// ── Bundle creation ───────────────────────────────────────────────────────────

/// Build the full `.tar.zst` bundle in memory and return it together with its manifest.
pub async fn create_bundle(
    pool: &SqlitePool,
    cfg: &AppConfig,
) -> Result<(Vec<u8>, BackupManifest), BackupError> {
    let db_path = cfg.db_file_path().ok_or(BackupError::InMemoryDb)?;

    let db_bytes = snapshot_db(pool, &db_path).await?;
    let db_sha256 = hex::encode(Sha256::digest(&db_bytes));

    let mig_version = migration_version(pool).await?;
    let items = item_count(pool).await?;
    let install = install_id(pool).await?;
    // The generation is expected to already be bumped/persisted by the caller
    // (so the DB snapshot above carries the same value the manifest records).
    let gen_val = generation(pool).await?;
    let created_at = Utc::now().to_rfc3339();

    // Build tar in memory, then compress.
    let tar_bytes = build_tar(
        &db_bytes,
        &cfg.storage_dir,
        &created_at,
        mig_version,
        items,
        &install,
        &db_sha256,
        gen_val,
    )?;

    // Bundles are not encrypted. Optional symmetric encryption would wrap
    // `tar_bytes` here, before or after zstd, from an in-memory or env
    // passphrase; it is left out to keep the binary-size budget and avoid a
    // crypto dependency. Until it exists, the bucket's own access control and
    // server-side encryption are the only protection a snapshot has.

    let compressed = zstd::encode_all(Cursor::new(&tar_bytes), 3)
        .map_err(|e| BackupError::Zstd(e.to_string()))?;

    let ts = created_at.replace(':', "-").replace('+', "Z");
    let object_key = format!("{}/tack-backup-{}.tar.zst", cfg.backup_prefix, ts);

    let manifest = BackupManifest {
        format_version: 1,
        created_at,
        migration_version: mig_version,
        db_sha256,
        install_id: install,
        item_count: items,
        object_key,
        bundle_size_bytes: compressed.len() as u64,
        generation: gen_val,
    };

    Ok((compressed, manifest))
}

#[allow(clippy::too_many_arguments)]
fn build_tar(
    db_bytes: &[u8],
    storage_dir: &str,
    created_at: &str,
    mig_version: u32,
    items: u64,
    install: &str,
    db_sha256: &str,
    generation: u64,
) -> Result<Vec<u8>, BackupError> {
    let mut ar = tar::Builder::new(Vec::new());

    // database.db
    let mut header = tar::Header::new_gnu();
    header.set_size(db_bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    ar.append_data(&mut header, "database.db", Cursor::new(db_bytes))?;

    // attachments/ — walk storage_dir recursively if it exists
    let storage = Path::new(storage_dir);
    if storage.is_dir() {
        append_dir_recursive(&mut ar, storage, storage)?;
    }

    // manifest.json
    let manifest_json = serde_json::to_vec(&serde_json::json!({
        "format_version": 1,
        "created_at": created_at,
        "migration_version": mig_version,
        "db_sha256": db_sha256,
        "install_id": install,
        "item_count": items,
        "generation": generation,
    }))?;
    let mut mh = tar::Header::new_gnu();
    mh.set_size(manifest_json.len() as u64);
    mh.set_mode(0o644);
    mh.set_cksum();
    ar.append_data(&mut mh, "manifest.json", Cursor::new(&manifest_json))?;

    ar.finish()?;
    Ok(ar.into_inner()?)
}

fn append_dir_recursive(
    ar: &mut tar::Builder<Vec<u8>>,
    base: &Path,
    dir: &Path,
) -> Result<(), BackupError> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let rel = path.strip_prefix(base).unwrap_or(&path);
        let tar_path = PathBuf::from("attachments").join(rel);

        if path.is_dir() {
            append_dir_recursive(ar, base, &path)?;
        } else {
            let data = std::fs::read(&path)?;
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            ar.append_data(&mut header, &tar_path, Cursor::new(data))?;
        }
    }
    Ok(())
}

// ── Upload ────────────────────────────────────────────────────────────────────

/// Bundles larger than this are streamed with `put_multipart` instead of a
/// single buffered PUT, so the whole compressed archive need not be re-buffered
/// as one request body inside the object-store client.
const MULTIPART_THRESHOLD: usize = 32 * 1024 * 1024;
/// Multipart chunk size (8 MiB — comfortably above S3's 5 MiB minimum part).
const MULTIPART_PART_SIZE: usize = 8 * 1024 * 1024;

/// Upload a bundle + sidecar manifest to the object store.
///
/// The bundle goes up first; a missing sidecar afterwards leaves an *orphan*
/// bundle, which [`prune`] reconciles (deletes) on its next run so a failed
/// sidecar PUT can never leak an invisible, unprunable object.
pub async fn upload(
    store: &dyn ObjectStore,
    manifest: &BackupManifest,
    bundle: Vec<u8>,
) -> Result<(), BackupError> {
    let bundle_key = OsPath::from(manifest.object_key.clone());
    let sidecar_key = OsPath::from(format!("{}.manifest.json", manifest.object_key));

    if bundle.len() > MULTIPART_THRESHOLD {
        let mut upload = store.put_multipart(&bundle_key).await?;
        for chunk in bundle.chunks(MULTIPART_PART_SIZE) {
            let payload = object_store::PutPayload::from(chunk.to_vec());
            upload.put_part(payload).await?;
        }
        upload.complete().await?;
        info!(key = %manifest.object_key, bytes = bundle.len(), "Uploaded backup bundle (multipart)");
    } else {
        let bundle_payload = object_store::PutPayload::from(bundle);
        store.put(&bundle_key, bundle_payload).await?;
        info!(key = %manifest.object_key, bytes = manifest.bundle_size_bytes, "Uploaded backup bundle");
    }

    let sidecar_bytes = serde_json::to_vec(manifest)?;
    let sidecar_payload = object_store::PutPayload::from(sidecar_bytes);
    store.put(&sidecar_key, sidecar_payload).await?;
    debug!(key = %sidecar_key, "Uploaded sidecar manifest");

    Ok(())
}

/// Full conflict-safe backup: read the local generation, reject if the remote
/// head is newer work from another install (unless `force`), bump + persist the
/// generation, snapshot, upload, and prune. On any failure after the bump the
/// generation is rolled back so a failed attempt never silently advances it.
///
/// Returns [`BackupError::GenerationConflict`] when another device is ahead.
pub async fn perform_backup(
    pool: &SqlitePool,
    cfg: &AppConfig,
    store: &dyn ObjectStore,
    force: bool,
) -> Result<BackupManifest, BackupError> {
    let local_gen = generation(pool).await?;
    let prospective = local_gen + 1;
    let my_install = install_id(pool).await?;

    if !force
        && let Some(remote) =
            upload_conflict(store, &cfg.backup_prefix, prospective, &my_install).await?
    {
        return Err(BackupError::GenerationConflict {
            local_generation: local_gen,
            remote_generation: remote.generation,
            remote: Box::new(remote),
        });
    }

    // Persist the bump BEFORE snapshotting so the DB snapshot's app_meta carries
    // the same generation the manifest records (restores adopt it).
    set_generation(pool, prospective).await?;

    let outcome = async {
        let (bundle, manifest) = create_bundle(pool, cfg).await?;
        upload(store, &manifest, bundle).await?;
        prune(store, &cfg.backup_prefix, cfg.backup_retention).await?;
        Ok::<_, BackupError>(manifest)
    }
    .await;

    match outcome {
        Ok(m) => Ok(m),
        Err(e) => {
            let _ = set_generation(pool, local_gen).await;
            Err(e)
        }
    }
}

// ── List ──────────────────────────────────────────────────────────────────────

/// List remote backups newest-first by reading sidecar manifests.
pub async fn list(
    store: &dyn ObjectStore,
    prefix: &str,
) -> Result<Vec<BackupManifest>, BackupError> {
    use futures::StreamExt;

    let prefix_path = OsPath::from(format!("{}/", prefix));
    let mut stream = store.list(Some(&prefix_path));
    let mut manifests: Vec<BackupManifest> = Vec::new();

    while let Some(item) = stream.next().await {
        let meta = item?;
        let key = meta.location.to_string();
        if !key.ends_with(".manifest.json") {
            continue;
        }
        let bytes = store.get(&meta.location).await?.bytes().await?;
        match serde_json::from_slice::<BackupManifest>(&bytes) {
            Ok(m) => manifests.push(m),
            Err(e) => warn!(key = %key, error = %e, "Could not parse sidecar manifest, skipping"),
        }
    }

    // Sort newest first
    manifests.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(manifests)
}

// ── Download ─────────────────────────────────────────────────────────────────

/// Download a bundle by object key and return its raw bytes.
pub async fn download(store: &dyn ObjectStore, key: &str) -> Result<Vec<u8>, BackupError> {
    let path = OsPath::from(key);
    let bytes = store.get(&path).await?.bytes().await?;
    Ok(bytes.to_vec())
}

// ── Prune ─────────────────────────────────────────────────────────────────────

/// Delete oldest backups, keeping the newest `keep` bundles, and reconcile any
/// orphaned bundles (a `.tar.zst` with no sidecar manifest — the fingerprint of
/// a backup whose sidecar PUT failed). Returns number of bundles deleted.
pub async fn prune(
    store: &dyn ObjectStore,
    prefix: &str,
    keep: usize,
) -> Result<usize, BackupError> {
    let mut manifests = list(store, prefix).await?;
    let mut deleted = 0;

    // ── Retention: `list` returns newest-first; delete everything past `keep`.
    if manifests.len() > keep {
        let to_delete = manifests.split_off(keep);
        for m in &to_delete {
            let bundle_key = OsPath::from(m.object_key.clone());
            let sidecar_key = OsPath::from(format!("{}.manifest.json", m.object_key));
            if let Err(e) = store.delete(&bundle_key).await {
                warn!(key = %m.object_key, error = %e, "Failed to delete old bundle");
            } else {
                deleted += 1;
            }
            let _ = store.delete(&sidecar_key).await;
        }
    }

    // ── Reconcile orphans: any bundle object without a matching sidecar.
    let orphans = orphan_bundles(store, prefix).await?;
    for key in orphans {
        if let Err(e) = store.delete(&OsPath::from(key.clone())).await {
            warn!(key = %key, error = %e, "Failed to delete orphaned bundle");
        } else {
            debug!(key = %key, "Reconciled orphaned bundle (no sidecar)");
            deleted += 1;
        }
    }

    info!(deleted, kept = keep, "Pruned old remote backups");
    Ok(deleted)
}

/// List bundle object keys (`*.tar.zst`) that have no `*.tar.zst.manifest.json`
/// sidecar — the leftover of a bundle upload whose sidecar PUT never landed.
async fn orphan_bundles(store: &dyn ObjectStore, prefix: &str) -> Result<Vec<String>, BackupError> {
    use futures::StreamExt;
    use std::collections::HashSet;

    let prefix_path = OsPath::from(format!("{}/", prefix));
    let mut stream = store.list(Some(&prefix_path));

    let mut bundles: Vec<String> = Vec::new();
    let mut sidecar_targets: HashSet<String> = HashSet::new();

    while let Some(item) = stream.next().await {
        let key = item?.location.to_string();
        if let Some(bundle) = key.strip_suffix(".manifest.json") {
            sidecar_targets.insert(bundle.to_string());
        } else if key.ends_with(".tar.zst") {
            bundles.push(key);
        }
    }

    Ok(bundles
        .into_iter()
        .filter(|b| !sidecar_targets.contains(b))
        .collect())
}

// ── Restore helpers ───────────────────────────────────────────────────────────

/// A file extracted from a bundle, ready to be written to disk.
#[derive(Debug)]
struct ExtractedFile {
    dest: PathBuf,
    data: Vec<u8>,
}

/// Extract DB and attachments from a bundle and stage them for next startup.
///
/// Writes:
/// - `<db_path>.restore`       — the database snapshot
/// - `<storage_dir>.restore/`  — the extracted attachment tree (if present in bundle)
pub async fn stage_restore(
    bundle_bytes: Vec<u8>,
    manifest: &BackupManifest,
    local_migration_version: u32,
    db_path: &Path,
    storage_dir: &str,
) -> Result<(), BackupError> {
    // Reject unknown bundle formats before touching anything.
    if manifest.format_version != 1 {
        return Err(BackupError::UnsupportedFormat(manifest.format_version));
    }

    if manifest.migration_version > local_migration_version {
        return Err(BackupError::SchemaTooNew {
            snapshot: manifest.migration_version,
            local: local_migration_version,
        });
    }

    let restore_db_path = PathBuf::from(format!("{}.restore", db_path.to_string_lossy()));
    let restore_storage = format!("{}.restore", storage_dir);
    let restore_storage_path = PathBuf::from(&restore_storage);

    // Clear any leftovers from a previous (aborted) restore attempt so a new
    // bundle never merges into a stale staging tree.
    let _ = tokio::fs::remove_file(&restore_db_path).await;
    let _ = tokio::fs::remove_dir_all(&restore_storage_path).await;

    // All sync work (decompression + tar parsing) before any await points.
    let restore_storage_for_parse = restore_storage_path.clone();
    let (db_bytes, attachments) = tokio::task::spawn_blocking(move || {
        parse_bundle(&bundle_bytes, &restore_storage_for_parse)
    })
    .await
    .map_err(|e| BackupError::Io(std::io::Error::other(e.to_string())))??;

    // Integrity: the extracted database must hash to the manifest's db_sha256.
    // Guards against silent corruption/bit-rot and tampered bundles.
    let actual_sha = hex::encode(Sha256::digest(&db_bytes));
    if actual_sha != manifest.db_sha256 {
        return Err(BackupError::IntegrityMismatch);
    }

    // Async I/O: write the staged files.
    tokio::fs::write(&restore_db_path, &db_bytes).await?;

    for file in attachments {
        if let Some(parent) = file.dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&file.dest, &file.data).await?;
    }

    info!(
        db_restore = %restore_db_path.display(),
        storage_restore = %restore_storage,
        "Restore staged — restart the server to apply"
    );
    Ok(())
}

/// Verify a bundle without staging anything: checks `format_version`, that the
/// snapshot schema is not newer than the running binary, and that the extracted
/// database hashes to the manifest's `db_sha256`. Used by the "Verify" preview
/// so a restore can be validated before touching the live DB.
pub async fn verify_bundle(
    bundle_bytes: Vec<u8>,
    manifest: &BackupManifest,
    local_migration_version: u32,
) -> Result<(), BackupError> {
    if manifest.format_version != 1 {
        return Err(BackupError::UnsupportedFormat(manifest.format_version));
    }
    if manifest.migration_version > local_migration_version {
        return Err(BackupError::SchemaTooNew {
            snapshot: manifest.migration_version,
            local: local_migration_version,
        });
    }

    let expected_sha = manifest.db_sha256.clone();
    // Parse in a blocking task (decompress + tar walk). The staging path is a
    // throwaway — verify never writes.
    let db_bytes = tokio::task::spawn_blocking(move || {
        parse_bundle(&bundle_bytes, Path::new("/nonexistent-verify-staging"))
            .map(|(db, _attachments)| db)
    })
    .await
    .map_err(|e| BackupError::Io(std::io::Error::other(e.to_string())))??;

    let actual_sha = hex::encode(Sha256::digest(&db_bytes));
    if actual_sha != expected_sha {
        return Err(BackupError::IntegrityMismatch);
    }
    Ok(())
}

/// Decompress and parse a bundle synchronously. Returns `(db_bytes, attachment_files)`.
fn parse_bundle(
    bundle_bytes: &[u8],
    restore_storage_path: &Path,
) -> Result<(Vec<u8>, Vec<ExtractedFile>), BackupError> {
    let decompressed = zstd::decode_all(Cursor::new(bundle_bytes))
        .map_err(|e| BackupError::Zstd(e.to_string()))?;

    let mut archive = tar::Archive::new(Cursor::new(decompressed));
    let mut db_bytes: Option<Vec<u8>> = None;
    let mut attachments: Vec<ExtractedFile> = Vec::new();

    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_path = entry.path()?.into_owned();
        let name = entry_path.to_string_lossy();

        if name == "database.db" {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            db_bytes = Some(buf);
        } else if name.starts_with("attachments/") {
            let rel = entry_path
                .strip_prefix("attachments/")
                .unwrap_or(&entry_path);
            if rel.as_os_str().is_empty() {
                continue;
            }
            // Refuse path-traversal / absolute entries so a crafted bundle can
            // never escape the staging dir (mirrors tar::Entry::unpack_in).
            use std::path::Component;
            let unsafe_component = rel.components().any(|c| {
                matches!(
                    c,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            });
            if unsafe_component {
                return Err(BackupError::UnsafePath(name.into_owned()));
            }
            let dest = restore_storage_path.join(rel);
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            attachments.push(ExtractedFile { dest, data: buf });
        }
    }

    let db = db_bytes.ok_or(BackupError::CorruptBundle)?;
    Ok((db, attachments))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
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
            sqlx::query(
                "CREATE TABLE app_meta (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL)",
            )
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
}
