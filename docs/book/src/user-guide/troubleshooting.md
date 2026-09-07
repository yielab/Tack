# Troubleshooting and FAQ

This page covers the problems you are most likely to hit while running Tack, and answers common questions about how it stores and protects your data. Tack is a single self-hosted binary backed by a local SQLite database, so most issues come down to the server process, the database file, or a few environment variables.

---

## Troubleshooting

Work through the matrix below first, then read the detailed sections for the commands and SQL you need.

| Symptom | Likely cause | Fix |
|---|---|---|
| Server exits immediately on start | Port `3210` already in use | Change `TACK_PORT` or stop the conflicting process |
| `database is locked` errors | Another process is holding `tack.db` | Run a single server; close other connections |
| Errors mentioning a failed migration | Interrupted/partial migration | Inspect `_migrations`; restore or restage a backup |
| Board or UI never loads | API server not running | Check `GET /api/health` |
| `401 Unauthorized` on every request | `TACK_API_TOKEN` set, but no/wrong `Authorization` header | Send `Authorization: Bearer <token>` |
| Browser console shows CORS errors | Origin not in the allow-list | Add it to `TACK_ALLOWED_ORIGINS` |
| Uploads rejected (`413` / too large) | File over the size limit | Stay under 50 MB; raise `TACK_MAX_BODY_SIZE` for non-attachments |
| Search returns nothing | SQLite built without FTS5 | Use a SQLite/binary with FTS5 enabled |
| Vocabulary or theme "didn't change" | Stale page state | Refresh; theme/palette live in `localStorage` |

---

### Server won't start / "port already in use"

**Cause.** By default the server binds to `127.0.0.1:3210`. If another process (often a previous Tack instance that did not exit) already holds that port, startup fails.

**Fix.** Either free the port or run Tack on a different one. The port comes from `TACK_PORT` (default `3210`); the bind address comes from `TACK_HOST` (default `127.0.0.1`).

```sh
# See what is using the default port
lsof -i :3210        # macOS / Linux

# Start on a different port
TACK_PORT=4000 tack serve
```

Open the UI at the host and port you started with, for example `http://127.0.0.1:4000`. See [Configuration](configuration.md) for the full list of variables.

---

### "Database is locked"

**Cause.** SQLite allows only one writer at a time. This error means another process already has `tack.db` open for writing — usually a second `tack serve` instance, an open `sqlite3` shell, or a database GUI.

**Fix.** Run exactly one Tack server against a given database file. Close any other process that has the file open (other `tack` instances, `sqlite3` sessions, DB browsers). The default connection string already uses read-write-create mode (`sqlite:tack.db?mode=rwc`), so you should not need to change it.

If you genuinely need a second instance, point it at a different database with `TACK_DATABASE_URL`.

---

### Migrations failed on startup

**Cause.** Tack runs its schema migrations automatically when the server starts and records each applied migration in the `_migrations` table. If the process is killed mid-migration, or the database file is from an incompatible build, startup can fail with a migration error.

**Fix.** Inspect which migrations were recorded:

```sh
sqlite3 tack.db
```

```sql
SELECT * FROM _migrations;
```

If a migration is recorded as applied but the schema is clearly incomplete, the safest recovery is to restore a known-good backup rather than hand-editing the schema. See [Backup and Restore](backup-restore.md). For the full upgrade path — what runs automatically, when Tack takes its own pre-upgrade snapshot, and how to safely enable the Part III runner-fleet execution features — see [`docs/MIGRATION-GUIDE.md`](../../../MIGRATION-GUIDE.md).

**How staged restore interacts with this.** A restore is *staged*, not applied live: Tack writes the uploaded database next to the live file as `<db>.restore` and tells you to restart. On the next startup, before opening the database, Tack swaps the files atomically:

- the current database is moved aside to `<db>.bak`,
- the staged `<db>.restore` becomes the live database.

So after a restore your previous database is preserved as `tack.db.bak`. If a restore leaves you worse off, stop the server and move `tack.db.bak` back to `tack.db` to return to the prior state.

---

### Board or UI won't load

**Cause.** The web UI is served by the same binary as the API. If the page is blank, stuck on a skeleton loader, or shows network errors, the API server is almost always down or unreachable.

**Fix.** Confirm the server is up with the health endpoint:

```sh
curl http://127.0.0.1:3210/api/health
```

A healthy server responds with `200 OK`. If the request hangs or is refused, the server is not running on that host/port — start it with `tack serve` and check the console for errors. If you changed `TACK_HOST`/`TACK_PORT`, query the address you actually bound to.

The `/api/health` endpoint is intentionally exempt from the API-token gate, so it always works even when `TACK_API_TOKEN` is set — making it a reliable liveness check.

---

### 401 Unauthorized / "token rejected"

**Cause.** When `TACK_API_TOKEN` is set on the server, every `/api/*` route except `/api/health` requires a matching Bearer token. A `401` means the header is missing, malformed, or does not match the configured token.

**Fix.** Send the token on every request:

```sh
curl -H "Authorization: Bearer <token>" http://127.0.0.1:3210/api/projects
```

The header value must be exactly `Bearer ` followed by the same string you set in `TACK_API_TOKEN`. If you are using the web UI behind a token, make sure the UI is configured with the same token. If you did not intend to require a token, unset `TACK_API_TOKEN` and restart. See [Administration & Security](administration.md).

