//! Server entry point, exposed as a library function so the single `tack`
//! binary (in the `tack-cli` crate) can start the HTTP server in-process.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::sync::oneshot;
use tracing::{info, warn};
use uuid::Uuid;

use crate::config::AppConfig;
use crate::handlers::local_runner::{LocalRunnerControl, effective_local_runner_enabled};
use crate::handlers::settings::effective_orch_enabled;
use crate::orch_store::build_control_plane_store;
use crate::remote_backup;
use crate::router::{AppState, build_router};
use crate::webhook::WebhookClient;
use tack_db::{Repository, init_pool, migrations, repo};
use tack_orch::reconciler;

/// Boot the Tack HTTP server: load config, run migrations, start background
/// tasks, and serve until a shutdown signal is received. No embedded runner
/// is ever wired in — `AppState::local_runner` is `None` and the routes in
/// `handlers::local_runner` are absent regardless of bind address. Use
/// [`serve_with_ready_and_local_runner`] to wire one in.
pub async fn serve() -> anyhow::Result<()> {
    serve_inner(None, None).await
}

/// Like [`serve`], but sends the real bound [`SocketAddr`] over `ready_tx`
/// once the listener is open and accepting connections, before this
/// function blocks on `axum::serve`. An in-process embedder can wait on the
/// receiving end to learn the exact address instead of polling a guessed
/// one — the only way to know it for certain when the configured port is
/// `0` and the OS picks the real one.
///
/// If the receiver has already been dropped, the send is a no-op: the
/// server still starts and runs normally, it just has nobody to tell.
/// No embedded runner is wired in — see [`serve`]'s doc comment.
pub async fn serve_with_ready(ready_tx: oneshot::Sender<SocketAddr>) -> anyhow::Result<()> {
    serve_inner(Some(ready_tx), None).await
}

/// Like [`serve_with_ready`], additionally wiring `local_runner` into
/// `AppState` so `handlers::local_runner`'s routes exist (still only on a
/// loopback bind — `router::build_router`'s own gate). `local_runner`'s
/// preference (`app_meta`, falling back to `AppConfig::local_runner_enable`
/// — `--with-runner`/`TACK_LOCAL_RUNNER_ENABLE`) is checked once the
/// listener is bound and, if on, started before this function blocks on
/// `axum::serve` — the exact same [`LocalRunnerControl::start`] call
/// `PUT /api/local-runner` makes later, so there is only ever one code path
/// into the runtime, not a boot-time one and a UI-triggered one. A start
/// failure at boot takes the whole process down (returned as an error)
/// rather than leaving a server running with the preference on but nothing
/// actually started; the caller of `PUT /api/local-runner` gets the same
/// error back instead, since starting the server itself never failed.
pub async fn serve_with_ready_and_local_runner(
    ready_tx: oneshot::Sender<SocketAddr>,
    local_runner: Arc<dyn LocalRunnerControl>,
) -> anyhow::Result<()> {
    serve_inner(Some(ready_tx), Some(local_runner)).await
}

