# Configuration Reference

The complete environment/TOML configuration for the API server and the runner.
Moved from CLAUDE.md (2026-08-19) so agent context stays lean; this file is the
single authority for these tables — update it, not CLAUDE.md, when adding a variable.

The API server loads configuration from `tack.toml` (if present) or environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `TACK_HOST` | `127.0.0.1` | Server bind address |
| `TACK_PORT` | `3210` | Server port |
| `TACK_DATABASE_URL` | `sqlite:tack.db?mode=rwc` | SQLite database path |
| `TACK_LOG_LEVEL` | `info` | `trace`, `debug`, `info`, `warn`, `error` |
| `TACK_LOG_JSON` | `false` | Structured JSON logging |
| `TACK_LOG_FILE` | _(none)_ | Write logs to this file **in addition to** stdout, in the same format `TACK_LOG_JSON` selects. Missing parent directories are created; if the directory cannot be created the server logs to stdout only rather than refusing to start. `tack service install` and the desktop app both set this so logs survive without a service manager attached |
| `TACK_STORAGE_DIR` | `./storage` | Attachment storage directory |
| `TACK_API_TOKEN` | _(none)_ | Optional Bearer token — requires `Authorization: Bearer <token>` on all API requests |
| `TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK` | `false` | Explicit opt-out for the startup refusal to bind a non-loopback address with no `TACK_API_TOKEN` set (see `docs/adr/0059-single-operator-identity-posture.md`). Loopback binds are unaffected either way. Off by default — this widens who can reach an unauthenticated API, so it must be a deliberate choice, never a fallback the code takes on its own |
| `TACK_ALLOWED_ORIGINS` | `http://localhost:8080,http://127.0.0.1:8080,http://localhost:3210,http://127.0.0.1:3210,https://tack.test` | Comma-separated allow-list of browser origins. Setting it replaces the default list. It gates CORS, and it gates the board's live WebSocket upgrade for any request whose `Origin` is not a loopback host — a bind to a loopback address (the default) additionally authorizes any loopback-hosted `Origin` on its own, so a local UI on any port (Vite's `5173` dev server included) receives live events with no configuration; a non-loopback bind, or a UI served from a non-loopback origin, still needs its origin listed here or the handshake is refused before it completes |
| `TACK_MAX_BODY_SIZE` | `2097152` | Global request body limit in bytes (default 2 MB; upload endpoint is always 50 MB) |
| `TACK_WEBHOOK_URL` | _(none)_ | Outbound webhook URL — when set, POSTs JSON events on item create/update/delete, sprint status changes, and due-soon alerts |
| `TACK_WEBHOOK_SECRET` | _(none)_ | HMAC-SHA256 signing secret; adds `X-Tack-Signature: sha256=<hex>` to each delivery |
| `TACK_GITHUB_TOKEN` | _(none)_ | GitHub PAT (`repo` scope). When set, item status changes are pushed back to linked GitHub issues (push-only: item done ⇄ issue closed). Never logged. See `docs/GITHUB-SYNC.md` |
| `TACK_GITHUB_API_BASE` | `https://api.github.com` | GitHub API root — override for GitHub Enterprise or to point tests at a mock. Used by both import and push-back |
| `TACK_BACKUP_ENDPOINT` | _(none)_ | S3-compatible endpoint URL (e.g. `https://<acct>.r2.cloudflarestorage.com`); omit for AWS S3 |
| `TACK_BACKUP_BUCKET` | _(none)_ | Bucket name — **required** to enable remote backup |
| `TACK_BACKUP_REGION` | `auto` | AWS/S3 region; Cloudflare R2 uses `auto` |
| `TACK_BACKUP_ACCESS_KEY` | _(none)_ | S3 access key ID — required to enable remote backup |
| `TACK_BACKUP_SECRET_KEY` | _(none)_ | S3 secret access key — required; never logged |
| `TACK_BACKUP_PREFIX` | `tack` | Object key prefix inside the bucket |
| `TACK_BACKUP_INTERVAL_SECS` | _(none)_ | Auto-backup interval in seconds; omit for manual-only. Values below 60 are raised to 60 with a warning — a tighter loop copies the database more often than it can change |
| `TACK_BACKUP_RETENTION` | `10` | Number of remote backups to keep after each upload |
| `TACK_LOCAL_RUNNER_ENABLE` | `false` | Startup default for whether the embedded runner runs — the same gate `tack serve --with-runner` sets; either satisfies it. `1` or `true` (case-insensitive) turn it on; anything else, including unset, is off. Read into `AppConfig::local_runner_enable`; a `PUT /api/local-runner` from the UI (ADR 0061 decisions 2 and 6) overrides it at runtime in `app_meta`, the same precedence `TACK_ORCH_ENABLE` already has — see [Embedded runner](#embedded-runner-tack-serve---with-runner) below. Off by default and refused outright (never silently downgraded) on a non-loopback bind |
| `TACK_ORCH_ENABLE` | `false` | Enables the orchestration reconciler and the `/api/control-planes`, `/api/projects/{id}/orch-link`, `/api/fleet` routes (and their later-wave successors). Unset ⇒ no reconciler task spawned, and every orch route answers `409` with the stable error code `orchestration_disabled` and a message naming how to enable it. It is **not** a `404`: the routes are still mounted and still behind `require_token`, so an unauthenticated caller gets `401` first and learns nothing either way |
| `TACK_ORCH_POLL_SECS` | `10` | Reconciler base poll interval in seconds (before per-plane backoff + jitter) |
| `TACK_ORCH_EVENT_RETENTION_DAYS` | `90` | Days of `orch_events` (and, once ingested, `orch_metrics`) history kept before the retention sweep rolls old rows into per-day aggregates and deletes them |
| `TACK_ORCH_APPROVAL_TOKEN` | _(none)_ | Separate shared secret required to grant/deny a docket approval via `POST /api/approvals/{token}`. Deliberately distinct from `TACK_API_TOKEN` — granting an approval is higher-privilege than editing a card. Never logged |
| `TACK_ORCH_DISPATCH_TOKEN` | _(none)_ | Separate shared secret required to trigger a docket pipeline run via `POST /api/projects/{id}/orch-dispatch` (ADR 0065). Mirrors `TACK_ORCH_APPROVAL_TOKEN` exactly: distinct from `TACK_API_TOKEN`, **fail-closed when unset** — this spends money on a remote fleet and cannot be cancelled. Never logged. The route reports only that a run *started*, never that it was *permitted*: docket's `/dispatch/{project}` answers before the pipeline itself runs, so a guardrail block is not observable on this response |
| `TACK_EXECUTION_RETENTION_ENABLE` | `false` | Enables the execution-domain retention sweep. **Off by default** (`crates/tack-api/src/config.rs#default_execution_retention_enable`) — this sweep deletes rows **and on-disk blobs**, so data deletion must be an explicit operator opt-in, matching `TACK_ORCH_ENABLE`'s own off-by-default posture. Covers four things, across two runtime tasks: (a) replay/idempotency bookkeeping and (b) terminal `execution_events` purge (`tack-orch`), plus (c) `execution_artifacts` rows **and their `TACK_STORAGE_DIR/execution-artifacts` blobs** and (d) overdue-decision expiry (`pending` → `expired`). Artifact blobs are typically the largest consumer in this domain. Decision expiry deliberately shares this one gate rather than running always-on — a test pins that posture so changing it is a reviewed diff |
| `TACK_EXECUTION_RETENTION_DAYS` | `90` | Days of history kept before the sweep purges it — applies to all four categories above (replay/idempotency bookkeeping, terminal `execution_events`, `execution_artifacts` rows and blobs, and decision expiry deadlines) |
| `TACK_EXECUTION_RETENTION_INTERVAL_SECS` | `3600` | Interval, in seconds, between execution-retention sweeps |
| `TACK_EXECUTION_HEALTH_ENABLE` | `true` | Enables the execution-domain health watch (runner/queue/lease/event counts; logs a `warn!` on stale-lease/`needs_operator` onset). On by default, unlike retention above — this reads and logs only, deletes nothing |
| `TACK_EXECUTION_HEALTH_INTERVAL_SECS` | `60` | Interval, in seconds, between execution health-watch checks |
| `TACK_EXECUTION_DECISION_TOKEN` | _(none)_ | Separate shared secret required to resolve a scoped execution decision via `POST /api/attempts/{attempt_id}/decisions/{decision_id}/resolve`. Mirrors `TACK_ORCH_APPROVAL_TOKEN` exactly: distinct from `TACK_API_TOKEN`, **fail-closed when unset** (the route rejects rather than falling back to the operator token). Never logged |

The `tack` CLI client — every subcommand other than `serve` — talks to a running server
over HTTP and never opens the database. It resolves the server's base URL from
`--base-url`, then `TACK_API_URL`, then `~/.tackrc`, then `http://127.0.0.1:3210`.

| Variable | Default | Description |
|----------|---------|-------------|
| `TACK_API_URL` | `http://127.0.0.1:3210` | Base URL of the server the CLI talks to, unless `--base-url` overrides it |
| `TACK_API_TOKEN` | _(none)_ | Bearer token the CLI sends, when the server it talks to requires one. Same variable the server reads to *require* a token — one value, two ends of the same connection |

The `tack-runner` binary is configured separately (defaults → `TOML` → environment → CLI flags,
in that order):

| Variable | Description |
|----------|-------------|
| `TACK_RUNNER_API_URL` | Tack API base URL the runner polls |
| `TACK_RUNNER_ENROLLMENT_TOKEN` | One-time operator-issued token; exchanged for a durable credential and never persisted |
| `TACK_RUNNER_ID` | Runner identity once enrolled |
| `TACK_RUNNER_STATE_DIR` | Owner-only directory for the journal and credential |
| `TACK_RUNNER_SECRET_VALUE` | Value `tack runner secret set` stores; when unset it reads the value from stdin instead. Never a command-line argument, which would be visible in `ps` and shell history. Read once, not persisted by the variable — the store keeps it (OS keychain, or an owner-only file where none answers within `PLATFORM_STORE_TIMEOUT`; `tack runner doctor` reports which backend a given boot picked) |
| `TACK_RUNNER_PROVIDER_VERCEL_AI_GATEWAY_ENABLED` | Turns on the `vercel_ai_gateway` provider endpoint (`[provider.vercel_ai_gateway]` in the TOML config). `1` or `true` (case-insensitive) enable it; default `false`. Off by default — this points a harness at a network endpoint and needs a credential, so it is a deliberate opt-in, never a fallback the runner takes on its own |
| `TACK_RUNNER_PROVIDER_VERCEL_AI_GATEWAY_SECRET` | Secret-store entry name the provider's credential is resolved from. Default `vercel-ai-gateway/default` — `SecretStore::resolve` does not append `/default` on its own, so a bare `vercel-ai-gateway` here resolves nothing |
| `TACK_RUNNER_VERCEL_AI_GATEWAY_TEST_BASE_URL` | **Test-only, not a real deployment knob.** `scripts/smoke.sh` step 13's only intended setter. When set to a loopback URL (`http://127.0.0.1:*`, `http://localhost:*`, or `http://[::1]:*`), rebases the catalog fetch and both per-harness endpoints under it instead of the real `ai-gateway.vercel.sh`, so a smoke run can prove key → catalog → spawn → actual-model against a local fake shim with a fake key, never a real vendor call. **This redirects wherever `fetch_catalog`'s `bearer_auth` sends the resolved credential** — it is not inert with respect to the secret, only with respect to whether the real vendor is ever reached. A non-loopback value is ignored outright (treated as unset), a cheap guard against an accidental redirect of a real key to a non-loopback host; it is not a defense against a process whose environment an attacker already controls, since that attacker could read the same credential directly out of the secret store this same process has open. Unset in every real deployment |
| `TACK_RUNNER_PROVIDER_ANTHROPIC_ENABLED` | Turns on Anthropic's own API as a provider endpoint (`[provider.anthropic]` in the TOML config) — a runner-held key pointed at `api.anthropic.com` directly, distinct from claude-code's own subscription login. Same on/off convention and off-by-default posture as the Vercel entry above |
| `TACK_RUNNER_PROVIDER_ANTHROPIC_SECRET` | Secret-store entry name this provider's credential is resolved from. Default `anthropic/default` |

Runner credentials are redacted in every log, `Debug` impl and error — the redaction is
structural (`RunnerCredential`'s `Debug`/`Display` are hardcoded to `[REDACTED]`), not
convention.

The `TACK_BACKUP_*` values are **defaults**. Cloud-backup settings (endpoint, bucket, region, access/secret key, prefix, retention) can also be edited at runtime from the UI (**Settings → Cloud Backup**) and are stored in the `app_meta` table; UI values override the env defaults. `TACK_BACKUP_INTERVAL_SECS` (automatic scheduling) remains env-only and takes effect at startup. The secret key is write-only over the API — never returned to clients.

## Embedded runner (`tack serve --with-runner`)

`tack serve --with-runner` (or `TACK_LOCAL_RUNNER_ENABLE=1`) runs the runner role as a
task inside the same process as the server, speaking runner-v1 over loopback HTTP
exactly like a remote runner would — see
[`docs/adr/0058-standalone-single-binary-runner.md`](adr/0058-standalone-single-binary-runner.md)
for why that HTTP hop is kept rather than shortcut. This is the fewest-steps way to see
a real agent attempt run against your own board: no second binary, no `tack runner
enroll` call, no token to copy anywhere.

- **Gate.** Off by default. `TACK_LOCAL_RUNNER_ENABLE` (table above) and `--with-runner`
  are equivalent; either turns it on. Refused outright — before any socket or database
  is opened — when the server is not bound to loopback (`TACK_HOST` other than
  `127.0.0.1`/`localhost`/an equivalent loopback address); this is a startup error, never
  a silent downgrade to a runner-less server, because an embedded runner executes
  arbitrary coding-agent processes on the host serving the UI.
- **UI toggle (ADR 0061 decisions 2 and 6).** `GET`/`PUT /api/local-runner` let a
  loopback-only UI turn the embedded runner on/off after `tack serve` is already up, with
  no restart — a `PUT` persists the choice to `app_meta` (overriding
  `TACK_LOCAL_RUNNER_ENABLE` from then on, the same precedence
  `PUT /api/settings/orchestration` already established) and starts or stops the runner
  task to match. `PUT`/`GET`/`DELETE /api/local-runner/secrets(/{name})` hand the runner
  a provider key the same way — write-only, never echoed, stored in whichever backend
  `tack runner secret set` would have used. Every one of these routes is absent (a plain
  404, not a gate that refuses) on any non-loopback bind, or when the process embedding
  the server never wired an embedded runner in at all (a bare library caller of
  `tack_api::serve()`).
- **First run.** A fresh state directory with no stored session self-provisions: it
  creates its own pending-runner row and redeems its own one-time enrollment token
  in-process, so no token is ever printed, copied, or configured by hand. A later start
  against the same state directory reuses the credential already on disk instead of
  provisioning a second runner.
- **State directory.** Defaults to `<TACK_STORAGE_DIR>/runner` — scoped to the same
  configuration as the database, so a server started against a different
  `TACK_DATABASE_URL` (paired, as every other per-install artifact in this crate already
  assumes, with its own `TACK_STORAGE_DIR`) never resolves to another server's runner
  state. `TACK_RUNNER_STATE_DIR` still overrides this default when set, exactly as it
  does for the standalone `tack-runner` binary (whose own default remains the bare,
  cwd-relative `.tack-runner` — it has no database or `storage_dir` to scope against).
  Holds the runner's credential (`session.json`) and its attempt journal, both written
  owner-only (`session.json` mode `0600`; the directory itself and journal entries
  `0700`/`0600`) — confirmed with `stat -c '%a'` against a real run, not assumed from the
  write path. An install upgrading from before this default existed has its already-
  enrolled state moved there automatically, once, the first time the new directory is
  found missing and the old one is not — never the reverse, and never once the new
  directory already exists.
- **Vendor/provider credentials — Tack is never a model gateway.** Each harness
  authenticates itself using its own mechanism; Tack does not read, store, forward, or
  proxy any of it, embedded or standalone. `tack runner doctor` reports exactly what
  this machine's own harnesses declare — run it yourself rather than trusting a stale
  copy in this file. The two current harnesses, mirrored from a real `tack runner
  doctor` run on a machine with both installed — the harness vocabulary itself is open
  (a runner may report any kind string), this build just ships adapters for these two:

  | Harness | How it authenticates | Gateway-routed variant (`[provider.vercel_ai_gateway]`) |
  |---|---|---|
  | `codex` | Its own CLI login flow or an API key it reads from its own environment/config (`codex --help`). This adapter forwards **no ambient host environment** into a run — only entries explicitly set on the execution request's own `environment` field ever reach the process. | When a request's provider names the configured endpoint: per-invocation `-c model_provider=…`/`model_providers.<key>.*` flags plus `AI_GATEWAY_API_KEY` in the spawned environment — never a write to `~/.codex/config.toml`. A request for a direct model receives none of it. |
  | `claude-code` | Typically an OAuth session under `$HOME/.claude` from its own login flow, or an API key from its own environment. This adapter forwards `HOME` and `PATH` from the runner process's own environment so the installed CLI can find its existing session. | `ANTHROPIC_BASE_URL` and `ANTHROPIC_AUTH_TOKEN` (plus a defensive empty `ANTHROPIC_API_KEY`) in the spawned environment, only when the request's provider names the configured endpoint. A request for a direct model receives none of it. |

  OpenRouter access and local-model endpoints (llama.cpp and similar) are configured the
  same way: through the harness's own configuration or environment. No `TACK_*` variable
  on the **API server** names a model provider or endpoint, and the API server itself
  never holds, forwards, or proxies a provider credential. The **runner** is not under
  that restriction: it may hold a provider key in its own owner-only state directory, and
  a loopback-only, embedded-runner-only route hands one to that store without the key
  ever touching `tack.db`, a log line, or the operator API otherwise — see
  [`docs/adr/0061-provider-credentials-at-the-runner-boundary.md`](adr/0061-provider-credentials-at-the-runner-boundary.md)
  for what a runner may hold, how a key reaches it, and how a gateway's model catalog is
  fetched. See [`docs/adr/0050-runner-control-plane.md`](adr/0050-runner-control-plane.md)
  ("the Tack API never starts a coding harness and never becomes a model proxy") and
  [`docs/adr/0058-standalone-single-binary-runner.md`](adr/0058-standalone-single-binary-runner.md)
  ("Vendor credentials remain outside Tack") for the decisions this one bounds.
- **Model selection is a separate question from credentials, and it is answered.**
  Which `(provider, model_id)` reaches the harness for a given execution request is
  resolved server-side through a four-tier precedence (request override →
  agent-profile default → project default (`projects.default_model`, set from the
  Agents page or `PATCH /api/projects/{id}`) → fleet default →
  auto-select), live-verified end to end and fully documented in [Choosing a model and
  a provider](book/src/user-guide/agent-runners.md#choosing-a-model-and-a-provider) —
  including why an auto-select request accepts today but never schedules. No `TACK_*`
  variable is involved on either side of this: routing the choice and holding the
  credential are different operations, and this file's table above has no row for a
  model provider or endpoint by design.
- **Log visibility.** The embedded runner's own log lines (self-provisioning,
  enrollment, claim, completion — anything logged by `tack_runner::*` or by the `tack`
  binary's own `local_runner`/`local_enrollment` modules) do **not** appear under
  default logging. `init_tracing`'s default filter
  (`tack_api={level},tack_db={level},tack_core={level},tower_http=debug`) only ever
  names `tack_api`, `tack_db` and `tack_core` — `TACK_LOG_LEVEL` changes `{level}` for
  those three crates but cannot add a target the filter string never mentions, so this
  is not fixable by raising `TACK_LOG_LEVEL` alone. Set `RUST_LOG` explicitly to include
  the runner's own targets:

  ```bash
  RUST_LOG=tack=info,tack_runner=info,tack_api=info,tack_db=info,tack_core=info \
    tack serve --with-runner
  ```

  Verified on a fresh state directory: under default logging, `tack_runner::*` and
  `tack::local_enrollment`/`tack::local_runner` produced zero log lines while the
  embedded runner enrolled and ran a real attempt; with the `RUST_LOG` override above,
  the same run showed `tack::local_enrollment: self-provisioned a local runner for the
  embedded runner to redeem ...`, `tack_runner::runtime: runner runtime started ...` and
  `tack_runner::client::transport: runner enrolled ...`. Server-side handler logs (e.g.
  `tack_api::handlers::runner_protocol`'s own `runner enrolled runner_id=...` line) are
  visible either way, since `tack_api` is already in the default filter — only the
  *runner's own* log lines were missing.

## Debugging

```bash
# Debug logging
TACK_LOG_LEVEL=debug cargo run -p tack-cli -- serve

# Trace SQL queries
RUST_LOG=tack_db=trace,tack_api=debug cargo run -p tack-cli -- serve

# JSON logs (for log aggregators)
TACK_LOG_JSON=true cargo run -p tack-cli -- serve

# See the embedded runner's own log lines under `--with-runner` (see
# "Embedded runner" above — off by default, silent by default)
RUST_LOG=tack=info,tack_runner=info,tack_api=info,tack_db=info,tack_core=info \
  cargo run -p tack-cli -- serve --with-runner
```