---

### CORS errors in the browser console

**Cause.** The browser blocks requests from an origin that the server has not allow-listed. Tack's CORS allow-list defaults to local origins only:

```
http://localhost:8080
http://127.0.0.1:8080
http://localhost:3210
http://127.0.0.1:3210
https://tack.test
```

If you serve the UI from a different host, port, or scheme, the browser reports a CORS failure.

**Fix.** Add your origin to `TACK_ALLOWED_ORIGINS` (comma-separated) and restart:

```sh
TACK_ALLOWED_ORIGINS="https://tack.example.com,http://127.0.0.1:8080" tack serve
```

Use the exact scheme, host, and port the browser sends — `http://` and `https://`, and a non-default port, are all distinct origins.

---

### Uploads failing

**Cause.** There are two separate limits. File attachments have a fixed maximum of **50 MB**. All other (non-attachment) request bodies are capped by `TACK_MAX_BODY_SIZE`, which defaults to **2 MB** (`2097152` bytes). A request over its limit is rejected (typically `413 Payload Too Large`).

**Fix.** For attachments, keep individual files under 50 MB. For large JSON imports or other big non-attachment payloads, raise the global limit and restart:

```sh
# Allow 10 MB request bodies for non-attachment endpoints
TACK_MAX_BODY_SIZE=10485760 tack serve
```

Note that raising `TACK_MAX_BODY_SIZE` does not change the 50 MB attachment ceiling.

---

### Search / FTS5 not working

**Cause.** Full-text search is built on SQLite's FTS5 extension. If your SQLite library was compiled without FTS5, the search index cannot be created or queried.

**Fix.** Verify FTS5 is available in the SQLite your environment uses:

```sh
sqlite3 tack.db "PRAGMA compile_options;" | grep FTS5
```

If `ENABLE_FTS5` is not listed, use a SQLite build (or a Tack binary) with FTS5 enabled. The official Tack binaries ship with FTS5 support.

---

### Vocabulary or theme change "didn't apply"

**Cause.** Two different things are at play. Display preferences — the light/dark mode and the colour palette — are stored in your browser's `localStorage`, not on the server, so they are per-browser and persist locally. Vocabulary changes are saved server-side per project, but an already-open tab may still show cached labels.

**Fix.** Refresh the page (a hard refresh if needed). Theme and palette will reload from `localStorage`; vocabulary labels will reload from the server. If a theme looks wrong only in one browser, clearing that browser's site data resets it to defaults. See [Appearance](appearance.md) and [Vocabulary](vocabulary.md).

---

## FAQ

### Is my data sent anywhere?

No. Tack is fully self-hosted. All your projects, items, and comments live in a local SQLite database on the machine you run it on. Nothing is sent to Anthropic or to any Tack-operated service. Outbound traffic only happens for features you explicitly configure — for example a webhook URL, a GitHub/Linear import, or an S3-compatible cloud backup you set up yourself.

### Can I use Tack offline?

Yes, in the local-first sense. Tack runs entirely on your own machine or LAN, with no dependency on an external service. The one requirement is that the Tack server process is running: the web UI and CLI both talk to it over HTTP. As long as the server is up on your machine (or a machine on your network), you can work without any internet connection.

### Where is my data stored?

In two places, both local:

- **Database** — a single SQLite file. By default `TACK_DATABASE_URL` is `sqlite:tack.db?mode=rwc`, i.e. a `tack.db` file in the directory you run the server from.
- **Attachments** — uploaded files live under `TACK_STORAGE_DIR`, which defaults to `./storage`, organised by item.

To relocate either, set the corresponding variable (see [Configuration](configuration.md)) and restart.

### How do I back up my data?

Use Tack's backup features rather than copying the live database while the server is running. You can download a local backup, or push a copy to an S3-compatible cloud store, and stage a restore that applies on the next restart. See [Backup and Restore](backup-restore.md) for the full workflow.

### Is Tack for a single user or a team?

Both, with a focus on solo developers and small teams. Multiple people can work against the same server, and board changes propagate live to all connected clients over a WebSocket connection, so updates appear without manual refreshes.

### Which platforms does it run on?

Tack ships as a single self-contained binary for Linux, macOS, and Windows. There is no separate runtime to install; the web UI can be embedded in the binary so one file serves both the API and the SPA.

### How do I secure Tack on a network?

Tack defaults to `127.0.0.1` for local-only use. Before exposing it beyond your machine:

1. **Require a token.** Set `TACK_API_TOKEN` so all `/api/*` routes (except `/api/health`) demand `Authorization: Bearer <token>`.
2. **Restrict origins.** Set `TACK_ALLOWED_ORIGINS` to only the hostnames that should reach the UI.
3. **Terminate TLS at a reverse proxy.** Tack itself serves plain HTTP; put it behind a reverse proxy (such as Caddy or nginx) that provides HTTPS, and bind Tack to a private address.

See [Administration & Security](administration.md) for step-by-step setup.

### Does Tack integrate with GitHub or Linear?

Yes. You can import issues from GitHub and from Linear into a project. GitHub integration also supports pushing status changes back to linked issues when a GitHub token is configured. See [Import and Export](import-export.md) for setup, filters, and token requirements.
