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
#[path = "remote_backup/tests.rs"]
mod tests;
