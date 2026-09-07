# Administration and Security

Tack has no identity model: there are no user accounts, no sessions, and no per-user permissions. `assignee` is a free-text label on an item, not an account — anyone can type any name into it. Every request that authenticates at all authenticates as the same single operator, via one shared `TACK_API_TOKEN`. Tack is built for one operator (or a small team willing to share that one secret), not for telling users apart; see [ADR 0059](https://github.com/yielab/tack/blob/develop/docs/adr/0059-single-operator-identity-posture.md) for the reasoning and what was deliberately left out.

Tack is local-first by default: it binds to `127.0.0.1`, requires no authentication, and stores everything in a single SQLite file. This page covers the configuration you apply when you move beyond a single-machine setup — locking down the API, controlling network exposure, enabling cloud backups, wiring up webhooks, and tuning logs. Every setting below is read from `tack.toml` or environment variables at startup; see [Configuration](configuration.md) for how those are loaded.

All examples assume the default base URL `http://127.0.0.1:3210`.

---

## Authentication

By default Tack accepts every request — appropriate for a pure-local install. To require a token, set `TACK_API_TOKEN` and restart the server. Once set, **every** `/api/*` route requires an `Authorization: Bearer <token>` header. One endpoint is exempt:

- `GET /api/health` — liveness/readiness probe, always open.

Start the server with a token:

```sh
TACK_API_TOKEN='a-long-random-secret' tack serve
```

Requests without (or with a wrong) token receive `401 Unauthorized`. Supply the token on every call:

```sh
# Rejected — no token
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:3210/api/projects
# → 401

# Accepted
curl -s http://127.0.0.1:3210/api/projects \
  -H 'Authorization: Bearer a-long-random-secret'
```

The token value is never written to logs. Use a long, random string and rotate it by restarting with a new value.

---

## CORS

Browsers block cross-origin API calls unless the server explicitly allows the page's origin. Tack's allow-list is `TACK_ALLOWED_ORIGINS`, a comma-separated list of exact origins (scheme + host + port). The default is:

```
http://localhost:8080,http://127.0.0.1:8080,http://localhost:3210,http://127.0.0.1:3210,https://tack.test
```

Change it when the browser loads the UI from a different origin than the API — for example a reverse-proxy hostname or a separate frontend dev server:

```sh
TACK_ALLOWED_ORIGINS='https://tack.example.com,https://app.example.com' tack serve
```

List every origin you serve the UI from; entries are matched exactly, with no wildcards. The bundled SPA served by the same process needs no extra entry.

---

## Network exposure and TLS

Tack binds to `TACK_HOST` (default `127.0.0.1`) on `TACK_PORT` (default `3210`), so out of the box it is reachable only from the local machine. Because Tack has no per-user accounts (see above), a bind reachable from beyond the local machine with no `TACK_API_TOKEN` configured hands full read/write access — the board, and the runner-scheduling surface — to anyone who can reach the port. Tack refuses to start in that configuration:

```sh
TACK_HOST=0.0.0.0 TACK_PORT=3210 tack serve
# Error: refusing to bind 0.0.0.0 without TACK_API_TOKEN; bind to loopback,
# set TACK_API_TOKEN, or set TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK=1
# to accept the risk
```

To serve it on a LAN, set a token alongside the routable bind:

```sh
TACK_HOST=0.0.0.0 TACK_PORT=3210 TACK_API_TOKEN='a-long-random-secret' tack serve
```

If a token genuinely cannot be configured — for example a container reachable only on a
network you already trust — set `TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK=1` to
start anyway. This is an explicit acceptance of the risk above, not a default; leave it
unset unless you have a specific reason to widen the bind without a credential.

Tack does not terminate TLS itself. For any non-localhost deployment, place it behind a reverse proxy (Caddy, nginx, Traefik) that handles HTTPS and forwards to the local port. Keep `TACK_HOST=127.0.0.1` and let only the proxy reach it. See [Deployment](../developer/deployment.md) for full proxy and TLS setup.

---

## Request limits

Non-attachment requests are capped by `TACK_MAX_BODY_SIZE` (bytes, default `2097152` = 2 MB). This protects the JSON API from oversized payloads:

```sh
TACK_MAX_BODY_SIZE=5242880 tack serve   # raise to 5 MB
```

The file-upload endpoint (`POST /api/items/{id}/attachments`) is exempt from this limit and is always capped at 50 MB, regardless of `TACK_MAX_BODY_SIZE`.

---

## Cloud backup (S3-compatible)

Tack can push database snapshots to any S3-compatible object store — AWS S3, Cloudflare R2, Backblaze B2, or MinIO. Remote backup is **enabled only when a bucket, an access key, and a secret key are all present**. For local snapshot/restore, see [Backup and Restore](backup-restore.md).

### Configuration sources

Two layers feed the effective config:

1. **Environment defaults** (`TACK_BACKUP_*`), applied at startup.
2. **UI overrides** (Settings → Cloud Backup), persisted in the `app_meta` table.

The UI values override the environment for these fields: endpoint, bucket, region, access key, secret key, prefix, retention. A blank UI field clears the override and falls back to the environment default. The one exception is the auto-backup **interval**, which is environment-only (`TACK_BACKUP_INTERVAL_SECS`) and applied at startup — it is not editable from the UI.

| Variable | Default | Purpose |
|----------|---------|---------|
| `TACK_BACKUP_ENDPOINT` | _(none)_ | S3-compatible endpoint URL. Omit for AWS S3; set for R2/B2/MinIO (e.g. `https://<account>.r2.cloudflarestorage.com`) |
| `TACK_BACKUP_BUCKET` | _(none)_ | Bucket name. **Required** to enable remote backup |
| `TACK_BACKUP_REGION` | `auto` | Region. AWS needs the real region; Cloudflare R2 uses `auto` |
| `TACK_BACKUP_ACCESS_KEY` | _(none)_ | S3 access key ID. **Required** to enable remote backup |
| `TACK_BACKUP_SECRET_KEY` | _(none)_ | S3 secret access key. **Required**; never logged |
| `TACK_BACKUP_PREFIX` | `tack` | Object key prefix inside the bucket |
| `TACK_BACKUP_INTERVAL_SECS` | _(none)_ | Auto-backup interval in seconds; omit for manual-only. Env-only, applied at startup |
| `TACK_BACKUP_RETENTION` | `10` | Number of remote backups to keep after each upload |

Example (Cloudflare R2):

```sh
TACK_BACKUP_ENDPOINT='https://<account>.r2.cloudflarestorage.com' \
TACK_BACKUP_BUCKET='tack-backups' \
TACK_BACKUP_REGION='auto' \
TACK_BACKUP_ACCESS_KEY='...' \
TACK_BACKUP_SECRET_KEY='...' \
TACK_BACKUP_INTERVAL_SECS=86400 \
tack serve
```

### Reading and writing settings via the API

`GET /api/settings/backup` returns the effective config. The secret key is never sent to clients — it is replaced by a boolean `secret_key_set`:

```sh
curl http://127.0.0.1:3210/api/settings/backup
```

```json
{
  "configured": true,
  "endpoint": "https://<account>.r2.cloudflarestorage.com",
  "bucket": "tack-backups",
  "region": "auto",
  "access_key": "...",
  "secret_key_set": true,
  "prefix": "tack",
  "retention": 10
}
```

`PUT /api/settings/backup` saves overrides. Sending a blank `secret_key` keeps the stored secret (so the masked UI field can be left untouched); any other blank string field clears that override and reverts to the environment default.

### Manual backup endpoints

When remote backup is configured, these endpoints operate on demand. If it is not configured they return `409 Conflict`.

| Method & path | Action |
|---------------|--------|
| `POST /api/backup/remote` | Create a bundle and upload it; prunes to `retention` afterward |
| `GET /api/backup/remote` | List remote backups, newest first |
| `POST /api/backup/remote/restore` | Download a bundle and **stage** it for the next restart |

Restore is staged, not live — restart the server to apply it. Omit the body (or `key`) to restore the latest backup, or target a specific object:

```sh
# Upload now
curl -X POST http://127.0.0.1:3210/api/backup/remote

# Restore a specific object (then restart the server)
curl -X POST http://127.0.0.1:3210/api/backup/remote/restore \
  -H 'Content-Type: application/json' \
  -d '{"key":"tack/2026-06-26T12-00-00Z.tackbundle"}'
```

---

## Webhooks

Set `TACK_WEBHOOK_URL` to receive an HTTP `POST` whenever work changes. Delivery is fire-and-forget: each event is sent on a background task with a 10-second timeout, and failures are logged but never block the originating request.

```sh
TACK_WEBHOOK_URL='https://hooks.example.com/tack' tack serve
```

### Event types

The event name is sent both as the `X-Tack-Event` request header and as the `event` field in the JSON body.

| Event | When it fires |
|-------|---------------|
| `item.created` | An item is created |
| `item.updated` | An item is updated (including status changes) |
| `item.deleted` | An item is deleted |
| `sprint.started` | A sprint transitions to **Active** |
| `sprint.completed` | A sprint transitions to **Closed** |
| `sprint.updated` | Any other sprint status change |
| `item.due_soon` | An item is due within the next hour (background check runs hourly) |

### Payload shapes

Every payload carries `event`, an RFC 3339 `timestamp`, and `project_id`. The remaining fields depend on the event:

```json
// item.created / item.updated / item.due_soon
{
  "event": "item.updated",
  "timestamp": "2026-06-26T12:00:00+00:00",
  "project_id": "1f0c…",
  "item": { /* full item object */ }
}
```

```json
// item.deleted — carries the id only, since the item is gone
{
  "event": "item.deleted",
  "timestamp": "2026-06-26T12:00:00+00:00",
  "project_id": "1f0c…",
  "item_id": "9ab3…"
}
```

```json
// sprint.started / sprint.completed / sprint.updated
{
  "event": "sprint.started",
  "timestamp": "2026-06-26T12:00:00+00:00",
  "project_id": "1f0c…",
  "sprint_id": "44de…",
  "sprint_name": "Sprint 7",
  "status": "active"
}
```

### Signing

Set `TACK_WEBHOOK_SECRET` to sign every delivery. Tack computes an HMAC-SHA256 over the exact request body and sends it as:

```
X-Tack-Signature: sha256=<hex>
```

Verify it on the receiver by recomputing the HMAC of the raw body with the same secret and comparing (constant-time) against the header value. Reject any request whose signature does not match.

```sh
TACK_WEBHOOK_URL='https://hooks.example.com/tack' \
TACK_WEBHOOK_SECRET='shared-signing-secret' \
tack serve
```

---

## Logging

Logging is controlled by three variables. Secrets — the API token, the webhook secret, the backup secret key, and the GitHub token — are never written to logs at any level.

| Variable | Default | Purpose |
|----------|---------|---------|
| `TACK_LOG_LEVEL` | `info` | Verbosity: `trace`, `debug`, `info`, `warn`, `error` |
| `TACK_LOG_JSON` | `false` | Emit structured JSON lines (for log aggregators) when `true`/`1` |
| `TACK_LOG_FILE` | _(none)_ | Write logs to this file path instead of (or in addition to) stderr |

```sh
TACK_LOG_LEVEL=debug TACK_LOG_JSON=true TACK_LOG_FILE=/var/log/tack.log tack serve
```

---

## Environment variable reference

Security- and administration-relevant settings, as read by the server at startup. Values can also be set in `tack.toml`; see [Configuration](configuration.md).

| Variable | Default | Purpose |
|----------|---------|---------|
| `TACK_HOST` | `127.0.0.1` | Bind address. Set to `0.0.0.0` to expose on a LAN (front with a TLS proxy). Requires `TACK_API_TOKEN` (or the opt-out below) once set to anything non-loopback — see [Network exposure and TLS](#network-exposure-and-tls) |
| `TACK_PORT` | `3210` | Listen port |
| `TACK_DATABASE_URL` | `sqlite:tack.db?mode=rwc` | SQLite database location |
| `TACK_API_TOKEN` | _(none)_ | When set, requires `Authorization: Bearer <token>` on all `/api/*` routes except `/api/health`. Never logged |
| `TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK` | `false` | Explicit opt-out for the non-loopback-without-token startup refusal (see [ADR 0059](https://github.com/yielab/tack/blob/develop/docs/adr/0059-single-operator-identity-posture.md)). Off by default — set only when a `TACK_HOST` reachable beyond the local machine is intentional and a token genuinely cannot be configured |
| `TACK_ALLOWED_ORIGINS` | see [`docs/CONFIG.md`](https://github.com/yielab/tack/blob/develop/docs/CONFIG.md) | Comma-separated CORS allow-list of exact origins |
| `TACK_MAX_BODY_SIZE` | `2097152` | Max body size in bytes for non-attachment requests (2 MB). Uploads are always capped at 50 MB |
| `TACK_STORAGE_DIR` | `./storage` | Attachment storage directory |
| `TACK_WEBHOOK_URL` | _(none)_ | Outbound webhook URL; enables event POSTs |
| `TACK_WEBHOOK_SECRET` | _(none)_ | HMAC-SHA256 signing secret; adds `X-Tack-Signature: sha256=<hex>`. Never logged |
| `TACK_GITHUB_TOKEN` | _(none)_ | GitHub PAT (`repo` scope) for issue push-back. Never logged |
| `TACK_GITHUB_API_BASE` | `https://api.github.com` | GitHub API root (override for GitHub Enterprise) |
| `TACK_BACKUP_ENDPOINT` | _(none)_ | S3-compatible endpoint URL; omit for AWS S3 |
| `TACK_BACKUP_BUCKET` | _(none)_ | Bucket name — required to enable remote backup |
| `TACK_BACKUP_REGION` | `auto` | S3 region (`auto` for Cloudflare R2) |
| `TACK_BACKUP_ACCESS_KEY` | _(none)_ | S3 access key ID — required to enable remote backup |
| `TACK_BACKUP_SECRET_KEY` | _(none)_ | S3 secret access key — required; never logged |
| `TACK_BACKUP_PREFIX` | `tack` | Object key prefix inside the bucket |
| `TACK_BACKUP_INTERVAL_SECS` | _(none)_ | Auto-backup interval in seconds; env-only, applied at startup |
| `TACK_BACKUP_RETENTION` | `10` | Remote backups to retain after each upload |
| `TACK_LOG_LEVEL` | `info` | Log verbosity |
| `TACK_LOG_JSON` | `false` | Structured JSON logging |
| `TACK_LOG_FILE` | _(none)_ | Optional log file path |

For the workflow-facing side of these features, see [Backup and Restore](backup-restore.md); for proxy and TLS setup, see [Deployment](../developer/deployment.md).
