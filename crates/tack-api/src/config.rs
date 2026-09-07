use serde::{Deserialize, Serialize};

/// Application configuration loaded from tack.toml or environment variables.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_database_url")]
    pub database_url: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default)]
    pub log_json: bool,
    #[serde(default = "default_log_file")]
    pub log_file: Option<String>,
    #[serde(default = "default_storage_dir")]
    pub storage_dir: String,

    /// Origins allowed by CORS. Comma-separated in env ($TACK_ALLOWED_ORIGINS).
    /// Defaults to localhost variants suitable for local-first use.
    #[serde(default = "default_allowed_origins")]
    pub allowed_origins: Vec<String>,

    /// Global maximum body size for non-attachment requests (bytes).
    /// Attachments use their own higher limit. Default: 2 MB.
    #[serde(default = "default_max_body_size_bytes")]
    pub max_body_size_bytes: usize,

    /// Optional Bearer token. When set, all `/api/*` routes (except `/api/health`)
    /// require `Authorization: Bearer <token>`. Leave unset for pure-local use.
    #[serde(default)]
    pub api_token: Option<String>,

    /// Optional outbound webhook URL. When set, Tack POSTs a JSON payload to
    /// this URL on every item create/update/delete, sprint status change, and
    /// when items become due within the next hour (background check every 60 min).
    #[serde(default)]
    pub webhook_url: Option<String>,

    /// Optional HMAC-SHA256 signing secret for webhook deliveries. When set,
    /// each request includes an `X-Tack-Signature: sha256=<hex>` header so
    /// the receiver can verify authenticity.
    #[serde(default)]
    pub webhook_secret: Option<String>,

    /// Optional GitHub personal access token (`repo` scope). When set, item
    /// status changes are pushed back to linked GitHub issues (push-only).
    /// Never logged.
    #[serde(default)]
    pub github_token: Option<String>,

    /// GitHub API base URL. Override for GitHub Enterprise or to point tests at
    /// a mock server. Defaults to `https://api.github.com`.
    #[serde(default = "default_github_api_base")]
    pub github_api_base: String,

    // ── Remote backup (S3-compatible object storage) ──────────────────────────
    /// S3-compatible endpoint URL. Omit for AWS S3; set for R2/B2/MinIO.
    /// Example: `https://<account>.r2.cloudflarestorage.com`
    #[serde(default)]
    pub backup_endpoint: Option<String>,

    /// Bucket name. Required to enable remote backup.
    #[serde(default)]
    pub backup_bucket: Option<String>,

    /// AWS/S3 region. Cloudflare R2 uses `auto`; AWS requires the real region.
    #[serde(default = "default_backup_region")]
    pub backup_region: String,

    /// S3 access key ID. Required to enable remote backup.
    #[serde(default)]
    pub backup_access_key: Option<String>,

    /// S3 secret access key. Required to enable remote backup. Never logged.
    #[serde(default)]
    pub backup_secret_key: Option<String>,

    /// Object key prefix inside the bucket. Default: `tack`.
    #[serde(default = "default_backup_prefix")]
    pub backup_prefix: String,

    /// Automatic backup interval in seconds. Omit (None) for manual-only backups.
    #[serde(default)]
    pub backup_interval_secs: Option<u64>,

    /// How many remote backups to retain after each upload. Default: 10.
    #[serde(default = "default_backup_retention")]
    pub backup_retention: usize,

    // ── Agent-Factory Control Center (orchestration) ───────────────────────────
    /// Enables the orchestration reconciler and every control-plane API route
    /// (`/api/control-planes`, `/api/projects/{id}/orch-link`, `/api/fleet`).
    /// **Off by default.** With this unset, no reconciler task is spawned (see
    /// `server.rs`) and every orch route 404s.
    #[serde(default)]
    pub orch_enable: bool,

    /// Base reconciler poll interval in seconds, before per-plane backoff and
    /// jitter are applied. Default: 10.
    #[serde(default = "default_orch_poll_secs")]
    pub orch_poll_secs: u64,

    /// How many days of `orch_events` (and, `orch_metrics`)
    /// history to keep before the retention sweep rolls old rows into per-day
    /// aggregates and deletes them. Default: 90.
    #[serde(default = "default_orch_event_retention_days")]
    pub orch_event_retention_days: u32,

    /// Shared secret required to grant/deny a docket approval via
    /// `POST /api/approvals/{token}`. Deliberately separate from
    /// `TACK_API_TOKEN`: granting an approval is a materially higher-privilege
    /// act than editing a card, so holding the ordinary API token is not enough
    /// on its own. Never logged.
    #[serde(default)]
    pub orch_approval_token: Option<String>,

    // ── Embedded runner (ADR 0061 decisions 2 and 6) ───────────────────────
    /// The env/CLI-flag default for whether the in-process embedded runner
    /// starts. `--with-runner`/`TACK_LOCAL_RUNNER_ENABLE` set this; the UI
    /// toggle (`PUT /api/local-runner`) overrides it in `app_meta` from
    /// then on, the same precedence `orch_enable` already establishes —
    /// see `handlers::local_runner::effective_local_runner_enabled`.
    #[serde(default)]
    pub local_runner_enable: bool,

    // ── Execution runtime retention/observability ───────────────
    /// Enables the execution-domain retention sweep (replay/idempotency
    /// bookkeeping + terminal `execution_events` purge — see
    /// `tack_orch::execution_retention`). **Off by default** — see
    /// `default_execution_retention_enable`'s doc comment for the full
    /// rationale. This sweep deletes rows; data deletion must be an
    /// operator opt-in, the same posture `TACK_ORCH_ENABLE` already
    /// establishes for this codebase.
    #[serde(default = "default_execution_retention_enable")]
    pub execution_retention_enable: bool,

    /// Days of replay/idempotency bookkeeping and terminal `execution_events`
    /// history kept before the retention sweep purges them. Default: 90 —
    /// matching `TACK_ORCH_EVENT_RETENTION_DAYS`'s own default.
    #[serde(default = "default_execution_retention_days")]
    pub execution_retention_days: u32,

    /// Interval, in seconds, between execution-retention sweeps. Default:
    /// 3600 (hourly) — retention is a hygiene task, not a latency-sensitive
    /// one, so this is deliberately far coarser than `TACK_ORCH_POLL_SECS`.
    #[serde(default = "default_execution_retention_interval_secs")]
    pub execution_retention_interval_secs: u64,

    /// Enables the execution-domain health watch (runner/queue/lease/event
    /// counts; logs a `warn!` on stale-lease/`needs_operator` onset — see
    /// `tack_orch::execution_observability`). **On by default** — unlike
    /// retention above, this reads and logs only; it deletes nothing, so it
    /// carries none of retention's data-loss risk.
    #[serde(default = "default_execution_health_enable")]
    pub execution_health_enable: bool,

    /// Interval, in seconds, between execution health-watch checks. Default: 60.
    #[serde(default = "default_execution_health_interval_secs")]
    pub execution_health_interval_secs: u64,

    /// Shared secret required to resolve a scoped execution decision via
    /// `POST /api/attempts/{attempt_id}/decisions/{decision_id}/resolve`.
    /// Deliberately separate from `TACK_API_TOKEN`, mirroring
    /// `orch_approval_token`'s own precedent exactly: resolving a decision
    /// releases whatever the harness/runner is blocked on, a materially
    /// higher-privilege act than the ordinary operator gate already covers.
    /// **Fail-closed when unset** — see
    /// `handlers::decisions::require_decision_token`'s doc comment. Never
    /// logged.
    #[serde(default)]
    pub execution_decision_token: Option<String>,

    /// Explicit escape hatch for `validate_security`'s refusal to start an
    /// unauthenticated non-loopback server. Tack has no per-user identity
    /// model, so a missing `TACK_API_TOKEN` on a reachable bind exposes full
    /// read/write access to whoever can reach the port; a container
    /// deployment that necessarily binds `0.0.0.0` on a network the operator
    /// already trusts (a private Docker network, a host firewall) is the one
    /// legitimate case for accepting that risk anyway. **Off by default** —
    /// setting it is a deliberate, documented choice, never a silent
    /// fallback the code takes on its own.
    #[serde(default)]
    pub allow_unauthenticated_nonloopback: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            database_url: default_database_url(),
            log_level: default_log_level(),
            log_json: false,
            log_file: default_log_file(),
            storage_dir: default_storage_dir(),
            allowed_origins: default_allowed_origins(),
            max_body_size_bytes: default_max_body_size_bytes(),
            api_token: None,
            webhook_url: None,
            webhook_secret: None,
            github_token: None,
            github_api_base: default_github_api_base(),
            backup_endpoint: None,
            backup_bucket: None,
            backup_region: default_backup_region(),
            backup_access_key: None,
            backup_secret_key: None,
            backup_prefix: default_backup_prefix(),
            backup_interval_secs: None,
            backup_retention: default_backup_retention(),
            orch_enable: false,
            orch_poll_secs: default_orch_poll_secs(),
            orch_event_retention_days: default_orch_event_retention_days(),
            orch_approval_token: None,
            local_runner_enable: false,
            execution_retention_enable: default_execution_retention_enable(),
            execution_retention_days: default_execution_retention_days(),
            execution_retention_interval_secs: default_execution_retention_interval_secs(),
            execution_health_enable: default_execution_health_enable(),
            execution_health_interval_secs: default_execution_health_interval_secs(),
            execution_decision_token: None,
            allow_unauthenticated_nonloopback: false,
        }
    }
}

