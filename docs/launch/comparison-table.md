# How Tack compares

Prepared for launch (V-C3). Every number here was checked against a live source on
2026-09-06 — GitHub's API for stars/license/archived status, or this repository's own
code for what Tack does and doesn't do. Commands are inline so any of these can be
re-checked in thirty seconds. This is drafted material, not yet published anywhere.

## The two axes that matter

Almost everything in this space is strong on exactly one of two axes: **it plans work**
(sprints, dependencies, multiple views, a place a human reads status) or **it executes
work** (spawns Claude Code / Codex / another CLI agent against a real repo). Very few
tools do both, and of the ones that do, none durably recovers a killed agent run without
either duplicating the work or losing the record of it.

| | Plans work (real PM: sprints, deps, multiple views) | Executes work (spawns a coding agent) | Crash recovery with no duplicate work | Self-hosted, no required cloud | License / cost |
|---|---|---|---|---|---|
| **Tack** | Yes | Yes — Claude Code, Codex | **Yes** — fencing token + lease + replay table; demoed end to end, see `docs/screenshots/recovery-demo.gif` | Yes — one binary, one SQLite file | MIT, free |
| Vibe Kanban (community, post-Bloop) | Kanban board, agent-focused | Yes | Not documented in the public repo | Yes | Apache-2.0, free |
| Crystal → Nimbalyst | Session list, not a PM board | Yes | Not documented | Yes | MIT, free |
| Conductor | No | Yes | Not documented | No — macOS app + cloud | Closed, $22M Series A (2026-03) |
| Sculptor (Imbue) | No — per-agent session view | Yes — Claude Code, Pi | Not documented | Yes (Docker-isolated) | MIT, free |
| Emdash (YC W26) | Thin tracker fed from Linear/Jira/GitHub — no native sprints, dependency graph, or per-domain vocabulary | Yes — 22 CLI agents | Not documented | Yes (local SQLite) | Apache-2.0, free |
| OpenHands | No | Yes | Not documented | Yes | MIT, free |
| Plane / Huly / Focalboard / Leantime / Vikunja | Yes, mature | **No** — none of these executes anything | N/A | Yes | OSS, free |

"Not documented" means the project's own README/docs make no claim about it, not that
it's confirmed absent — nobody outside those projects can prove a negative from the
outside. Tack's own recovery claim is checked against its own code and a recorded run,
not asserted.

**Stars, for scale, not as a quality signal** (`gh api repos/<owner>/<repo> --jq
.stargazers_count`, 2026-09-06): Plane 58,941 · OpenHands 86,285 · Vibe Kanban 28,020 ·
Huly 27,577 · Focalboard 26,448 · Leantime 11,532 · Emdash 5,609 · Vikunja 5,290 ·
Crystal 3,115 · Sculptor 223 · **Tack 0**.

## Why the audience for this exists right now

Bloop, the company behind Vibe Kanban, shut down on 2026-04-10 ("Goodbye bloop" —
`vibekanban.com/blog/shutdown`): thousands of daily engineers, no business model found.
Vibe Kanban itself didn't disappear — it went community-maintained under Apache-2.0 —
but its cloud features (shared issues, comments, cross-org projects) were removed 30
days later, and the company that built it is gone. Crystal was deprecated in February
2026, its own README now reading "deprecated and replaced by Nimbalyst," with existing
users explicitly told to migrate for active updates. Both are real, large, named
codebases (28k and 3.1k stars) whose users had the ground shift under them inside the
same twelve months. That's the audience these drafted posts are written for — not a
hypothetical one.

## Where Tack is honestly behind

A comparison that only lists wins reads as marketing and gets discounted. These are
real, checked against the code, not softened:

- **No accounts.** One optional shared Bearer token (`TACK_API_TOKEN`); no per-user
  identity, roles, or permissions. Every operator with the token can do everything.
  (ADR 0059 documents this as a deliberate posture for a single-operator tool, not an
  oversight — see `docs/adr/0059-single-operator-identity-posture.md`.)
- **No outbound notifications.** Zero SMTP/email code anywhere in the tree —
  `grep -rn 'smtp\|email' crates/` turns up only a custom-field *type* named `email`
  and test fixtures setting `git config user.email`, nothing that sends mail. If you
  want to know an attempt finished, you have to look.
- **English only.** The vocabulary system renames domain terms ("Sprint" → "Iteration")
  per project type; it has no language layer. There is no `i18n`/`locale` string in the
  frontend (`grep -rn 'i18n\|locale' frontend/src` finds only `localeCompare` calls
  used for deterministic sort order, not translation).
- **One contributor.** `git shortlog -sn --all` shows one person (a second name in
  that log, `structuralB`, resolves to the same GitHub account via its commit's own
  noreply email — checked, not assumed).
- **Zero users outside this repository's own testing.** 0 stars, 0 forks, 0 human-filed
  issues (`gh issue list --state all` returns none; the GitHub API's "3 open issues"
  count is entirely Dependabot PRs).
- **No time tracking.** `estimate`/`estimate_unit` exist on an item; nothing records
  time actually spent — no `time_spent` field anywhere in the schema.
- **Artifacts download, they don't diff.** An agent's generated files can be downloaded
  one at a time; there's no in-UI viewer to see what changed.
- **One active SQLite writer.** S3 backup is snapshot replication, not live
  multi-writer sync — this is a single-machine tool, not a distributed one.
- **Not code-signed.** Release binaries need a right-click-Open on macOS or
  SmartScreen's "More info → Run anyway" on Windows.
- **No native mobile app**, and the desktop app itself hasn't shipped a macOS build yet.

None of these are secret — they're the same list in the README's "Known limitations"
and this repository's own architecture decision records. Repeating them here is the
point: the pitch is what's real, not what's aspirational.
