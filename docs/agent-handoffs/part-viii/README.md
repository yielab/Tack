# Part VIII handoffs — Docket bridge hardening (Phase 62)

**Read this file's header and your card's block. Nothing else in it.** It exists so a card
agent spends its context on the card, not on discovering what to read. Everything about
*how* to work a card is identical to Part VII's and is restated here only where it differs.

One handoff per card, named `VIII-<card>.md`, written from [`TEMPLATE.md`](TEMPLATE.md).
Each card writes **exactly one**; corrections are dated amendments. No card edits the board
in `TODO.md`; the wave integrator does, after independent verification.

The board is `TODO.md` → **Part VIII**, §VIII.0–§VIII.6 (it sits **above** Part VII). The
decisions of record are `docs/adr/0060-docket-control-plane-disposition.md` (accepted
2026-08-31) and `docs/adr/0065-docket-pipeline-dispatch-trigger.md` (**proposed**
2026-09-08 — binding on VIII-A2 only, and VIII-A2 is not dispatched until it is accepted).

## What this Part is, in one paragraph

ADR 0060 decided the Docket bridge stays: maintained, optional, and never the owner of a
runner-v1 execution request. It left four things unfinished, and every card here closes
exactly one of them. **No card adds a new Docket surface.** A card that finds itself
designing one has left the Part — write the question in the handoff and stop.

## Waves, order, and the base to branch from

| Wave | Cards | Parallel? | Needs | Base SHA |
|---|---|---|---|---|
| 24 | VIII-A1 · VIII-B1 · VIII-B2 · VIII-C1 | yes — four disjoint file sets | nothing beyond ADR 0060 | `1b9b7e6` + the 2026-09-08 planning commit; **pin `git rev-parse --short develop` at dispatch**, not this line |
| 25 | VIII-A2 | — | ADR 0065 **accepted**, plus VIII-A1 and VIII-B2 integrated | the Wave 24 integration SHA |
| 26 | VIII-C2 | — | Wave 25 integrated | the Wave 25 integration SHA |

**Integration line: `develop`.** Every card branches as `agent/viii-<card>-<slug>`
(`agent/viii-a1-dispatch`, `agent/viii-b1-mirror-guard`, …) and never merges itself.

## Rules that bite in this Part specifically

1. **`../rack-cli` is docket's repository. Read it; never write to it.** It is a separate
   git repo. Nothing from it is ever committed into this tree. When a card needs to know
   what docket sends or accepts, `../rack-cli/src/docket/` is the contract — not a Tack DTO,
   not a fixture, not this file's prose.
2. **Never touch `~/.docket`.** It holds the user's real approvals and audit log, and a
   worktree of this very repo. A card that runs docket sets `DOCKET_HOME` to a temporary
   directory it owns and names that directory in its handoff.
3. **`cargo` writes outside `/home`.** That partition is 94% full (41G free) and this tree's
   `target/` is already 87G. Export the `CARGO_TARGET_DIR` your dispatch prompt pins before
   the first build. **A card that fills the disk fails the whole wave, not just itself.**
4. **`.githooks/pre-push` is the definition of done.** Run it. A green test suite is not a
   finished change — `cargo fmt`, `check-comments.sh` and `check-test-hygiene.sh` are part of
   the gate, and formatting is invisible until a push is attempted.
5. **Comments explain the code, never the board.** No card ids, wave numbers, dates or
   `TODO.md §` references in any comment you write. `check-comments.sh` enforces it.
6. **Never read `TODO.md` whole** (~199k tokens). Extract your Part with
   `grep -n "^# \|^## " TODO.md`, then `sed -n '<start>,<end>p'` for §VIII only.

## Card blocks

### VIII-A1 — `DocketAdapter::dispatch`, implemented

Board: `TODO.md` §VIII.4 → VIII-A1. Owns the `dispatch` method and the "Write methods"
paragraph in `crates/tack-orch/src/adapters/docket.rs`, the `dispatch` trait doc in
`crates/tack-orch/src/lib.rs`, and new cases in `crates/tack-orch/tests/docket_adapter_test.rs`
and `docket_wire_contract_test.rs`.

Read, in this order: the module doc of `adapters/docket.rs` (its "Write methods" and
"Verified live" sections), `../rack-cli/src/docket/serve.py`'s `do_POST` (the
`/dispatch/` branch — **this is the contract**), then `enqueue_task`'s implementation in the
same adapter as the shape to mirror. Do not read the reconciler.

The one trap: `enqueue_task` deliberately does not parse fields its signature cannot carry.
`dispatch` returns `Result<String, OrchError>` — the run id and nothing else. Do not widen
the trait signature.

### VIII-B1 — the mirror guard, enforced

Board: `TODO.md` §VIII.4 → VIII-B1. Owns `crates/tack-api/src/handlers/executions.rs`, one
read-only query in `crates/tack-db/src/repo/orch.rs`,
`crates/tack-api/tests/orchestration/dispatch/dual_scheduling.rs`, and the "One scheduling
owner" paragraph of `crates/tack-orch/src/adapters/legacy_bridge.rs`.

Read, in this order: `legacy_bridge.rs`'s "One scheduling owner" section (it states the gap
you are closing, in bold), `dispatcher.rs` around line 327 (the guard in the *other*
direction — mirror its shape), the existing `dual_scheduling.rs`, then `repo/orch.rs` for how
`orch_tasks` is queried today.

The one trap: with `TACK_ORCH_ENABLE` off, a stale `orch_tasks` row must **not** block
runner-v1 — that would invert "runner-v1 is the plan of record". Acceptance 3 requires a
test for both states.

### VIII-B2 — the compatibility decision reaches an operator

Board: `TODO.md` §VIII.4 → VIII-B2. Owns one response shape in
`crates/tack-api/src/handlers/orch.rs`, its renderer under
`frontend/src/features/settings/orchestration/`, the regenerated `docs/openapi.json` and
`frontend/src/shared/api/schema.gen.ts`, and one new case under `crates/tack-api/tests/orchestration/`.

Read, in this order: `legacy_bridge.rs`'s constants and the doc comments above them (they
say what the label is *for*, including that no route surfaces it today), then
`handlers/orch.rs` to choose which response carries it.

The one trap: the label must be **imported from `tack_orch::adapters::legacy_bridge`**, never
re-typed as a string literal, and a test must assert the response equals the constant. A
copied literal is exactly how the decision and the wire drift apart.

### VIII-C1 — measure the `orch_*_new` claim, then correct what repeats it

Board: `TODO.md` §VIII.4 → VIII-C1. **Documentation only.** Owns the
`orch_runs_new`/`orch_approvals_new` bullet in `.claude/scope-discipline.md` and any other
document its own grep finds repeating that claim.

Read, in this order: the `orch_*_new` bullet in `.claude/scope-discipline.md`, the
"Measurement" section of `docs/adr/0060-docket-control-plane-disposition.md` (it states the
opposite, and gives the command), then the 037/038 rebuild in `crates/tack-db/src/migrations.rs`.

The one trap: this card is the measurement, not a predetermined edit. **If the
scope-discipline bullet turns out to be right, change nothing and say so.** And whatever you
find, delete no table, migration or code — a real leftover is a finding for a new card.