fn default_github_api_base() -> String {
    "https://api.github.com".into()
}
fn default_host() -> String {
    "127.0.0.1".into()
}
fn default_port() -> u16 {
    3210
}
fn default_database_url() -> String {
    "sqlite:tack.db?mode=rwc".into()
}
fn default_log_level() -> String {
    "info".into()
}
fn default_log_file() -> Option<String> {
    None
}
fn default_storage_dir() -> String {
    "./storage".into()
}
fn default_max_body_size_bytes() -> usize {
    2 * 1024 * 1024
} // 2 MB

fn default_backup_region() -> String {
    "auto".into()
}
fn default_backup_prefix() -> String {
    "tack".into()
}
fn default_backup_retention() -> usize {
    10
}

fn default_orch_poll_secs() -> u64 {
    10
}
fn default_orch_event_retention_days() -> u32 {
    90
}

fn default_execution_retention_enable() -> bool {
    // This sweep deletes rows (`purge_stale_execution_replays`,
    // `purge_stale_terminal_execution_events`) — data deletion must be
    // opt-in, matching `TACK_ORCH_ENABLE`'s own off-by-default posture, not
    // read-only hygiene the operator never asked for. `execution_health_enable`
    // stays `true` below: it only reads and logs, it deletes nothing.
    false
}
fn default_execution_retention_days() -> u32 {
    90
}
fn default_execution_retention_interval_secs() -> u64 {
    3600
}
fn default_execution_health_enable() -> bool {
    true
}
fn default_execution_health_interval_secs() -> u64 {
    60
}

