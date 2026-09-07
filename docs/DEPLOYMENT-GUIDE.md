# Tack Deployment Guide

This guide covers deploying Tack to production.

Tack is a **single, self-contained binary** (about 21 MiB — see [Benchmarks](BENCHMARKS.md)) with the SolidJS SPA
embedded. One process serves the REST API (`/api/*`), the WebSocket, and the web
UI — same-origin, so there is no separate frontend service, no CORS to configure
for the bundled UI, and no static host to run. All state lives in one SQLite
file plus an attachments directory. Deployment is therefore "run one binary
behind a reverse proxy."

---

## Table of Contents

1. [Get the binary](#get-the-binary)
2. [Run it](#run-it)
3. [Systemd service (recommended)](#systemd-service-recommended)
4. [Per-user service (no root)](#per-user-service-no-root)
5. [Reverse proxy + HTTPS (Caddy)](#reverse-proxy--https-caddy)
6. [Reverse proxy (nginx)](#reverse-proxy-nginx)
7. [Docker](#docker)
8. [Environment configuration](#environment-configuration)
9. [Backups](#backups)
10. [Monitoring & logging](#monitoring--logging)
11. [Security checklist](#security-checklist)
12. [Scaling considerations](#scaling-considerations)
13. [Troubleshooting](#troubleshooting)
14. [Maintenance](#maintenance)

---

## Get the binary

### Option A — download a release

Prebuilt binaries for Linux (x86_64), macOS (Intel + Apple Silicon), and Windows
are attached to each [GitHub Release](https://github.com/yielab/tack/releases).
Each archive contains the single `tack` executable, `LICENSE`, `README.md`, and a
`QUICKSTART.txt`. From v0.1.0-beta.7 on, releases also ship a `SHA256SUMS` file,
build provenance attestations, and an SBOM — verify before deploying:

```bash
sha256sum -c SHA256SUMS            # checksums
gh attestation verify tack --repo yielab/tack   # provenance (optional)
```

### Option B — one-line installer

```bash
curl -fsSL https://raw.githubusercontent.com/yielab/tack/main/install.sh | sh
```

Resolves the newest release asset for your platform and installs `tack`.

### Option C — build from source

The SPA must be built first so `--features embed-spa` can embed it. The Makefile
does both steps:

```bash
git clone https://github.com/yielab/tack.git
cd tack
make build
# → npm --prefix frontend ci && npm --prefix frontend run build
#   cargo build -p tack-cli --release --features embed-spa
# Produces target/release/tack  (SPA embedded)
```

For a fully static Linux binary (no glibc dependency — ideal for minimal hosts
and containers):

```bash
rustup target add x86_64-unknown-linux-musl
sudo apt-get install -y musl-tools
npm --prefix frontend ci && npm --prefix frontend run build
cargo build --release --target x86_64-unknown-linux-musl -p tack-cli --features embed-spa
# → target/x86_64-unknown-linux-musl/release/tack
```

---

## Run it

```bash
# Bare `tack` (or `tack serve`) starts the server + web UI.
./tack

# Open http://127.0.0.1:3210 — the SPA loads and talks to /api same-origin.
```

By default Tack binds `127.0.0.1:3210`, writes `tack.db` in the current directory,
and stores attachments in `./storage`. Point those anywhere with env vars:

```bash
TACK_HOST=127.0.0.1 \
TACK_PORT=3210 \
TACK_DATABASE_URL="sqlite:/var/lib/tack/tack.db?mode=rwc" \
TACK_STORAGE_DIR="/var/lib/tack/storage" \
  ./tack
```

The same binary is also the CLI client (`./tack --help`, `./tack add`,
`./tack list`, …) — it talks to a running server over HTTP, never the DB directly.

> **Security note:** bind to `127.0.0.1` and put a reverse proxy in front. If you
> must bind a non-loopback address (`TACK_HOST=0.0.0.0`), set `TACK_API_TOKEN` —
> otherwise the API (including the "download the whole database" endpoint) is open
> to anyone who can reach the port. In this exact configuration (non-loopback bind
> **and** no `TACK_API_TOKEN`), the server refuses to start: its security preflight
> rejects the configuration before any network resources open. Set
> `TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK=1` only to accept that risk
> deliberately (e.g. behind a trusted authenticating proxy). See `docs/CONFIG.md`
> for the full `TACK_*` variable table.

---

## Systemd service (recommended)

For a native Linux host, run Tack as a hardened systemd unit bound to loopback,
with a reverse proxy terminating TLS.

```bash
# 1. Install the binary and create a service user + data dirs
sudo install -m 0755 tack /usr/local/bin/tack
sudo useradd --system --home /var/lib/tack --shell /usr/sbin/nologin tack
sudo mkdir -p /var/lib/tack/storage
sudo chown -R tack:tack /var/lib/tack

# 2. Write the unit
sudo tee /etc/systemd/system/tack.service >/dev/null <<'EOF'
[Unit]
Description=Tack project management
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=tack
Group=tack
WorkingDirectory=/var/lib/tack
Environment=TACK_HOST=127.0.0.1
Environment=TACK_PORT=3210
Environment=TACK_DATABASE_URL=sqlite:/var/lib/tack/tack.db?mode=rwc
Environment=TACK_STORAGE_DIR=/var/lib/tack/storage
Environment=TACK_LOG_LEVEL=info
Environment=TACK_LOG_JSON=true
# Uncomment to require a bearer token on every API request:
# Environment=TACK_API_TOKEN=change-me-to-a-long-random-secret
ExecStart=/usr/local/bin/tack serve
Restart=always
RestartSec=5

# Hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
ReadWritePaths=/var/lib/tack

[Install]
WantedBy=multi-user.target
EOF

# 3. Enable and start
sudo systemctl daemon-reload
sudo systemctl enable --now tack
sudo systemctl status tack
```

Logs go to the journal: `sudo journalctl -u tack -f`.

---

## Per-user service (no root)

For a personal machine — no dedicated service account, no `sudo` — `tack service`
installs the same idea as a **user** unit instead of a system one: a systemd user unit
on Linux (`~/.config/systemd/user/tack.service`, `systemctl --user`), a launchd agent on
macOS. It uses this OS's own per-user application-data folder for the database, storage,
runner state, and log file, so nothing is written next to wherever the command was run.
Not supported on Windows; install the desktop app there instead.

```sh
tack service install     # writes the unit, then `systemctl --user enable --now tack`
tack service status       # prints the unit's state and the health URL
tack service uninstall    # stops and removes the unit; the data root is left untouched
```

See [`tack service`](book/src/user-guide/cli.md#service) for real output from all three
commands. Prefer the system-level unit above for a shared or internet-facing deployment —
this one runs as your own user and stops when your user session's systemd instance does
(`loginctl enable-linger` keeps it running across logouts).

---

## Reverse proxy + HTTPS (Caddy)

Caddy terminates TLS (automatic Let's Encrypt), forwards everything to the single
Tack process, and transparently upgrades the WebSocket — no special block needed
in modern Caddy, `reverse_proxy` handles the upgrade automatically.

```caddyfile
tack.example.com {
    encode gzip

    # One upstream serves the API, the WebSocket, and the SPA.
    reverse_proxy 127.0.0.1:3210

    header {
        Strict-Transport-Security "max-age=31536000; includeSubDomains; preload"
        X-Content-Type-Options    "nosniff"
        X-Frame-Options           "SAMEORIGIN"
        Referrer-Policy           "strict-origin-when-cross-origin"
    }

    log {
        output file /var/log/caddy/tack.log
    }
}
```

Reload Caddy after editing (`sudo systemctl reload caddy`). This project's own
local dev setup uses exactly this pattern — a single upstream behind a
systemd-managed Caddy (see `Caddyfile.local` and `/home/ox/Sites/LOCAL-DOMAINS.md`).

---

## Reverse proxy (nginx)

```nginx
server {
    listen 80;
    server_name tack.example.com;

    location / {
        proxy_pass http://127.0.0.1:3210;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # WebSocket upgrade (board live-updates)
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_read_timeout 86400;
    }
}
```

Add TLS with Certbot: `sudo certbot --nginx -d tack.example.com`.

---

## Docker

A `Dockerfile` and `docker-compose.yml` ship at the repo root. The image is a
three-stage build (build SPA → compile a static musl binary that embeds it → copy
onto a distroless base) producing a shell-less image barely larger than the binary. There is a single
service and a single volume for the database and attachments.

```bash
# Build and run with compose
docker compose up -d
curl http://localhost:3210/api/health
docker compose logs -f

# Or plain docker
docker build -t tack:latest .
docker run -d --name tack -p 3210:3210 -v tack-data:/data tack:latest
```

The container binds `0.0.0.0` internally and stores everything under `/data`
(`/data/tack.db` + `/data/storage`), persisted by the named volume. If you
publish the port beyond localhost, set `TACK_API_TOKEN` in `docker-compose.yml`
(a commented example is included there). The distroless image has no shell, so
health-check the container from the host (`curl .../api/health`) or via a proxy.

---

## Environment configuration

Tack reads config from `tack.toml` (if present) or environment variables. The
full table lives in [CLAUDE.md](../CLAUDE.md) and the
[API reference](./API-REFERENCE.md); the deployment-relevant ones:

```bash
# Server
TACK_HOST=127.0.0.1                                  # bind address (loopback by default)
TACK_PORT=3210
TACK_DATABASE_URL=sqlite:/var/lib/tack/tack.db?mode=rwc
TACK_STORAGE_DIR=/var/lib/tack/storage               # attachments

# Auth & CORS
TACK_API_TOKEN=<long-random-secret>                  # require Authorization: Bearer <token>
TACK_ALLOWED_ORIGINS=https://tack.example.com        # comma-separated CORS allow-list

# Logging
TACK_LOG_LEVEL=info                                  # trace|debug|info|warn|error
TACK_LOG_JSON=true                                   # structured logs for aggregators
TACK_LOG_FILE=/var/log/tack/tack.log                 # optional file sink

# Body limits
TACK_MAX_BODY_SIZE=2097152                           # 2 MB default (upload endpoint is always 50 MB)
```

`TACK_ALLOWED_ORIGINS` is only relevant if you point a *separate-origin* browser
client at the API. The bundled SPA is same-origin and needs no CORS config. There
is **no** `TACK_CORS_ORIGIN` variable — the allow-list is `TACK_ALLOWED_ORIGINS`.

Optional integrations (outbound webhooks, GitHub sync, and S3-compatible
cloud backup) are configured with their own `TACK_*` variables — see CLAUDE.md.

### Configuration file (tack.toml)

```toml
host = "127.0.0.1"
port = 3210
database_url = "sqlite:/var/lib/tack/tack.db?mode=rwc"
storage_dir = "/var/lib/tack/storage"
log_level = "info"
log_json = true
log_file = "/var/log/tack/tack.log"
allowed_origins = "https://tack.example.com"
```

---

## Backups

Tack has **built-in** backup/restore — you do not need to reach into SQLite
manually.

### Built-in local backup

```bash
# Download a consistent snapshot (VACUUM INTO) over the API
curl -s http://127.0.0.1:3210/api/backup -o tack-backup.db

# Or via the CLI
tack backup > tack-backup.db          # writes a snapshot
tack restore tack-backup.db           # stages a restore (applied on next restart)
```

Restore is **staged**: the uploaded DB is written next to the live one and swapped
in atomically on the next server start. Restart the service after restoring.

### File-level backup (systemd host)

```bash
# Stop-free snapshot of the live DB (WAL-safe)
sqlite3 /var/lib/tack/tack.db ".backup /var/backups/tack/tack-$(date +%F).db"
# Attachments live on disk, back them up too:
tar czf /var/backups/tack/storage-$(date +%F).tar.gz -C /var/lib/tack storage
```

Automate with cron or a systemd timer, and prune old files with
`find /var/backups/tack -mtime +30 -delete`.

### Cloud backup (S3-compatible)

Tack can back up the **database plus attachments** as one `.tar.zst` bundle to any
S3-compatible store (Cloudflare R2, Backblaze B2, AWS S3, self-hosted MinIO). Set
the `TACK_BACKUP_*` variables (see CLAUDE.md) or configure it at runtime under
**Settings → Cloud Backup**, then:

```bash
tack backup --remote        # upload a bundle now
tack backups                # list remote bundles (newest first)
tack restore --remote       # stage the latest remote bundle, then restart
```

Set `TACK_BACKUP_INTERVAL_SECS` to schedule automatic uploads. The secret key is
never logged and is write-only over the API.

Bundles carry a generation counter and an install ID: an upload that would
overwrite newer work from a **different** install is rejected (pass `force` to
override), and a restore that would clobber newer local work requires
confirmation. Restores verify the bundle's SHA-256 and schema version before
staging, snapshot the current state first, and roll back if the swap fails.

> **Encryption at rest is not implemented.** Bundles are compressed but
> unencrypted in the bucket. Use a private bucket with encryption-at-rest enabled on
> the provider side, and scope the access key to that one bucket.

---

## Monitoring & logging

### Health check

```bash
curl http://127.0.0.1:3210/api/health
# {"status":"ok","version":"0.1.0-beta.7","migrations_applied":18}
```

Point uptime monitoring (Uptime Kuma, Healthchecks.io, a load-balancer probe) at
`/api/health` and alert on non-200 or a stalled `migrations_applied` count.

### Logs

```bash
sudo journalctl -u tack -f                       # systemd
sudo journalctl -u tack --since "24 hours ago"   # export a window
docker compose logs -f                            # Docker
```

Set `TACK_LOG_JSON=true` for structured logs an aggregator can parse.

### Debug endpoints

`/api/debug/info` and `/api/debug/db-stats` return build/config and per-table row
counts. They sit behind the same bearer-token gate as the rest of the API when
`TACK_API_TOKEN` is set — keep the token set on any exposed instance.

---

## Security checklist

- [ ] Bind `TACK_HOST=127.0.0.1`; expose only through the reverse proxy.
- [ ] If binding non-loopback, set a long random `TACK_API_TOKEN`.
- [ ] Terminate TLS at Caddy/nginx (HSTS enabled).
- [ ] Restrict `TACK_ALLOWED_ORIGINS` if a separate-origin client uses the API.
- [ ] Run under a dedicated, unprivileged service user (the systemd unit above).
- [ ] Firewall the raw app port so only the proxy can reach it.
- [ ] Automate backups and test a restore.
- [ ] Add rate limiting at the proxy if the instance is public.
- [ ] Keep `TACK_LOG_LEVEL=info` or `warn` in production; `TACK_LOG_JSON=true`.

### Firewall (UFW)

```bash
sudo ufw allow 80/tcp
sudo ufw allow 443/tcp
sudo ufw deny  3210/tcp    # block direct app access; only the proxy reaches it
sudo ufw enable
```

### Rate limiting (Caddy)

```caddyfile
tack.example.com {
    rate_limit {
        zone tack {
            key    {remote_host}
            events 100
            window 1m
        }
    }
    reverse_proxy 127.0.0.1:3210
}
```

---

## Scaling considerations

Tack is a single-writer SQLite application, designed for a solo developer or a
small team. There is no horizontal write scaling — that is a deliberate design
choice, not a gap. Practical guidance:

- **Vertical:** the binary is tiny (~12 MB idle RSS) and reads are sub-millisecond;
  a modest VM handles a small team comfortably.
- **Write throughput:** SQLite serializes writes. This is fine at small-team scale;
  it is the first thing you would feel under heavy concurrent writes.
- **Concurrency:** the DB runs in WAL mode for better read/write concurrency.
- Multi-user auth, per-user identity, and Postgres are explicitly **out of scope**
  for the current design (see the roadmap's "Future / Optional").

---

## Troubleshooting

**Service won't start** — check logs:

```bash
sudo journalctl -u tack -n 50      # systemd
docker compose logs tack            # Docker
```

Common causes: port already in use (change `TACK_PORT`), the data directory is not
writable by the service user, or a migration failure (inspect the `_migrations`
table).

**Database locked** — SQLite allows one writer at a time. Make sure only one `tack`
process points at the DB file. Keep `?mode=rwc` in the URL (the default).

**FTS5 not found** — search needs SQLite compiled with FTS5. The bundled/static
builds include it; a system SQLite without FTS5 will fail migrations.

**WebSocket not updating** — ensure the proxy forwards the `Upgrade`/`Connection`
headers (Caddy does automatically; nginx needs the two `proxy_set_header` lines
above) and does not time the connection out (`proxy_read_timeout 86400`).

**Database corruption** — `sqlite3 tack.db "PRAGMA integrity_check;"`, then restore
from a backup if needed.

---

## Maintenance

### Update procedure

```bash
# 1. Back up first
tack backup > /var/backups/tack/pre-upgrade-$(date +%F).db

# 2. Replace the binary (download a new release or rebuild)
sudo install -m 0755 tack /usr/local/bin/tack

# 3. Restart — migrations run forward automatically on startup
sudo systemctl restart tack
curl http://127.0.0.1:3210/api/health
```

### Database vacuum

```bash
sqlite3 /var/lib/tack/tack.db "VACUUM;"    # reclaim space; run occasionally
```

### Log rotation (systemd host with a file sink)

```bash
sudo tee /etc/logrotate.d/tack >/dev/null <<'EOF'
/var/log/tack/*.log {
    daily
    rotate 14
    compress
    delaycompress
    notifempty
    create 0640 tack tack
    postrotate
        systemctl reload tack
    endscript
}
EOF
```

If you use `journalctl` (no `TACK_LOG_FILE`), systemd already rotates the journal.

---

## Support

- [GitHub Issues](https://github.com/yielab/tack/issues)
- [API Reference](./API-REFERENCE.md)
- [Security Policy](../SECURITY.md)