async fn serve_inner(
    ready_tx: Option<oneshot::Sender<SocketAddr>>,
    local_runner: Option<Arc<dyn LocalRunnerControl>>,
) -> anyhow::Result<()> {
    // Load configuration
    let config = AppConfig::load();

    // Initialize logging/tracing
    init_tracing(&config);

    // Reject unsafe exposure before opening a database or listener.
    security_preflight(&config)?;

    info!(
        version = env!("CARGO_PKG_VERSION"),
        host = %config.host,
        port = config.port,
        database = %config.database_url,
        "Starting Tack server"
    );

    // Apply any staged restore before opening the pool.
    apply_staged_restore(&config);

    // Initialize database
    let pool = init_pool(&config.database_url).await?;
    migrations::run_all(&pool).await?;
    repo::templates::seed_builtin_templates(&pool).await?;

    // Ensure a default workspace exists
    let workspace_id = ensure_default_workspace(&pool).await?;

    // Build application state
    let repo = Repository::new(pool);

    // Create broadcast channel for WebSocket updates (capacity: 100 messages)
    let (broadcast_tx, _) = tokio::sync::broadcast::channel(100);

    let webhook = config.webhook_url.clone().map(|url| {
        info!(url=%url, "Webhook delivery enabled");
        WebhookClient::new(url, config.webhook_secret.clone())
    });

    let state = AppState {
        repo,
        config: config.clone(),
        workspace_id,
        broadcast_tx,
        webhook,
        orch_runtime: crate::orch_runtime::OrchRuntime::new(),
        local_runner: local_runner.clone(),
    };

    // Spawn background task: automatic remote backup on configured interval.
    // Guard the interval: `tokio::time::interval` panics on a zero duration, and
    // anything under a minute would hammer the object store, so clamp low values.
    //
    // The interval itself is env-only (`TACK_BACKUP_INTERVAL_SECS`), but whether
    // the destination is *configured* can also come from UI-saved settings, so
    // spawn whenever an interval is set and re-check the effective config each
    // tick — a UI-only cloud config still schedules.
    if let Some(interval_secs) = config.backup_interval_secs {
        const MIN_BACKUP_INTERVAL_SECS: u64 = 60;
        let interval_secs = if interval_secs < MIN_BACKUP_INTERVAL_SECS {
            warn!(
                requested = interval_secs,
                clamped_to = MIN_BACKUP_INTERVAL_SECS,
                "TACK_BACKUP_INTERVAL_SECS is below the 60s minimum; clamping"
            );
            MIN_BACKUP_INTERVAL_SECS
        } else {
            interval_secs
        };
        let bg_state = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            interval.tick().await; // skip the first immediate tick
            loop {
                interval.tick().await;
                run_scheduled_backup(&bg_state).await;
            }
        });
        info!(interval_secs, "Remote backup scheduler enabled");
    }

    // Spawn background task: fire "item.due_soon" webhook for items due within the next hour
    if state.webhook.is_some() {
        let bg_state = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(3600));
            interval.tick().await; // skip the first immediate tick
            loop {
                interval.tick().await;
                check_due_soon(&bg_state).await;
            }
        });
    }

    // Start the orchestration reconciler, one task per registered control
    // plane, polling `/health` + `/status.json` — if the *effective*
    // setting says to. Off by default; the effective
    // value is the `app_meta`-stored flag if the UI has ever set one, else
    // `TACK_ORCH_ENABLE`'s startup value — same precedence Cloud Backup
    // already uses for its own settings. Unlike Cloud Backup, this one also
    // has a runtime toggle: `PUT /api/settings/orchestration`
    // (`handlers/settings.rs`) calls `state.orch_runtime.start`/`.stop`
    // directly, so an operator can turn this on or off without a restart —
    // this boot-time call is just what makes the *initial* state agree with
    // whatever was last saved (or the env default, on a first-ever boot).
    if effective_orch_enabled(&state).await {
        // The store gets a clone of the
        // same broadcast sender every WebSocket subscriber shares, so it can
        // emit `BoardEvent::AgentRunUpdated`/`ApprovalPending` straight from
        // its `upsert_runs`/`upsert_approvals` — see orch_store.rs's module
        // doc for why the emit lives there and not in the reconciler.
        //
        // `with_app_context`
        // hands the store the rest of what `AppState` carries, so
        // `upsert_runs` can run `dispatcher::apply_mapped_status` — the
        // workflow engine — when a run reaches a terminal state, exactly
        // like a human-driven PATCH. See orch_store.rs's own doc comments
        // for the "human wins" design and why it's optional everywhere else.
        let store = build_control_plane_store(&state);
        state
            .orch_runtime
            .start(
                store,
                reconciler::ReconcilerConfig {
                    poll_secs: config.orch_poll_secs,
                    // Trace ingestion's event_retention_days must be the same
                    // cutoff `spawn_retention_sweep` uses
                    // (config.orch_event_retention_days both places) —
                    // persist_events' retention-composition guard depends on
                    // the two agreeing. See reconciler.rs's `ReconcilerConfig`
                    // doc comment.
                    event_retention_days: config.orch_event_retention_days,
                    ..Default::default()
                },
            )
            .await;
        // Resolved before the `info!` call, not inline in its arguments: an
        // `.await` inside a tracing macro's argument list holds a non-`Send`
        // formatting temporary across the await point, which makes the
        // enclosing function's future non-`Send` — fatal for a caller that
        // wants to `tokio::spawn` the server rather than only `block_on` it.
        let control_planes = state.orch_runtime.live_task_count().await;
        info!(
            control_planes,
            poll_secs = config.orch_poll_secs,
            "Orchestration reconciler enabled"
        );
    }

    // Start the execution-domain retention sweep, the artifact/event sweep +
    // decision-expiry sweep, and the health watch — three cancellable
    // background tasks gated by two flags
    // (`TACK_EXECUTION_RETENTION_ENABLE`/`TACK_EXECUTION_HEALTH_ENABLE`): the
    // artifact/event/decision sweep shares `retention_enable` with the
    // replay/idempotency purge above it, on purpose — see
    // `execution_runtime.rs::spawn_artifact_and_decision_sweep`'s own doc
    // comment for why artifact deletion must never be gated any more loosely
    // than that purge already is. Health defaults on (read-only: no outbound
    // call, no new API surface, just logging a `warn!` on a stale
    // lease/`needs_operator` request). Retention defaults **off** (see
    // `config.rs#default_execution_retention_enable`'s doc comment): it
    // deletes rows, so — unlike health — it needs an explicit operator
    // opt-in, the same posture `TACK_ORCH_ENABLE` already establishes for
    // this codebase. See `execution_runtime.rs`'s own doc comment for why
    // this isn't stored on `AppState`: `stop()` is called once below, after
    // the HTTP server itself has already stopped accepting requests, and
    // nothing in the current API surface needs to toggle it at runtime.
    let execution_runtime = crate::execution_runtime::ExecutionRuntime::new();
    execution_runtime
        .start(state.repo.clone(), (&config).into())
        .await;

    // A cheap `Clone` (mostly `Arc`s and a `Uuid`/`AppConfig` underneath),
    // kept only so the auto-start check below — after the listener binds,
    // below `build_router` moving `state` into the router it builds — still
    // has a live `AppState` to read `app_meta` through. `tack-cli` must
    // never open the database itself (`CLAUDE.md`'s crate map), so this
    // effective-enabled check has to happen here, not in the composing
    // binary, even though the binary is what decided whether to embed a
    // runner at all.
    let state_for_local_runner = state.clone();

    // Build router
    let app = build_router(state);

    // Start server
    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    info!(%addr, "Server listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;

    // `bind` already puts the socket in the listening state, so a
    // connection attempt against `bound_addr` succeeds from this point on
    // even before `axum::serve` below starts its accept loop — the kernel
    // queues it. That makes here, not function entry, the earliest correct
    // place to signal readiness.
    let bound_addr = listener.local_addr()?;
    if let Some(tx) = ready_tx {
        let _ = tx.send(bound_addr);
    }

    // Bring a wired-in embedded runner up to whatever its persisted
    // preference (or the `--with-runner`/`TACK_LOCAL_RUNNER_ENABLE` default,
    // where the UI has never overridden it) already says — the exact same
    // `LocalRunnerControl::start` call `PUT /api/local-runner` makes later,
    // so a boot-time start and a UI-triggered one are one code path, not
    // two. Loopback is re-checked here against the *persisted* preference,
    // separately from `local_runner.rs::ensure_loopback`'s check against
    // this boot's own flag: a preference saved from an earlier loopback
    // session must never auto-start a runner on a server now bound
    // elsewhere — it is silently not honored, matching
    // `router::build_router`'s identical rule for the route's own
    // existence, rather than erroring on an ordinary deployment that
    // happens to carry a stale row. A start failure while genuinely on
    // loopback takes the whole server down (loud, matching
    // `local_runner.rs`'s existing "either side dying kills the process"
    // posture for the `--with-runner` flag specifically) rather than
    // leaving it serving with the preference on but nothing running.
    if let Some(control) = &local_runner
        && state_for_local_runner.config.binds_loopback()
        && effective_local_runner_enabled(&state_for_local_runner).await
    {
        control.start().await?;
    }

    // Plain-stdout banner for end users running the distributed binary —
    // visible regardless of log level/format.
    let display_host = if config.host == "0.0.0.0" {
        "localhost"
    } else {
        &config.host
    };
    println!(
        "\n  Tack v{} is running.\n  Open http://{}:{} in your browser.\n  Press Ctrl+C to stop.\n",
        env!("CARGO_PKG_VERSION"),
        display_host,
        config.port
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    // Join the execution retention/health tasks before exiting — the HTTP
    // server has already stopped accepting requests at this point, so this
    // wait costs nothing observable and is what makes "shutdown joins task"
    // true rather than aspirational (see `execution_runtime.rs`'s doc
    // comment for why this differs from `OrchRuntime::stop`, which
    // deliberately does not block).
    execution_runtime.stop().await;

    info!("Server shut down gracefully");
    Ok(())
}

/// Validate the security boundary before any network-facing resources start.
fn security_preflight(config: &AppConfig) -> anyhow::Result<()> {
    config.validate_security()
}

/// Splits a configured log-file path into the directory and file name a file
/// appender needs, creating the directory if it is missing. Returns `None`
/// when the path names no file, or when its directory cannot be created —
/// logging to stdout is never given up because a file could not be opened.
fn log_file_target(path: &str) -> Option<(std::path::PathBuf, std::ffi::OsString)> {
    let path = std::path::Path::new(path);
    let name = path.file_name()?.to_owned();
    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => std::path::PathBuf::from("."),
    };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!(
            "tack: cannot create the log directory {}: {e}; logging to stdout only",
            dir.display()
        );
        return None;
    }
    Some((dir, name))
}