fn default_allowed_origins() -> Vec<String> {
    vec![
        "http://localhost:8080".into(),
        "http://127.0.0.1:8080".into(),
        "http://localhost:3210".into(),
        "http://127.0.0.1:3210".into(),
        "https://tack.test".into(),
    ]
}

/// True when `host` names a loopback address. Shared by `AppConfig::binds_loopback`
/// (the server's own bind address) and `middleware::board_websocket_is_authorized`
/// (a browser `Origin` header's host) so both recognize the same set of
/// "this machine" names.
pub fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    // Only a parsed address counts: a DNS name that merely begins with
    // `127.` (`127.attacker.example`) is somebody else's host, and a browser
    // `Origin` can carry exactly that. IPv6 literals arrive bracketed from a
    // URL host and bare from a bind address; both forms are accepted.
    let literal = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')).unwrap_or(host);
    match literal.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => ip.is_loopback(),
        Ok(std::net::IpAddr::V6(ip)) => {
            ip.is_loopback() || ip.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
        }
        Err(_) => false,
    }
}

impl AppConfig {
    /// True when this bind address is restricted to the local machine.
    pub fn binds_loopback(&self) -> bool {
        is_loopback_host(self.host.as_str())
    }

    /// Reject a configuration that would expose an unauthenticated API or
    /// send credentials to a malformed configured endpoint.
    pub fn validate_security(&self) -> anyhow::Result<()> {
        if !self.binds_loopback()
            && self.api_token.as_deref().is_none_or(str::is_empty)
            && !self.allow_unauthenticated_nonloopback
        {
            anyhow::bail!(
                "refusing to bind {} without TACK_API_TOKEN; bind to loopback, set TACK_API_TOKEN, or set TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK=1 to accept the risk",
                self.host
            );
        }
        for origin in &self.allowed_origins {
            validate_origin(origin)?;
        }
        validate_outbound_url("github_api_base", &self.github_api_base)?;
        if let Some(url) = &self.webhook_url {
            validate_outbound_url("webhook_url", url)?;
        }
        if let Some(url) = &self.backup_endpoint {
            validate_outbound_url("backup_endpoint", url)?;
        }
        Ok(())
    }

