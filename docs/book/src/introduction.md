# Tack

[![CI](https://github.com/yielab/tack/actions/workflows/ci.yml/badge.svg?branch=develop)](https://github.com/yielab/tack/actions/workflows/ci.yml)

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
> an isolated workspace, launches the coding agent you already use — Claude Code or
> Codex — and reports back. **It holds the keys; the board never sees them.**
>
> They are separate because they scale and fail differently. **One board, many runners:**
> a board on a small VPS dispatches to runners on ten developers' machines, each with its
> own agent, model and capacity. A runner that dies mid-run cannot corrupt the board — its
> lease expires and its fencing token stops writing. A board that restarts cannot lose a
> run — the runner's journal knows what it started. **One developer runs both in one
> process with one command**, on the same contract, with the same recovery.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="diagrams/two-components-dark.svg">
  <img src="diagrams/two-components-light.svg" alt="Two components: the board (one) on the left holds workflows, timelines, leases, fencing, and history; runners (many) on the right each launch a harness — Claude Code or Codex — near your code and credentials. One arrow, from runner to board, labeled &quot;pulls work&quot;: the board never calls out.">
</picture>

Tack tracks work for any domain — software sprints, a kitchen renovation, thesis chapters, a
maintenance schedule — through fully configurable vocabulary and workflow columns, and can hand
any item that's agent-eligible to a real coding-agent run instead of just tracking it. No
accounts. No cloud. No subscriptions. One binary, one SQLite file.

---

## Core concepts

Six terms recur throughout this documentation:

| Term | What it means |
|------|---------------|
| **Item** | The basic unit of work. One `Item` model backs everything — a task, bug, feature, epic, building, work order, assignment — and your project's vocabulary decides how it's labeled. |
| **Workflow** | The set of named status columns an item moves through (e.g. To Do → Doing → Done), each with a category and an optional WIP limit. See [Workflows & Statuses](user-guide/workflows.md). |
| **Project type** | A template chosen at creation that pre-loads a matching workflow *and* vocabulary (software, construction, legal, …). Everything stays editable afterward. |
| **Vocabulary** | Per-project label overrides that rename built-in terms to your domain — "Task" → "Work Order", "Sprint" → "Phase". The UI, CLI, and API all follow your terms. See [Vocabulary](user-guide/vocabulary.md). |
| **Runner** | A small worker process that lives where your code and credentials already are, pulls eligible items from the board, launches a coding-agent harness, and reports back. See [Agent Runners](user-guide/agent-runners.md). |
| **Run** (attempt) | One execution of an item by a runner — leased under a fencing token so at most one attempt is ever active, and recorded as durable history: events, decisions, artifacts, and measured cost. |

## How this documentation is organized

| Section | For whom |
|---------|----------|
| **User Guide** | Anyone running Tack: setup, views, CLI, configuration |
| **Developer Guide** | Contributors and people extending the codebase |
| **Learning Path** | Developers new to Rust, Axum, or SolidJS; explains the stack with analogies |
| **Roadmap** | What each development phase set out to do, and what came of it |

## Quick links

- [Agent Runners & Fleet Execution](user-guide/agent-runners.md) — handing a board item to Codex or Claude Code
- [Quick Start](user-guide/quick-start.md) — up and running in five minutes
- [Architecture Overview](developer/README.md) — the mental model behind the codebase
- [Frontend & Design System](developer/frontend.md) — tokens, palettes, and the UI kit
- [Rust Primer](developer/learning/rust-primer.md) — start here if Rust is new to you
- [API Reference](developer/api-reference.md) — every REST endpoint (see `docs/openapi.json` for the exact, generated count)

## Keeping docs current

These docs live alongside the code in `docs/book/src/`. Every push to `develop` runs
`mdbook build` in CI (with a broken-link check). If the book fails to build, CI fails,
so structural drift is caught before it merges.

For prose changes (user-facing descriptions, learning explanations), update the relevant
`.md` file in the same PR as your code change. The "Edit this page" link at the top of each
page opens the file directly on GitHub.

```
docs/book/src/
├── user-guide/        ← user-facing docs
├── developer/         ← contributor docs
│   └── learning/      ← stack explanation with analogies
└── roadmap.md
```