fn init_tracing(config: &AppConfig) {
    use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(format!(
            "tack_api={level},tack_db={level},tack_core={level},tower_http=debug",
            level = config.log_level
        ))
    });

    let fmt_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true);

    // A configured log file is written *in addition to* stdout, never instead
    // of it: a service manager captures stdout, and the file is what someone
    // can open without one. Colour codes are left out — nothing reads this
    // file through a terminal. The layer itself is built inside each branch
    // below because a layer is typed by the subscriber it stacks onto, and
    // the two branches stack onto different ones.
    let file_target = config.log_file.as_deref().and_then(log_file_target);
    let file_writer = |(dir, name): (std::path::PathBuf, std::ffi::OsString)| {
        tracing_appender::rolling::never(dir, name)
    };

    // `try_init` rather than `init`: this process may call `serve_inner` more
    // than once (multiple in-process servers under one test binary, e.g. under
    // `cargo llvm-cov`, which runs all tests as one process rather than
    // nextest's one-process-per-test). Only the first call can ever install
    // the global subscriber; a `Err` here means one already is installed, which
    // is exactly the outcome this call wanted, so it is not a failure to log or
    // propagate.
    if config.log_json {
        let _ = tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer.json())
            .with(file_target.map(|t| {
                fmt::layer()
                    .with_ansi(false)
                    .with_file(true)
                    .with_line_number(true)
                    .with_writer(file_writer(t))
                    .json()
            }))
            .try_init();
    } else {
        let _ = tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .with(file_target.map(|t| {
                fmt::layer()
                    .with_ansi(false)
                    .with_file(true)
                    .with_line_number(true)
                    .with_writer(file_writer(t))
            }))
            .try_init();
    }
}