    /// Returns true when all three required remote-backup fields are set.
    pub fn remote_backup_enabled(&self) -> bool {
        self.backup_bucket.is_some()
            && self.backup_access_key.is_some()
            && self.backup_secret_key.is_some()
    }

    /// Extract the filesystem path from the database URL.
    /// Returns `None` for in-memory databases.
    pub fn db_file_path(&self) -> Option<std::path::PathBuf> {
        let url = &self.database_url;
        if url.contains(":memory:") {
            return None;
        }
        let rest = url.strip_prefix("sqlite:")?;
        let rest = rest.split('?').next().unwrap_or(rest);
        if rest.is_empty() {
            return None;
        }
        Some(std::path::PathBuf::from(rest))
    }

    /// Load config from file, falling back to defaults.
    pub fn load() -> Self {
        let mut config = if let Ok(content) = std::fs::read_to_string("tack.toml")
            && let Ok(config) = toml::from_str(&content)
        {
            config
        } else {
            Self::default()
        };
        if let Ok(v) = std::env::var("TACK_HOST") {
            config.host = v;
        }
        if let Ok(v) = std::env::var("TACK_PORT") {
            config.port = v.parse().unwrap_or(3210);
        }
        if let Ok(v) = std::env::var("TACK_DATABASE_URL") {
            config.database_url = v;
        }
        if let Ok(v) = std::env::var("TACK_LOG_LEVEL") {
            config.log_level = v;
        }
        if let Ok(v) = std::env::var("TACK_LOG_JSON") {
            config.log_json = v == "true" || v == "1";
        }
        if let Ok(v) = std::env::var("TACK_LOG_FILE") {
            config.log_file = Some(v);
        }
        if let Ok(v) = std::env::var("TACK_STORAGE_DIR") {
            config.storage_dir = v;
        }
        if let Ok(v) = std::env::var("TACK_ALLOWED_ORIGINS") {
            config.allowed_origins = v.split(',').map(|s| s.trim().to_string()).collect();
        }
        if let Ok(v) = std::env::var("TACK_MAX_BODY_SIZE") {
            config.max_body_size_bytes = v.parse().unwrap_or(default_max_body_size_bytes());
        }
        // Never log the token value
        if let Ok(v) = std::env::var("TACK_API_TOKEN")
            && !v.is_empty()
        {
            config.api_token = Some(v);
        }
        if let Ok(v) = std::env::var("TACK_WEBHOOK_URL")
            && !v.is_empty()
        {
            config.webhook_url = Some(v);
        }
        if let Ok(v) = std::env::var("TACK_WEBHOOK_SECRET")
            && !v.is_empty()
        {
            config.webhook_secret = Some(v);
        }
        if let Ok(v) = std::env::var("TACK_GITHUB_TOKEN")
            && !v.is_empty()
        {
            config.github_token = Some(v);
        }
        if let Ok(v) = std::env::var("TACK_GITHUB_API_BASE")
            && !v.is_empty()
        {
            config.github_api_base = v;
        }
        if let Ok(v) = std::env::var("TACK_BACKUP_ENDPOINT")
            && !v.is_empty()
        {
            config.backup_endpoint = Some(v);
        }
        if let Ok(v) = std::env::var("TACK_BACKUP_BUCKET")
            && !v.is_empty()
        {
            config.backup_bucket = Some(v);
        }
        if let Ok(v) = std::env::var("TACK_BACKUP_REGION")
            && !v.is_empty()
        {
            config.backup_region = v;
        }
        if let Ok(v) = std::env::var("TACK_BACKUP_ACCESS_KEY")
            && !v.is_empty()
        {
            config.backup_access_key = Some(v);
        }
        // Never log the secret key value
        if let Ok(v) = std::env::var("TACK_BACKUP_SECRET_KEY")
            && !v.is_empty()
        {
            config.backup_secret_key = Some(v);
        }
        if let Ok(v) = std::env::var("TACK_BACKUP_PREFIX")
            && !v.is_empty()
        {
            config.backup_prefix = v;
        }
        if let Ok(v) = std::env::var("TACK_BACKUP_INTERVAL_SECS") {
            config.backup_interval_secs = v.parse().ok();
        }
        if let Ok(v) = std::env::var("TACK_BACKUP_RETENTION") {
            config.backup_retention = v.parse().unwrap_or(default_backup_retention());
        }
        if let Ok(v) = std::env::var("TACK_ORCH_ENABLE") {
            config.orch_enable = v == "1" || v.eq_ignore_ascii_case("true");
        }
        if let Ok(v) = std::env::var("TACK_LOCAL_RUNNER_ENABLE") {
            config.local_runner_enable = v == "1" || v.eq_ignore_ascii_case("true");
        }
        if let Ok(v) = std::env::var("TACK_ORCH_POLL_SECS") {
            config.orch_poll_secs = v.parse().unwrap_or(default_orch_poll_secs());
        }
        if let Ok(v) = std::env::var("TACK_ORCH_EVENT_RETENTION_DAYS") {
            config.orch_event_retention_days =
                v.parse().unwrap_or(default_orch_event_retention_days());
        }
        // Never log the approval-token value
        if let Ok(v) = std::env::var("TACK_ORCH_APPROVAL_TOKEN")
            && !v.is_empty()
        {
            config.orch_approval_token = Some(v);
        }
        if let Ok(v) = std::env::var("TACK_EXECUTION_RETENTION_ENABLE") {
            config.execution_retention_enable = v == "1" || v.eq_ignore_ascii_case("true");
        }
        if let Ok(v) = std::env::var("TACK_EXECUTION_RETENTION_DAYS") {
            config.execution_retention_days =
                v.parse().unwrap_or(default_execution_retention_days());
        }
        if let Ok(v) = std::env::var("TACK_EXECUTION_RETENTION_INTERVAL_SECS") {
            config.execution_retention_interval_secs = v
                .parse()
                .unwrap_or(default_execution_retention_interval_secs());
        }
        if let Ok(v) = std::env::var("TACK_EXECUTION_HEALTH_ENABLE") {
            config.execution_health_enable = v == "1" || v.eq_ignore_ascii_case("true");
        }
        if let Ok(v) = std::env::var("TACK_EXECUTION_HEALTH_INTERVAL_SECS") {
            config.execution_health_interval_secs = v
                .parse()
                .unwrap_or(default_execution_health_interval_secs());
        }
        // Never log the decision-token value — same posture as the
        // approval-token parse above.
        if let Ok(v) = std::env::var("TACK_EXECUTION_DECISION_TOKEN")
            && !v.is_empty()
        {
            config.execution_decision_token = Some(v);
        }
        if let Ok(v) = std::env::var("TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK") {
            config.allow_unauthenticated_nonloopback = v == "1" || v.eq_ignore_ascii_case("true");
        }
        config
    }
}

