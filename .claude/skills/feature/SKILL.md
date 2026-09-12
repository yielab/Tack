---
name: feature
description: Add or finish a Tack feature (new entity, endpoint, UI view, config, or an open gap) following the repo's layering, migration, auth and testing patterns. Use for "add X", "finish Y", or any feature work not tied to a planning-board card.
---

# Add / finish a feature the repo's way

> **Report using `.claude/reporting-contract.md`.** Lead with the capability in plain language — what someone can or cannot do now — and keep file and function names in the technical-detail section at the end. Explain every blocker as: what is missing, what it was for, what it blocks.
>
> **Budget your reading with `.claude/context-budget.md`** — `TODO.md` whole is ~199k tokens,
> the three active boards ~33k (one Part ~15k; one card's cold start ~7k), all handoffs together ~240k. Grep before you read; read a range
> before a file. CLAUDE.md is already in your context — don't re-derive or re-read it.
> **Bound your scope with `.claude/scope-discipline.md`** — this tree's recurring defect is
> well-built mechanisms with no caller.


For work that exists as a card on the active `TODO.md` board, use `/card` instead — it
carries the ownership and handoff rules. This skill is the architectural path.

## 0. Check it isn't already half-built — and that it should exist at all

**This is the step this codebase skips, and it is the expensive one.** The tree carries
`model_profiles` (a table, a repo module and a UI surface consulted by *nothing* since Phase
56), a fully built and contract-pinned `decisions` path that no harness has ever exercised,
and an entire docket control plane that Part III superseded and which for weeks could not
simply be deleted because hundreds of doc comments cited its board sections (0 do today;
ADR 0060 keeps it by decision). All three were built well. None of
them had a caller when they were written.

Read `.claude/scope-discipline.md` before designing. Then:

```bash
rg -n "<concept>" crates/ frontend/src | head -30        # existing code paths
grep -rn "<concept>" docs/agent-handoffs/ | head         # was it attempted/deferred/rejected?
grep -n "<concept>" TODO.md | head                       # is it already someone's card?
```

Three questions to answer before writing anything, in the handoff or the reply:

1. **Who calls it?** Name the caller. If it does not exist yet, this feature is that caller's
   work, not its own.
2. **Is it already carded?** Parts VI and V own specific files; §VI.5 and §V.5 list what is
   *deliberately* out of scope (notifications, i18n, time tracking, accounts). Building one of
   those off-board conflicts with a decision that was already made.
3. **Is one implementor enough?** One backend needs no trait. Build for the second real case,
   not the hypothetical fifth.

Recorded open gaps live in the **active waves' status-board entries** in `TODO.md` (~line
55 onward; never read the file whole — see `.claude/context-budget.md`) and the most recent
handoffs.

## 1. The layering path (inward-out; each layer has its own tests)

1. **tack-core** — model + pure logic (`models.rs`, `workflow.rs`, …). Zero I/O. Unit
   tests in-file while the trailing module is under 150 lines, else in `<module>/tests.rs`.
   Every layer's tests obey the size rules in `docs/plans/human-maintainability.md` §2.2
   (one claim per test, ≤ 60-line body, ≤ 60-char name, helpers in `tests/common`), checked
   by `python3 scripts/maintainability.py check --changed`.
2. **tack-db** — migration in `migrations.rs`: **one `ALTER` per migration name** (the
   runner executes statements individually without a wrapping transaction — a
   multi-`ALTER` migration that fails midway bricks the install). Repo module in `repo/`.
   Read-then-write transactions MUST be `BEGIN IMMEDIATE`. A new secret column is added
   to `remote_backup.rs::scrub_snapshot_secrets` in the same commit.
3. **tack-api** — handler in `handlers/`, DTOs, route in `router.rs`, spec entry in
   `openapi.rs`. Keep the two auth surfaces separate: operator routes under `/api`
   behind `require_token`; runner-facing routes under the runner protocol tree with
   their own credential — never route one through the other's middleware. Board-relevant
   mutations broadcast via `websocket::broadcast_event()`.
4. **Regenerate, never hand-edit** the OpenAPI spec and frontend schema (commands in
   `/gate`).
5. **frontend** — types from the generated schema only; colors via `--color-*` tokens,
   never raw hex; a changed response shape updates the matching unit/E2E mocks in the
   same change; unknown values render as an honest literal (e.g. `Not measured`) — a
   zero standing in for "unknown" is a lie, especially about money.
6. **tack-cli** (if operator-facing) — subcommand speaking HTTP to the server; the CLI
   never opens the DB directly.
7. Anything touching a frozen wire contract additionally updates the fixture dir + its
   pin table in one change (see `/card` rule) — fixtures are the authority.

## 2. Config & security posture

- Anything that **deletes data or reaches the network** ships off-by-default behind a
  `TACK_*_ENABLE` gate; read/log-only watchers may default on. Follow the precedent of
  the nearest existing flag.
- Privileged actions get their **own token**, distinct from `TACK_API_TOKEN`, fail-closed
  when unset (existing pattern: approval/decision tokens).
- New config vars are documented in `docs/CONFIG.md`'s table in the same change; secrets are
  write-only over the API and never logged.

## 3. Done means gated

Run `/gate <scope>` for every layer touched. Update an existing doc under `docs/` if one
covers the area; don't create new .md docs unprompted (workspace rule).
