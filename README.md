# Tack

[![CI](https://github.com/yielab/tack/actions/workflows/ci.yml/badge.svg?branch=develop)](https://github.com/yielab/tack/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.94%2B-orange.svg)](https://www.rust-lang.org/)
[![Beta](https://img.shields.io/badge/status-beta-yellow.svg)](CHANGELOG.md)

**A self-hosted project board that dispatches its items to coding agents — Claude Code
or Codex — through runners that live where your code and credentials already
are. One binary. The board plans and records; the runner executes.**

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/diagrams/two-components-dark.svg">
    <img src="docs/diagrams/two-components-light.svg" width="720" alt="Two components: the board (one) on the left holds workflows, timelines, leases, fencing, and history; runners (many) on the right each launch a harness — Claude Code or Codex — near your code and credentials. One arrow, from runner to board, labeled &quot;pulls work&quot;: the board never calls out.">
  </picture>
</p>

> **Tack is two components, built to be one product.**
>
> **The board** is the project manager: workflows, timelines, dependencies, per-project
> vocabulary — one binary, one SQLite file, no accounts, no cloud. It is the plan, the
> policy and the record. It decides *what* runs, *when*, under *which* limits, and it keeps
> the durable history of every run: events, decisions, artifacts, and what it measurably
> cost. **It never executes code and never holds a model credential.**
>
> **The runner** is a small worker that lives where the code and the credentials already
> are — a laptop, a CI box, a machine with a GPU. It pulls work from the board, checks out
> an isolated workspace, launches the coding agent you already use — Claude Code or Codex
> — and reports back. **It holds the keys; the board never sees them.**
>
> They are separate because they scale and fail differently. **One board, many runners:**
> a board on a small VPS dispatches to runners on ten developers' machines, each with its
> own agent, model and capacity. A runner that dies mid-run cannot corrupt the board — its
> lease expires and its fencing token stops writing. A board that restarts cannot lose a
> run — the runner's journal knows what it started. **One developer runs both in one
> process with one command**, on the same contract, with the same recovery.

## Durable by design

The paragraph above isn't a claim without a witness. [`scripts/smoke.sh` step
9](scripts/smoke.sh#L322-L409) kills the runner mid-attempt against a real server and a
real lease, proves the attempt turns `needs_operator` with no blind duplicate execution,
then recovers it with an explicit operator requeue to a succeeded attempt. A recorded run
of this exact sequence lands in this slot next; until then, the script is the proof.

## How it works

1. **Plan it on the board** — create an item, assign it to an agent profile, set its
   budget and policy.
2. **A runner picks it up where the code is** — it pulls the request, checks out an
   isolated workspace, and launches the harness with its own credentials.
3. **The run is recorded on the item** — events, decisions, and artifacts land back on
   the board as they happen, and the finished attempt stays in the item's history.

## Two components

| | The board | The runner |
| --- | --- | --- |
| What it does | Plans, schedules, and records | Checks out, launches the harness, reports back |
| What it holds | Workflow, policy, budgets, durable history | Credentials, workspace, the harness subprocess |
| How many | One | Many — one per machine with code and credentials |
| What happens when it dies | Restarts and loses nothing — the runner's journal knows what it started | Its lease expires, its fencing token stops writing; no duplicate, no silent loss |
| How you run it | `tack serve` | `tack serve --with-runner` (embedded) or `tack-runner` (separate binary, remote) |

**Jump to:** [Features](#features) · [Screenshots](#screenshots) ·
[Requirements](#requirements) · [Run it](#run-it) · [Status](#status) ·
[Architecture](#architecture) · [Documentation](#documentation) ·
[Contributing](#contributing)

---

## Why Tack

Most open-source project managers — Plane, Huly, Focalboard, Leantime, Vikunja —
track work but execute none of it. The open-source agent orchestrators that used to
sit alongside them have mostly shut down or gone closed-source. Tack does both at
once: a real project manager (boards, timelines, dashboards, per-project vocabulary)
and a real, multi-harness agent execution fleet, in one self-hosted binary with no
external services.

## Features

### Agent execution

- Assign a board item to `claude-code` or `codex` and it runs as a
  tracked, durable attempt — not a fire-and-forget shell command
- Pull-based runner protocol: a lightweight `tack-runner` claims eligible work and
  executes near its own repo and credentials; Tack never calls into your machine
- Fenced leases (one active attempt per request), decisions for human-in-the-loop
  approval, and structured artifacts on every run
- Measured usage only — cost and token counts are shown as measured or explicitly
  **not measured**, never estimated or silently shown as zero

### Project management

- Configurable workflows — Scrum, Kanban, or phase-based — with per-project vocabulary
  so the UI, CLI, and API all speak the language of your domain
- Board, list, table, calendar, timeline, and dashboard views
- Hierarchical items, dependency DAGs with cycle detection, custom fields, comments,
  attachments, full-text search, templates, and bulk operations
- Realtime updates over WebSocket with optimistic UI
- JSON/YAML/CSV export, GitHub Issues and Linear import, hot and S3-compatible backup

### Automation surfaces

- A REST API described by a checked-in [OpenAPI spec](docs/openapi.json), plus a CLI
  with JSON output and shell completions
- `tack mcp` — lets Claude Code, Codex, and other MCP clients read and update the
  board through normal workflow validation
- Outbound signed webhooks and optional GitHub push sync

## Screenshots

<p align="center">
  <img src="docs/screenshots/agents.png" width="98%" alt="The Agents page, fully earned: agent execution on, Codex and Claude Code both detected, Claude Code's own login verified by a real test run, a project default model saved, and that test run's own attempt shown Succeeded." />
</p>
<p align="center">
  <img src="docs/screenshots/attempt.png" width="98%" alt="An item's Execution tab: a real attempt shown Succeeded, its requested-vs-actual model matched against claude-sonnet-4-5, and its usage economics — token cost measured, wall-clock cost explicitly Not measured rather than shown as zero." />
</p>
<p align="center">
  <img src="docs/screenshots/hero.gif" width="98%" alt="Board, Timeline, and vocabulary editor — project-management views only; no agent run is shown in this recording" />
</p>
<p align="center">
  <img src="docs/screenshots/board.png" width="49%" alt="Board — Kanban with WIP limits and drag-and-drop" />
  <img src="docs/screenshots/timeline.png" width="49%" alt="Timeline — Gantt view with draggable bars" />
</p>
<p align="center">
  <img src="docs/screenshots/dashboard.png" width="49%" alt="Dashboard — status distribution and sprint throughput" />
  <img src="docs/screenshots/list.png" width="49%" alt="List — sortable rows with inline editing" />
</p>

<details>
<summary>Vocabulary editor — rename any term to match your domain</summary>
<br>

![Vocabulary editor](docs/screenshots/settings-vocabulary.png)

</details>

## Requirements

Running a release binary needs nothing else: SQLite and the web UI are embedded in
the single `tack` binary — no Docker, no database server, no separate frontend build.

| | |
| --- | --- |
| **Platform** | Linux, macOS (Intel + Apple Silicon), Windows |
| **Browser** | Any current Chrome, Firefox, Safari, or Edge |
| **Footprint** | 10.3 MiB binary (UI embedded), ~11.7 MiB idle memory — measured in [Benchmarks](docs/BENCHMARKS.md) |

Building from source instead needs [Rust 1.94+](https://rustup.rs/) and
[Node.js 20+](https://nodejs.org/).

## Run it

**Get the app** — download it, open it, the board is a window on your machine, the
same server underneath:

- **Linux:** the `.AppImage` or `.deb` from the
  [releases page](https://github.com/yielab/tack/releases). The AppImage runs as
  downloaded (`chmod +x`, then run it); the `.deb` installs normally
  (`sudo apt install ./Tack_*_amd64.deb`).
- **Windows:** the `.msi` from the same page — run it.
- **macOS:** not built yet; use the binary below until it is.

![Tack's desktop window open on the Agents page, showing agent execution on and Codex and Claude Code both detected on the machine](docs/screenshots/desktop-window.png)

The app adds an icon to your system tray. **Closing the window doesn't stop
it** — the board keeps running, and the tray icon reopens the window. **Quit
from the tray** when you want it to actually stop.

![Tack's tray menu open: Open Tack, agent execution status, Launch at login, and Quit](docs/screenshots/desktop-tray.png)

> The app isn't code-signed yet. On macOS, right-click **Open** the first time. On
> Windows, use **More info → Run anyway** if SmartScreen appears.

**Or run the binary directly** — servers, CI, anywhere a window doesn't make sense.
Either way, `tack service install` keeps it running past the session that started
it — a per-user background service, no root required: a `systemd --user` unit on
Linux, a `launchd` agent on macOS. `tack service uninstall` removes it again;
`tack service status` says whether it's active. That's the same promise the app's tray
makes with a window attached: install it once, and it's there whenever you open the
board or point a client at it — not something you remember to start.

```bash
curl -fsSL https://raw.githubusercontent.com/yielab/tack/main/install.sh | sh
tack serve --with-runner
```

**With Cargo** — installs from the `develop` branch, the single binary with the UI
embedded:

```bash
cargo install --git https://github.com/yielab/tack tack-cli --features embed-spa
tack serve --with-runner
```

**Or download** the archive for your system from the
[releases page](https://github.com/yielab/tack/releases):

```bash
tar xzf tack-*.tar.gz && cd tack-*/
./tack serve --with-runner
```

**Windows:** extract the zip and run `tack.exe serve --with-runner`.

Open **`http://localhost:3210`**. Project data lives in `tack.db`; attachments live in
`storage/`. Back up both.

`--with-runner` self-provisions an agent runner inside the same process — no second
binary, no token to copy anywhere — so a board item assigned to `claude-code` or
`codex` (whichever of those you have installed and logged in) actually
executes. It's off by default and loopback-only; see
[Agent Runners](docs/book/src/user-guide/agent-runners.md#standalone-mode-tack-serve---with-runner)
and [`docs/CONFIG.md`](docs/CONFIG.md#embedded-runner-tack-serve---with-runner). Plain
`tack` (no flag) starts just the server and board UI, with no runner running yet — turn
it on, see what harness it found, and set a provider key or a default model from the
**Agents** page in the UI, no flag or restart required.

> The binary is not code-signed yet. On macOS, right-click **Open** the first time
> (or run `xattr -d com.apple.quarantine tack`). On Windows, use
> **More info → Run anyway** if SmartScreen appears.

## Status

Tack is in public beta. The core project-management product — workflows, views,
DAGs, search, backup — is complete. The native, harness-agnostic agent-execution
fleet described above is implemented and gated behind `--with-runner`, but has not
shipped in a tagged release yet; install it today from the `develop` branch with the
Cargo command above.

**Harness proof** — every row below is checked against the actual installed binaries
on a real machine, not mocked out; see `docs/agent-handoffs/part-v/V-A2.md` for the
full evidence, including reverted-fix proofs and live run logs.

| Harness | Status |
| --- | --- |
| `claude-code` | Completes real, live end-to-end attempts today, on a subscription or through a configured provider endpoint. |
| `codex` | Completes real, live end-to-end attempts through a configured provider endpoint. On a plain subscription it runs the whole pipeline — claim, checkout, spawn, real network call, structured result — but the account tier decides which models it may request. |

**Known limitations:**

- One optional shared Bearer token; no per-user identities or permissions
- One active SQLite writer; S3 backup is snapshot replication, not live multi-writer sync
- The browser UI requires its Tack server to be running — no offline mode
- The existing Docket integration is a legacy, disabled-by-default bridge, not proof
  of harness-agnostic execution
- Imported usage/cost values may be estimates; native telemetry always labels its
  measurement source
- Responsive web UI only — no native mobile application
- Release binaries are not code-signed yet

Full phase-by-phase history lives in the [roadmap](docs/book/src/roadmap.md); the
active board is [`TODO.md`](TODO.md).

## Architecture

Tack owns scheduling, policy, leases, and execution history. A lightweight
`tack-runner` runs near the source repository and credentials, claims eligible work,
and invokes a local harness through an adapter:

```text
PM item
   │ create execution request
   ▼
Tack scheduler ── policy + capability matching ──► eligible fleet
   │                                                   │
   │ lease with fencing token                          │ runner claims work
   ▼                                                   ▼
durable attempt ◄── events / decisions / artifacts ─ tack-runner
                                                        │
                                   HarnessAdapter ──────┼──────┐
                                                        │      │
                                                   Codex CLI  Claude Code
                                                        │
                                                 future harnesses
```

This separates concepts that must not be conflated:

| Concept | Responsibility |
| --- | --- |
| **PM item** | Human-facing unit of planned work. |
| **Execution request** | Durable request to perform an item, with policy and eligibility constraints. |
| **Fleet** | Schedulable pool of runners with declared capabilities. |
| **Runner** | Worker process that leases work and executes it near its repo and credentials. |
| **Harness** | Agent runtime such as Codex CLI or Claude Code. |
| **Attempt** | Immutable execution history for one lease and run. |
| **Decision** | Structured request for human input or authorization. |

Rules that hold everywhere: runners pull work, Tack never calls back into developer
machines; compatibility is capability-driven, not assumed; a request has at most one
valid active lease, enforced by a fencing token; ambiguous outcomes stop for operator
review instead of being retried blindly.

The application itself is a modular monolith, with the runner fleet as a separate
binary rather than a new layer inside it:

```text
tack-core   Domain models, workflow rules, vocabulary, and dependency graph (no I/O)
    ↑
tack-db     SQLite persistence through sqlx, FTS5, and repositories
    ↑
tack-orch   Optional Docket client and reconciliation boundary
    ↑
tack-api    Axum HTTP/WebSocket server, configuration, and integrations
    ↑
tack-cli    Server binary, CLI client, embedded SolidJS app, and MCP server

tack-runner Separate binary — pull-based protocol, credentials, harness adapters
```

See the [developer architecture overview](docs/book/src/developer/README.md) for the
current code and the [roadmap](docs/book/src/roadmap.md) for where it's headed.

## Documentation

Full documentation is in [`docs/book/`](docs/book/), built with
[mdBook](https://rust-lang.github.io/mdBook/) and published to
[yielab.github.io/tack](https://yielab.github.io/tack/) on every push to `develop`.

| Guide | Description |
| --- | --- |
| [Quick Start](docs/book/src/user-guide/quick-start.md) | First-run walkthrough. |
| [API Reference](docs/book/src/developer/api-reference.md) | Auth and API examples; OpenAPI is the machine-readable source of truth. |
| [CLI Reference](docs/book/src/user-guide/cli.md) | `tack` subcommands. |
| [MCP Server](docs/MCP.md) | Connect Tack to an interactive MCP-capable agent. |
| [Configuration](docs/book/src/user-guide/configuration.md) | Environment and `tack.toml` reference. |
| [Architecture](docs/book/src/developer/README.md) | Current crate boundaries and design decisions. |
| [Benchmarks](docs/BENCHMARKS.md) | Reproducible footprint and latency measurements. |
| [Testing](docs/TESTING.md) | Unit, integration, E2E, load, and security tests. |
| [Roadmap](docs/book/src/roadmap.md) | Phase-by-phase status, past and active. |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for how to report bugs, propose features, and
submit pull requests. Quick start for local development:

```bash
git clone https://github.com/yielab/tack.git
cd tack
git config core.hooksPath .githooks   # runs fmt + clippy before every push, like CI
make build                            # frontend + release binary
make dev                              # API + Vite hot reload
make test && make e2e && make audit   # unit/integration, browser, dependency checks
```

## License

MIT — see [LICENSE](LICENSE).