fn validate_origin(origin: &str) -> anyhow::Result<()> {
    let parsed = reqwest::Url::parse(origin)
        .map_err(|_| anyhow::anyhow!("allowed origin must be an absolute http(s) origin"))?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.path() != "/"
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        anyhow::bail!("allowed origin must be a bare http(s) origin: {origin}");
    }
    Ok(())
}

fn validate_outbound_url(name: &str, value: &str) -> anyhow::Result<()> {
    let parsed = reqwest::Url::parse(value)
        .map_err(|_| anyhow::anyhow!("{name} must be an absolute http(s) URL"))?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        anyhow::bail!("{name} must be a credential-free http(s) URL without query or fragment");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsafe_non_loopback_startup_is_rejected() {
        let config = AppConfig {
            host: "0.0.0.0".into(),
            ..AppConfig::default()
        };
        let err = config
            .validate_security()
            .expect_err("non-loopback bind with no token and no opt-out must fail to start");
        let message = err.to_string();
        assert!(message.contains("TACK_API_TOKEN"));
        assert!(message.contains("TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK"));
    }

    #[test]
    fn unsafe_non_loopback_startup_with_a_token_is_accepted() {
        let config = AppConfig {
            host: "0.0.0.0".into(),
            api_token: Some("a-real-token".into()),
            ..AppConfig::default()
        };
        assert!(config.validate_security().is_ok());
    }

    #[test]
    fn unsafe_non_loopback_startup_with_the_documented_opt_out_is_accepted() {
        let config = AppConfig {
            host: "0.0.0.0".into(),
            allow_unauthenticated_nonloopback: true,
            ..AppConfig::default()
        };
        assert!(config.validate_security().is_ok());
    }

    #[test]
    fn loopback_startup_with_no_token_is_unaffected_by_the_opt_out_flag() {
        // The opt-out only ever relaxes the non-loopback branch; it must never
        // become a second way to fail a loopback bind that today always
        // succeeds — pure-local mode is the design's baseline case, not one
        // this flag should be able to touch either way.
        let config = AppConfig::default();
        assert!(config.binds_loopback());
        assert!(config.validate_security().is_ok());

        let config_with_flag_set = AppConfig {
            allow_unauthenticated_nonloopback: true,
            ..AppConfig::default()
        };
        assert!(config_with_flag_set.validate_security().is_ok());
    }

    #[test]
    fn a_loopback_host_is_an_address_or_localhost_never_a_name_that_starts_like_one() {
        for host in ["127.0.0.1", "127.9.9.9", "::1", "[::1]", "::ffff:127.0.0.1", "localhost", "LOCALHOST"] {
            assert!(is_loopback_host(host), "{host} names this machine");
        }
        for host in ["127.attacker.example", "127", "localhost.attacker.example", "10.0.0.1", "::2", ""] {
            assert!(!is_loopback_host(host), "{host} must not pass as loopback");
        }
    }

    #[test]
    fn origin_validation_is_exact_and_rejects_lookalike_shapes() {
        assert!(validate_origin("https://tack.example.test").is_ok());
        assert!(validate_origin("https://tack.example.test/path").is_err());
        assert!(validate_origin("https://user:t@tack.example.test").is_err());
    }
}
