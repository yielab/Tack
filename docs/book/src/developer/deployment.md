# Deployment

Tack is a single process with no external service dependencies. The deployment model is
intentionally minimal: copy a binary, point it at a directory, run it.

> This page covers the single-binary, systemd, and reverse-proxy models. For
> **Docker / docker-compose**, the full environment-variable table, and production
> hardening, see the standalone [Deployment Guide](../../../DEPLOYMENT-GUIDE.md).
> For tokens, CORS, webhooks, and cloud backup, see
> [Administration & Security](../user-guide/administration.md).

---

## Single-Binary Deployment (Recommended)

Build one binary that embeds the SPA and serves everything:

```sh
# 1. Build the frontend
cd frontend && npm ci && npm run build && cd ..

# 2. Build the release binary with embedded SPA
cargo build --release --features embed-spa -p tack-cli

# Result: target/release/tack
```

Copy the binary to the server and run it:

```sh
scp target/release/tack user@server:/opt/tack/tack
ssh user@server

# On the server
mkdir -p /var/data/tack /var/log/tack
TACK_DATABASE_URL=sqlite:/var/data/tack/tack.db \
TACK_STORAGE_DIR=/var/data/tack/storage \
TACK_LOG_FILE=/var/log/tack/api.log \
/opt/tack/tack
```

The binary is statically linked and has no runtime dependencies.

---

## systemd Service

Create `/etc/systemd/system/tack.service`:

```ini
[Unit]
Description=Tack API server
After=network.target

[Service]
Type=simple
User=tack
WorkingDirectory=/opt/tack
ExecStart=/opt/tack/tack
Restart=on-failure
RestartSec=5

Environment=TACK_DATABASE_URL=sqlite:/var/data/tack/tack.db
Environment=TACK_STORAGE_DIR=/var/data/tack/storage
Environment=TACK_LOG_FILE=/var/log/tack/api.log
Environment=TACK_LOG_JSON=true
Environment=TACK_API_TOKEN=change-me

[Install]
WantedBy=multi-user.target
```

```sh
sudo systemctl daemon-reload
sudo systemctl enable --now tack
sudo journalctl -u tack -f
```

---

## Reverse Proxy with Caddy

Put Caddy in front to get automatic HTTPS:

```
tack.example.com {
    reverse_proxy 127.0.0.1:3210
}
```

```sh
sudo systemctl reload caddy
```

Caddy handles TLS certificates via Let's Encrypt automatically.

---

## Reverse Proxy with nginx

```nginx
server {
    listen 443 ssl;
    server_name tack.example.com;

    ssl_certificate     /etc/letsencrypt/live/tack.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/tack.example.com/privkey.pem;

    location / {
        proxy_pass         http://127.0.0.1:3210;
        proxy_http_version 1.1;
        proxy_set_header   Upgrade $http_upgrade;
        proxy_set_header   Connection "upgrade";
        proxy_set_header   Host $host;
        proxy_set_header   X-Real-IP $remote_addr;
    }
}
```

The `Upgrade` and `Connection` headers are required for the WebSocket endpoint (`/api/projects/{id}/boards/live`).

---

## Local Development with Caddy (.test domain)

For local use behind the project's `Caddyfile.local`:

```
tack.test {
    reverse_proxy 127.0.0.1:3210
}
```

Import it from the global `/home/ox/Sites/Caddyfile` and reload:

```sh
sudo systemctl reload caddy
```

The app is then available at `https://tack.test`.

---

## Security Checklist

- [ ] Set `TACK_API_TOKEN` to a long random string
- [ ] Set `TACK_ALLOWED_ORIGINS` to your domain only
- [ ] Run behind HTTPS (Caddy or nginx with Let's Encrypt)
- [ ] Run as a non-root system user
- [ ] Set database and storage paths outside the binary directory
- [ ] Schedule daily backups (see [Backup & Restore](../user-guide/backup-restore.md))
- [ ] Restrict port 3210 at the firewall — only the reverse proxy needs it

---

## Troubleshooting

**Migration errors on startup:**

```sh
sqlite3 /var/data/tack/tack.db "SELECT * FROM _migrations;"
```

Applied migrations are checked at every startup as an exact ordered prefix by name and
checksum — deleting or editing a row does not make it re-run; it shifts every later
row out of position and the server refuses to boot. If a migration record looks
corrupt, do not touch the `_migrations` table by hand: inspect it to see where the
history diverges, then restore a known-good backup (see
[Backup and Restore](../user-guide/backup-restore.md)).

**Database locked:**

SQLite allows only one writer at a time. Check for another process holding the file:

```sh
lsof /var/data/tack/tack.db
```

**FTS5 not available:**

```sh
sqlite3 /var/data/tack/tack.db "PRAGMA compile_options;" | grep FTS5
```

If FTS5 is missing, recompile SQLite with `SQLITE_ENABLE_FTS5`, or install a SQLite package that includes it.

**Binary size too large:**

The release binary uses `opt-level = "z"`, LTO, and symbol stripping. [Benchmarks](../../../BENCHMARKS.md) records the size this build should land near; a binary well past it usually means `--features embed-spa` picked up a large frontend dist. Check `du -sh frontend/dist/`.
