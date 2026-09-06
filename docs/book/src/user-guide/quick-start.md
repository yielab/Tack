# Quick Start

Two ways to get Tack running: **install the binary** (the fast path — no build tools, you just want to use Tack) or **run from source in development mode** (for contributors and people who want hot reload). Pick the one that matches you.

---

## Install and run (the fast path)

Tack is a single self-contained binary — the web UI, REST API, and SQLite engine are all inside one file. No runtime, database server, or container required. Choose any one of the methods below.

**Prefer an app you open instead of a command you run?** Download the desktop app from the [releases page](https://github.com/yielab/tack/releases) — the `.AppImage` or `.deb` on Linux, the `.msi` on Windows (macOS isn't built yet). Opening it starts the same server this page describes, inside its own window, with an icon in your system tray: closing the window leaves it running, and the tray's **Quit** is what actually stops it. Skip to [First use](#first-use) once it's open.

**One line (Linux / macOS):**

```sh
curl -fsSL https://raw.githubusercontent.com/yielab/tack/main/install.sh | sh
tack            # starts the server + web UI at http://localhost:3210
```

**Download a release archive** from the [releases page](https://github.com/yielab/tack/releases):

```sh
# Linux / macOS
tar xzf tack-*.tar.gz && cd tack-*/
./tack
```

On **Windows**, extract the zip and double-click `tack.exe`.

Then open **`http://localhost:3210`** in your browser. On first start, Tack creates `tack.db` and a `storage/` folder next to the binary and runs all 18 database migrations automatically. Those two paths *are* your data — back them up and you've backed up everything.

> **First-run note (unsigned binary).** The binaries are not code-signed yet. On macOS, right-click → **Open** the first time (or run `xattr -d com.apple.quarantine tack`). On Windows, click **More info → Run anyway** if SmartScreen appears.

**Verify the server is up:**

```sh
curl http://localhost:3210/api/health
# {"status":"ok","version":"0.1.0-beta.6","migrations_applied":18}
```

If this fails, see [Troubleshooting](troubleshooting.md).

---

## First use

1. **Create a project.** Click **New Project** on the Projects page, or press `Ctrl+K` and type "new project". Choose a **project type** — a template that pre-loads a matching [workflow](workflows.md) and [vocabulary](vocabulary.md) you can customize later.

2. **Add an item.** On the Board, click the **+** inside any column, or the **+ New** toolbar button. Give it a title and press Enter. An *item* is the basic unit of work — a task, bug, building, assignment, or whatever your vocabulary calls it.

3. **Move it.** Drag the card to another column. Status changes save immediately with optimistic UI (the card moves before the server confirms).

4. **Open the detail drawer.** Click the card body (not the drag handle) to see all fields, dependencies, comments, attachments, and custom fields. See [Working with Items](items.md).

5. **Find your way around.** Press `Ctrl+K` for the [command palette](command-palette.md) (jump to any view, run an action) or `Ctrl+/` to search items. Switch theme and palette from the sidebar footer — see [Appearance](appearance.md).

Ready to put it on a network or add a token? See [Administration & Security](administration.md).

---

## Run an item with an agent

The fewest steps from the binary you already have to a coding agent completing an item
on your board, no second process and no operator-issued token — every step below is a
real command against a real server, copied from an actual run.

Start the server with the embedded runner instead of plain `tack` (it self-provisions
on first start — see [Standalone mode](agent-runners.md#standalone-mode-tack-serve---with-runner)):

```sh
tack serve --with-runner
```

In another terminal, create an agent profile — its instructions travel with every
request created against it:

```sh
tack agent-profile create "release-notes" \
  --instructions "Summarize the diff and write docs/CHANGELOG entries."
```

```text
Created agent profile: release-notes (ap_91b4e)
  id: ap_91b4ea76-9f1a-4725-8a58-21a57d92572c
```

Find the runner id — the embedded runner enrolled itself under it:

```sh
curl -s http://127.0.0.1:3210/api/runners | jq -r '.data[].runner_id'
```

Using the item id from [First use](#first-use) above, create the execution request
(swap in your own harness — `codex` or `claude-code` — and whichever model
it accepts; see [Choosing a model and a
provider](agent-runners.md#choosing-a-model-and-a-provider) if unsure):

```sh
tack execution create <ITEM_ID> \
  --runner <RUNNER_ID> \
  --agent-profile ap_91b4ea76-9f1a-4725-8a58-21a57d92572c \
  --harness claude-code --model-provider anthropic --model-id claude-sonnet-4-5 \
  --agent-profile-snapshot '{"name":"release-notes","instructions":"Summarize the diff and write docs/CHANGELOG entries.","tool_policy":{},"timeout_seconds":600,"budgets":{}}' \
  --repository '{"kind":"git","remote":"/path/to/your/repo","base_revision":"<COMMIT_SHA>","subdirectory":null}' \
  --permission-policy '{"tools":[],"network":false}' \
  --timeout-seconds 600
```

```text
Created execution request: exec_0fe
  state: queued
  id:    exec_0fe7252989f5f3d40a056c1da45b035039e4a8247ad89e5222cf9280134ec5d1
```

```sh
tack execution get exec_0fe7252989f5f3d40a056c1da45b035039e4a8247ad89e5222cf9280134ec5d1
```

```text
Execution request exec_0fe7252989f5f3d40a056c1da45b035039e4a8247ad89e5222cf9280134ec5d1
  state:   succeeded (done)
```

That is a completed attempt. The same request can be created from the item's "Run with
agent" button in the web UI instead of the CLI — see [Running an item with an
agent](agent-runners.md#running-an-item-with-an-agent) for all four ways to do this, the
full field-by-field reference, and what today's known gaps are (no memory of prior runs
in the modal, no runner picker — copy the id from the command above).

---

## Development mode (run from source)

For contributing to Tack or running with hot reload. The API server and Vite dev server run as separate processes.

**Prerequisites:**

- **Rust toolchain** via [rustup](https://rustup.rs) (stable, 1.75+)
- **Node.js 20+** and npm

```sh
rustc --version
node --version
```

**Terminal 1 — API server:**

```sh
git clone https://github.com/yielab/tack.git
cd tack
cargo run -p tack-cli -- serve
```

The server binds to `http://127.0.0.1:3210` and runs migrations on first start.

**Terminal 2 — frontend dev server:**

```sh
cd frontend
npm install
npm run dev
```

Vite starts at `http://localhost:5173` and proxies all `/api/*` requests to the API server. Open that URL in a browser.

### Build the single binary yourself

One process that serves both the API and the SPA — what the release archives ship:

```sh
# 1. Build the frontend
cd frontend && npm run build && cd ..

# 2. Build the API with the embedded SPA
cargo build --release --features embed-spa -p tack-cli

# 3. Run it
./target/release/tack
# open http://127.0.0.1:3210
```

Without `--features embed-spa` the binary serves only the API; use the Vite dev server or any static file host for the frontend.

---

## Environment variables

| Variable | Default | Description |
|---|---|---|
| `TACK_PORT` | `3210` | Listen port |
| `TACK_DATABASE_URL` | `sqlite:tack.db?mode=rwc` | SQLite file path |
| `TACK_LOG_LEVEL` | `info` | `trace` · `debug` · `info` · `warn` · `error` |
| `TACK_API_TOKEN` | _(none)_ | When set, all API calls need `Authorization: Bearer <token>` |

See [Configuration](configuration.md) for the full reference and `tack.toml` format, and [Administration & Security](administration.md) for tokens, CORS, webhooks, and cloud backup.