async fn ensure_default_workspace(pool: &sqlx::SqlitePool) -> anyhow::Result<Uuid> {
    let existing: Option<String> = sqlx::query_scalar("SELECT id FROM workspaces LIMIT 1")
        .fetch_optional(pool)
        .await?;

    if let Some(id_str) = existing {
        Ok(Uuid::parse_str(&id_str)?)
    } else {
        let id = Uuid::new_v4();
        let vocab = serde_json::to_string(&tack_core::vocabulary::default_vocabulary())?;
        sqlx::query(
            "INSERT INTO workspaces (id, name, description, default_vocabulary) VALUES (?, 'Default Workspace', 'Auto-created workspace', ?)"
        )
        .bind(id.to_string())
        .bind(&vocab)
        .execute(pool)
        .await?;
        info!(%id, "Created default workspace");
        Ok(id)
    }
}

/// If a `.restore` file exists next to the live DB, apply it before startup.
///
/// When the restore originated from a remote bundle, an attachments staging dir
/// (`<storage_dir>.restore/`) may also exist. The swap is fail-safe: on any
/// failure it rolls back to the original files rather than booting an empty DB.
/// Stale `-wal`/`-shm` sidecars of the old DB are deleted first so
/// SQLite cannot replay them onto the freshly restored database.
fn apply_staged_restore(config: &AppConfig) {
    use std::path::Path;

    let Some(db_path) = config.db_file_path() else {
        return;
    };
    let restore_str = format!("{}.restore", db_path.to_string_lossy());
    let restore_path = Path::new(&restore_str);
    if !restore_path.exists() {
        return;
    }
    warn!(path = %restore_str, "Applying staged restore");

    let ts = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let storage_restore = format!("{}.restore", config.storage_dir);
    let storage_restore_path = Path::new(&storage_restore);

    match apply_restore_swap(
        &db_path,
        restore_path,
        Path::new(&config.storage_dir),
        storage_restore_path,
        &ts,
    ) {
        Ok(baks) => {
            info!(
                db_bak = %baks.db_bak,
                "Restore applied; previous DB preserved as a timestamped .bak"
            );
            // Keep only the newest .bak generation for each target.
            prune_old_baks(&db_path, &baks.db_bak);
            if let Some(storage_bak) = &baks.storage_bak {
                prune_old_baks(Path::new(&config.storage_dir), storage_bak);
            }
        }
        Err(e) => {
            warn!("Staged restore failed and was rolled back; the original database is intact: {e}")
        }
    }
}

/// Paths of the timestamped backups produced by a successful swap.
struct RestoreBaks {
    db_bak: String,
    storage_bak: Option<String>,
}

/// Fail-safe swap of the staged DB (and optional attachments dir) into place.
///
/// Sequence, each step rolled back on failure of a later step:
/// 1. delete stale `<db>-wal`/`<db>-shm`
/// 2. move current DB → `<db>.bak-<ts>`
/// 3. promote `<db>.restore` → live DB
/// 4. (if a storage staging dir exists) move current storage → `<dir>.bak-<ts>`
/// 5. promote `<dir>.restore` → live storage dir
///
/// On success returns the `.bak` paths. On any failure the original DB and
/// storage dir are put back so the server boots the pre-restore state.
fn apply_restore_swap(
    db_path: &std::path::Path,
    restore_db: &std::path::Path,
    storage_dir: &std::path::Path,
    storage_restore: &std::path::Path,
    ts: &str,
) -> std::io::Result<RestoreBaks> {
    // 1. Delete stale WAL/SHM of the DB about to be replaced.
    let wal = format!("{}-wal", db_path.to_string_lossy());
    let shm = format!("{}-shm", db_path.to_string_lossy());
    let _ = std::fs::remove_file(&wal);
    let _ = std::fs::remove_file(&shm);

    // 2. Move the current DB aside (may not exist on a fresh install).
    let db_bak = format!("{}.bak-{}", db_path.to_string_lossy(), ts);
    let db_existed = db_path.exists();
    if db_existed {
        std::fs::rename(db_path, &db_bak)?;
    }

    // 3. Promote the restored DB.
    if let Err(e) = std::fs::rename(restore_db, db_path) {
        // Roll back: put the original DB back.
        if db_existed {
            let _ = std::fs::rename(&db_bak, db_path);
        }
        return Err(e);
    }

    // 4/5. Storage swap (only when a staging dir is present).
    let mut storage_bak = None;
    if storage_restore.exists() {
        let bak = format!("{}.bak-{}", storage_dir.to_string_lossy(), ts);
        let storage_existed = storage_dir.exists();
        if storage_existed && let Err(e) = std::fs::rename(storage_dir, &bak) {
            // Roll back the DB swap.
            let _ = std::fs::rename(db_path, restore_db);
            if db_existed {
                let _ = std::fs::rename(&db_bak, db_path);
            }
            return Err(e);
        }
        if let Err(e) = std::fs::rename(storage_restore, storage_dir) {
            // Roll back storage, then the DB swap.
            if storage_existed {
                let _ = std::fs::rename(&bak, storage_dir);
            }
            let _ = std::fs::rename(db_path, restore_db);
            if db_existed {
                let _ = std::fs::rename(&db_bak, db_path);
            }
            return Err(e);
        }
        storage_bak = Some(bak);
    }

    Ok(RestoreBaks {
        db_bak,
        storage_bak,
    })
}

/// Delete older `<base>.bak-*` generations, keeping only `keep`. Best-effort.
fn prune_old_baks(base: &std::path::Path, keep: &str) {
    let Some(parent) = base.parent() else { return };
    let Some(name) = base.file_name().and_then(|n| n.to_str()) else {
        return;
    };
    let prefix = format!("{name}.bak-");
    let keep_name = std::path::Path::new(keep)
        .file_name()
        .and_then(|n| n.to_str());
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let fname = entry.file_name();
        let Some(fname) = fname.to_str() else {
            continue;
        };
        if fname.starts_with(&prefix) && Some(fname) != keep_name {
            let path = entry.path();
            if path.is_dir() {
                let _ = std::fs::remove_dir_all(&path);
            } else {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C handler");
    warn!("Shutdown signal received");
}

/// Run a scheduled remote backup. Reads the *effective* config each tick so
/// UI-saved settings (bucket/prefix/creds/retention) are honored, and runs
/// the conflict-safe upload path — on a cross-device conflict it warns and
/// skips rather than clobbering another device's newer work.
async fn run_scheduled_backup(state: &AppState) {
    let cfg = crate::handlers::settings::effective_backup_config(state).await;
    if !cfg.remote_backup_enabled() {
        // A UI-only config may not be set yet; nothing to do this tick.
        tracing::debug!("Scheduled backup skipped: remote backup not configured");
        return;
    }

    let store = match remote_backup::store_from_config(&cfg) {
        Ok(store) => store,
        Err(e) => {
            warn!(error = %e, "Scheduled backup store init failed");
            return;
        }
    };

    // Scheduled backups never force: a conflict means another device is ahead,
    // so we skip and let the user restore/resolve.
    match remote_backup::perform_backup(state.pool(), &cfg, store.as_ref(), false).await {
        Ok(manifest) => info!(key = %manifest.object_key, "Scheduled backup complete"),
        Err(remote_backup::BackupError::GenerationConflict {
            remote_generation, ..
        }) => {
            warn!(
                remote_generation,
                "Scheduled backup skipped: another device has newer work (restore to resolve)"
            );
        }
        Err(e) => warn!(error = %e, "Scheduled backup failed"),
    }
}

/// Query for items due within the next hour and fire `item.due_soon` webhooks.
async fn check_due_soon(state: &AppState) {
    let Some(wh) = &state.webhook else { return };

    let now = Utc::now();
    let window_end = now + chrono::Duration::hours(1);

    match state.repo.list_items_due_soon(now, window_end).await {
        Ok(items) => {
            for item in items {
                let payload = serde_json::json!({
                    "event": "item.due_soon",
                    "timestamp": now.to_rfc3339(),
                    "project_id": item.project_id,
                    "item": item,
                });
                wh.fire("item.due_soon", payload);
            }
        }
        Err(e) => {
            tracing::warn!(error=%e, "Failed to query due-soon items for webhook");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    /// Working directory that removes itself when the returned guard drops,
    /// so a failed assertion leaves nothing behind either.
    fn workdir(tag: &str) -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix(tag)
            .tempdir()
            .expect("temporary directory")
    }

    #[test]
    fn a_configured_log_path_creates_its_directory_and_splits_into_dir_and_name() {
        let dir_guard = workdir("log-target");
        let nested = dir_guard.path().join("logs").join("tack.log");
        assert!(!nested.parent().unwrap().exists());

        let (dir, name) = log_file_target(nested.to_str().unwrap()).expect("a usable target");

        assert_eq!(dir, nested.parent().unwrap());
        assert_eq!(name, "tack.log");
        assert!(
            dir.is_dir(),
            "the log directory must be created, not assumed"
        );
    }

    #[test]
    fn a_bare_file_name_logs_beside_the_working_directory() {
        let (dir, name) = log_file_target("tack.log").expect("a usable target");
        assert_eq!(dir, Path::new("."));
        assert_eq!(name, "tack.log");
    }

    #[test]
    fn a_path_naming_no_file_is_refused_rather_than_guessed() {
        assert!(log_file_target("/").is_none());
        assert!(log_file_target("..").is_none());
    }

    /// The whole point of the setting: after `init_tracing` has read a config
    /// carrying a log path, a line logged through the global subscriber has to
    /// land in that file. Asserting that a layer was constructed would have
    /// passed for the entire time this setting silently did nothing. This test
    /// installs the process-wide subscriber, so it must stay the only test in
    /// this binary that does.
    #[test]
    fn a_configured_log_file_receives_the_lines_that_are_logged() {
        let dir_guard = workdir("log-write");
        let path = dir_guard.path().join("logs").join("tack.log");

        let config = AppConfig {
            log_file: Some(path.to_string_lossy().into_owned()),
            log_level: "info".into(),
            log_json: false,
            ..AppConfig::default()
        };
        init_tracing(&config);

        tracing::error!(marker = "written-to-the-file", "log file smoke line");

        let written = fs::read_to_string(&path).expect("the log file must exist");
        assert!(
            written.contains("written-to-the-file"),
            "the configured log file got no line; it holds: {written:?}"
        );
    }

    /// The DB swap succeeds but the storage swap fails — the whole
    /// operation must roll back so the original DB is intact and bootable.
    #[test]
    fn restore_swap_rolls_back_when_storage_swap_fails() {
        let dir_guard = workdir("storage-fail");
        let dir = dir_guard.path();
        let db_path = dir.join("tack.db");
        let restore_db = dir.join("tack.db.restore");
        fs::write(&db_path, b"ORIGINAL").unwrap();
        fs::write(&restore_db, b"RESTORED").unwrap();

        // A staging dir exists (so the storage step runs)…
        let storage_restore = dir.join("storage.restore");
        fs::create_dir_all(&storage_restore).unwrap();
        fs::write(storage_restore.join("a.bin"), b"attach").unwrap();

        // …but the live storage path sits under a MISSING parent, so promoting
        // the staging dir into place fails with ENOENT — after the DB swap.
        let storage_dir = dir.join("missing_parent").join("storage");

        let result =
            apply_restore_swap(&db_path, &restore_db, &storage_dir, &storage_restore, "TS");
        assert!(result.is_err(), "storage promotion should fail");

        // Rolled back: the ORIGINAL DB is back in place and bootable.
        assert_eq!(fs::read(&db_path).unwrap(), b"ORIGINAL");
        // The staged restore file is put back for a future attempt.
        assert_eq!(fs::read(&restore_db).unwrap(), b"RESTORED");
        // No stray .bak left behind for the DB.
        assert!(!Path::new(&format!("{}.bak-TS", db_path.to_string_lossy())).exists());

        fs::remove_dir_all(dir).ok();
    }

    /// Happy path: DB + storage both swap, and a timestamped .bak is kept.
    #[test]
    fn restore_swap_promotes_db_and_storage() {
        let dir_guard = workdir("ok");
        let dir = dir_guard.path();
        let db_path = dir.join("tack.db");
        let restore_db = dir.join("tack.db.restore");
        fs::write(&db_path, b"ORIGINAL").unwrap();
        fs::write(&restore_db, b"RESTORED").unwrap();

        let storage_dir = dir.join("storage");
        fs::create_dir_all(&storage_dir).unwrap();
        fs::write(storage_dir.join("old.bin"), b"old").unwrap();
        let storage_restore = dir.join("storage.restore");
        fs::create_dir_all(&storage_restore).unwrap();
        fs::write(storage_restore.join("new.bin"), b"new").unwrap();

        let baks = apply_restore_swap(&db_path, &restore_db, &storage_dir, &storage_restore, "TS")
            .expect("swap should succeed");

        assert_eq!(fs::read(&db_path).unwrap(), b"RESTORED");
        assert!(storage_dir.join("new.bin").exists());
        assert!(!storage_dir.join("old.bin").exists());
        assert!(Path::new(&baks.db_bak).exists());
        assert!(baks.storage_bak.is_some());
        // The staging inputs are consumed.
        assert!(!restore_db.exists());
        assert!(!storage_restore.exists());

        fs::remove_dir_all(dir).ok();
    }

    /// Stale `-wal`/`-shm` sidecars of the replaced DB are deleted so SQLite
    /// cannot replay them onto the freshly restored database.
    #[test]
    fn restore_swap_deletes_stale_wal_shm() {
        let dir_guard = workdir("wal");
        let dir = dir_guard.path();
        let db_path = dir.join("tack.db");
        let restore_db = dir.join("tack.db.restore");
        fs::write(&db_path, b"ORIGINAL").unwrap();
        fs::write(&restore_db, b"RESTORED").unwrap();
        fs::write(dir.join("tack.db-wal"), b"waljunk").unwrap();
        fs::write(dir.join("tack.db-shm"), b"shmjunk").unwrap();

        // No storage staging dir → storage step is skipped.
        let storage_dir = dir.join("storage");
        let storage_restore = dir.join("storage.restore");

        apply_restore_swap(&db_path, &restore_db, &storage_dir, &storage_restore, "TS").unwrap();

        assert!(
            !dir.join("tack.db-wal").exists(),
            "stale -wal must be deleted"
        );
        assert!(
            !dir.join("tack.db-shm").exists(),
            "stale -shm must be deleted"
        );
        assert_eq!(fs::read(&db_path).unwrap(), b"RESTORED");

        fs::remove_dir_all(dir).ok();
    }

    // Serializes the tests below that must mutate process env vars to steer
    // `AppConfig::load()`, since env vars are process-global and `cargo
    // test` runs this binary's tests concurrently. An async-aware mutex
    // because the guard needs to stay held across this test's `.await`s.
    static SERVE_ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    /// Puts back whatever `TACK_PORT`/`TACK_DATABASE_URL` held before the
    /// test ran, including on panic, so a failure here can't leak a stray
    /// `TACK_PORT=0` into a test that runs after it in the same process.
    struct EnvRestore {
        port: Option<String>,
        database_url: Option<String>,
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            // SAFETY: the caller holds `SERVE_ENV_LOCK` for the guard's
            // whole lifetime, so no other thread in this test binary reads
            // or writes these two vars concurrently with this restore.
            unsafe {
                match &self.port {
                    Some(v) => std::env::set_var("TACK_PORT", v),
                    None => std::env::remove_var("TACK_PORT"),
                }
                match &self.database_url {
                    Some(v) => std::env::set_var("TACK_DATABASE_URL", v),
                    None => std::env::remove_var("TACK_DATABASE_URL"),
                }
            }
        }
    }

    /// Acceptance case for the readiness seam: start the server through
    /// `serve_with_ready`, wait on the oneshot (no sleep, no retry loop),
    /// then make one real HTTP request against the address it reports.
    /// Requesting port 0 forces an OS-assigned port, so a passing request
    /// also proves the signaled address is the real one, not the guess a
    /// caller would otherwise have to poll.
    #[tokio::test]
    async fn serve_with_ready_signals_the_real_bound_address() {
        let _env_guard = SERVE_ENV_LOCK.lock().await;
        let _restore = EnvRestore {
            port: std::env::var("TACK_PORT").ok(),
            database_url: std::env::var("TACK_DATABASE_URL").ok(),
        };
        // SAFETY: serialized by `SERVE_ENV_LOCK` above.
        unsafe {
            std::env::set_var("TACK_PORT", "0");
            std::env::set_var("TACK_DATABASE_URL", "sqlite::memory:");
        }

        let (ready_tx, ready_rx) = oneshot::channel();
        let server = tokio::spawn(serve_with_ready(ready_tx));

        let addr = ready_rx.await.expect("readiness signal never arrived");
        assert_ne!(
            addr.port(),
            0,
            "signaled address must be the real OS-assigned port, not the configured 0"
        );

        let response = reqwest::get(format!("http://{addr}/api/health"))
            .await
            .expect("request against the signaled address must succeed");
        assert!(response.status().is_success());

        server.abort();
    }

    /// The unmodified `serve()` entry point must still exist and behave the
    /// same as before this seam was added: no readiness channel, same boot
    /// sequence. This doesn't run it (that's `cargo run -p tack-cli -- serve`,
    /// verified manually), it just pins the call shape so `serve_inner`'s
    /// `None` path can't silently drift from what `tack-cli` calls.
    #[test]
    fn serve_still_takes_no_arguments() {
        let _: fn() -> _ = serve;
    }

    /// A control whose `start()` records whether it was ever called — the
    /// auto-start check under test in
    /// `a_persisted_enable_preference_never_auto_starts_on_a_non_loopback_bind`
    /// below is the only thing calling it in that test.
    struct RecordingControl {
        started: std::sync::atomic::AtomicBool,
    }

    #[async_trait::async_trait]
    impl LocalRunnerControl for RecordingControl {
        async fn status(&self) -> crate::handlers::local_runner::RuntimeStatus {
            crate::handlers::local_runner::RuntimeStatus {
                state: crate::handlers::local_runner::RuntimeState::Stopped,
                since: None,
            }
        }
        async fn start(
            &self,
        ) -> Result<(), crate::handlers::local_runner::LocalRunnerControlError> {
            self.started
                .store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
        async fn stop(&self) {}
        async fn list_secrets(&self) -> Vec<crate::handlers::local_runner::SecretMeta> {
            Vec::new()
        }
        async fn set_secret(
            &self,
            _name: &str,
            _value: &str,
        ) -> Result<(), crate::handlers::local_runner::LocalRunnerControlError> {
            Ok(())
        }
        async fn remove_secret(
            &self,
            _name: &str,
        ) -> Result<(), crate::handlers::local_runner::LocalRunnerControlError> {
            Ok(())
        }
        async fn catalog(&self) -> crate::handlers::local_runner::CatalogSnapshot {
            crate::handlers::local_runner::CatalogSnapshot::NotConfigured
        }
    }

    /// The regression this seam exists to prevent: a preference saved from
    /// an earlier loopback session (here, simulated with
    /// `TACK_LOCAL_RUNNER_ENABLE=1` — the env-default half of the same
    /// precedence `app_meta` would otherwise win) must never auto-start an
    /// embedded runner on a server now bound to `0.0.0.0`. Proved directly
    /// against `RecordingControl::start`, not just against the route being
    /// absent — a route being unreachable would not by itself prove the
    /// runner never actually started.
    #[tokio::test]
    async fn a_persisted_enable_preference_never_auto_starts_on_a_non_loopback_bind() {
        let _env_guard = SERVE_ENV_LOCK.lock().await;
        let _restore = EnvRestore {
            port: std::env::var("TACK_PORT").ok(),
            database_url: std::env::var("TACK_DATABASE_URL").ok(),
        };
        let previous_host = std::env::var("TACK_HOST").ok();
        let previous_enable = std::env::var("TACK_LOCAL_RUNNER_ENABLE").ok();
        let previous_unauth = std::env::var("TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK").ok();
        // SAFETY: serialized by `SERVE_ENV_LOCK` above.
        unsafe {
            std::env::set_var("TACK_PORT", "0");
            std::env::set_var("TACK_DATABASE_URL", "sqlite::memory:");
            std::env::set_var("TACK_HOST", "0.0.0.0");
            std::env::set_var("TACK_LOCAL_RUNNER_ENABLE", "1");
            std::env::set_var("TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK", "1");
        }

        let control = Arc::new(RecordingControl {
            started: std::sync::atomic::AtomicBool::new(false),
        });
        let local_runner: Arc<dyn LocalRunnerControl> = control.clone();
        let (ready_tx, ready_rx) = oneshot::channel();
        let server = tokio::spawn(serve_with_ready_and_local_runner(ready_tx, local_runner));

        let addr = ready_rx.await.expect("readiness signal never arrived");
        // The server itself must still come up normally — refusing to
        // *start the runner* is not refusing to *serve*.
        let response = reqwest::get(format!("http://{addr}/api/health"))
            .await
            .expect("server must still boot and serve on a non-loopback bind");
        assert!(response.status().is_success());

        assert!(
            !control.started.load(std::sync::atomic::Ordering::SeqCst),
            "a non-loopback bind must never auto-start the embedded runner, \
             even with TACK_LOCAL_RUNNER_ENABLE=1"
        );

        server.abort();
        // SAFETY: still serialized by `SERVE_ENV_LOCK`.
        unsafe {
            match previous_host {
                Some(v) => std::env::set_var("TACK_HOST", v),
                None => std::env::remove_var("TACK_HOST"),
            }
            match previous_enable {
                Some(v) => std::env::set_var("TACK_LOCAL_RUNNER_ENABLE", v),
                None => std::env::remove_var("TACK_LOCAL_RUNNER_ENABLE"),
            }
            match previous_unauth {
                Some(v) => std::env::set_var("TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK", v),
                None => std::env::remove_var("TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK"),
            }
        }
    }
}
