# VIII-B2 handoff

- Base SHA / branch / final SHA: `abfd24b` / `agent/viii-b2-compat-label` / `aabdd3044a4a5ed4cd61df07cc117a01cbf748e7`
- Files changed (must equal ownership list):
  - `crates/tack-api/src/handlers/orch.rs` (the one response shape — `OrchLinkResponse`)
  - `crates/tack-api/tests/orchestration/control_plane/resource.rs` (one new test case)
  - `docs/openapi.json`, `frontend/src/shared/api/schema.gen.ts` (regenerated)
  - `frontend/src/features/settings/orchestration/CompatibilityPanel.tsx` (new), `OrchestrationPanel.tsx`, `api.ts`, `OrchestrationPanel.test.tsx`
  - This handoff
- Contract fixtures consumed: none — this card touches no runner-v1 wire contract.
- Behavior implemented: `OrchLinkResponse` (the `GET`/`PUT /api/projects/{id}/orch-link`
  response) gained two fields, `compatibility_label` and `compatibility_policy`, populated
  from `tack_orch::adapters::legacy_bridge::LEGACY_DOCKET_COMPATIBILITY_LABEL` and
  `..._POLICY` — imported, never re-typed. `frontend/.../orchestration/CompatibilityPanel.tsx`
  renders both, sourced from the already-loaded `OrchLink`, no new fetch. No behavior the
  label describes was touched — no route added, no scheduling logic changed.
- Tests added and exact commands/results:
  - `cargo nextest run --workspace -E 'test(orch_link_carries_the_legacy_docket_compatibility_constants)'`
    → `1 test run: 1 passed`. Asserts both PUT and GET response JSON equal the
    `tack_orch::adapters::legacy_bridge` constants verbatim (not a copied literal).
  - `cd frontend && npx vitest run src/features/settings/orchestration` → `8 files, 63 tests
    passed`. The linked-project case in `OrchestrationPanel.test.tsx` mocks
    `compatibility_label: 'legacy-docket:test-fixture-v9'` and a fixture policy string
    distinct from the real constants, then asserts both render verbatim — proves the panel
    reads the API response, not a hardcoded string.
  - `cargo nextest run --workspace` → `1477 tests run: 1477 passed, 7 skipped`.
  - `cd frontend && npm run type-check && npx vitest run` → tsc clean; `91 files, 857 tests
    passed`.
  - `./.githooks/pre-push` → `✓ pre-push checks passed` (comments, test hygiene, `cargo fmt
    --all --check` for the workspace and `tack-desktop` separately, `cargo clippy
    --workspace --all-targets -- -D warnings`, lockfile + `schema.gen.ts` freshness).
- Failure/adversarial case proved: see *Revert proof* below — both the backend and
  frontend tests fail red when the wired value is a literal instead of the constant/prop.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: the two new fields are populated
  unconditionally on every `OrchLinkResponse`, not gated on the linked control plane's
  `kind`. This is accurate today — `tack_orch::adapters::registry::build` only ever
  constructs a `"docket"` adapter (confirmed by reading `registry.rs`; `github_actions.rs`
  is compile-only and not reachable through `CreateControlPlaneRequest`/`registry::build`),
  so every real `OrchLink` today points at a legacy Docket bridge. If a second real kind is
  ever registered, this response would need a `kind`-aware branch (or a `None`) — flagging
  it here rather than adding a DB join I do not own (`repo/orch.rs` is VIII-B1's file).
- Secrets/logging review: no secret-shaped data touched. The two new fields are static
  prose/constants, not user data; `docs/CONFIG.md`'s secret-scrubbing rule does not apply.
- Safe merge order and likely conflicts: independent of VIII-A1, VIII-B1, VIII-C1 — none of
  them touch `handlers/orch.rs`'s `OrchLinkResponse`, the `frontend/.../orchestration/`
  directory, or this test file. Merges cleanly onto `develop` at any point in Wave 24;
  VIII-A2 (which adds a route to the same `handlers/orch.rs` file) should merge after this
  branch, not before, since it will otherwise need to rebase past these two new struct
  fields.
- Checklist: no unowned files touched, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| An operator can see the Docket bridge's compatibility label and policy on a linked project's Orchestration settings tab | `frontend/src/features/settings/orchestration/CompatibilityPanel.tsx`; rendered inside `OrchestrationPanel.tsx`'s linked-project branch |
| The wire label is never a re-typed literal — it is the same string as the constant that names ADR 0060's decision | `orch_link_carries_the_legacy_docket_compatibility_constants` (`crates/tack-api/tests/orchestration/control_plane/resource.rs`); passes with the real import, fails when hand-hardcoded (see *Revert proof*) |
| The frontend renders whatever the API returns, not a hardcoded string | `OrchestrationPanel.test.tsx`'s linked-project test, mocked with a fixture value (`legacy-docket:test-fixture-v9`) distinct from the real constant; passes with `props.link.compatibility_label`, fails when the component hardcodes the real constant instead (see *Revert proof*) |
| No behavior the label describes changed — the bridge is exactly as maintained/optional as before | No route added; `dispatcher.rs`, `docket.rs`, `legacy_bridge.rs` itself untouched; `cargo nextest run --workspace` stays at 1477 passing |

## Measured numbers

- `cargo nextest run --workspace`: `1477 tests run: 1477 passed, 7 skipped` (command:
  `CARGO_TARGET_DIR=/tmp/tack-agent-targets/viii-b2 cargo nextest run --workspace`).
- `cd frontend && npx vitest run`: `91 files, 857 tests passed`.
- `git diff --stat abfd24b..HEAD`: 8 files changed, 143 insertions(+), 0 deletions(-) —
  entirely inside this card's ownership list.

## What a stranger still cannot do

A stranger arriving at this repo still cannot tell, from the API alone, which compatibility
state a project's Docket bridge is in *before* linking one — `compatibility_label`/
`compatibility_policy` only appear inside `link` once `linked: true`. That is deliberate
(§VIII.4's Acceptance 2 asks which *existing* response carries the label, and the only
response that has anything to attach it to pre-link is `OrchLinkView`'s `linked: false`
shell, which carries no `link` object at all) but it does mean an unlinked project's
Settings → Orchestration tab still shows nothing about the bridge's compatibility posture —
only the "set up orchestration" empty state. Surfacing it pre-link would mean either adding
a field to `OrchLinkView` itself (outside a per-link response, and a shape decision I did
not judge myself authorized to make beyond what Acceptance 2/3 asked for) or is left for a
future card.

## Which response, and why

**Chosen: `OrchLinkResponse`** (the `link` object inside `GET`/`PUT
/api/projects/{id}/orch-link`'s `OrchLinkView`). Reasons:
- It is the response `frontend/src/features/settings/orchestration/` already fetches and
  renders (`OrchestrationPanel.tsx` → `orchestrationApi.getLink`), and my ownership is
  scoped to that frontend directory — `ControlPlaneResponse`'s natural renderer lives in
  `frontend/src/features/settings/orchestrationSettings/` (the global control-planes admin
  page), a directory this card does not own.
- Semantically it is "the client-facing view of a project's control-plane link" — exactly
  the granularity ADR 0060's decision operates at (a project's bridge to Docket), not a
  fleet-wide metric.
- Every control plane this API can actually construct today is a Docket bridge (see *Known
  limitations* above), so attaching the label unconditionally here is accurate, not a
  guess.

**Rejected: `ControlPlaneResponse`** (`GET/POST/PATCH /api/control-planes`). This is
arguably the more "correct" per-instance home for a compatibility label (`kind`-scoped, one
row per registered bridge), but its renderer is `orchestrationSettings/`, which VIII-B2 does
not own — adding a field there without touching its frontend would violate "the frontend
renders the label from the API, never from a hardcoded string" with no test to pin it in my
own ownership.

**Rejected: `OrchBudgetResponse` / `OrchPolicyResponse`**. Both are consumed in
`orchestration/` and so were in scope, but both are metrics-shaped (spend estimate,
guardrail counters) with an established precedent (`BudgetPanel`'s own doc comment, the
"no `paused` field" rule in `api.ts`) of staying narrowly scoped to their one concern.
Attaching a static decision label to either would mix an identity fact into a numbers
response for no benefit over `OrchLinkResponse`, which already has the same reach and a
cleaner semantic fit.

## Where the prose policy lives, and why

**Chosen: both — carried in the JSON (`compatibility_policy`) and rendered in the UI**
(`CompatibilityPanel.tsx`, directly beneath the label badge). `LEGACY_DOCKET_COMPATIBILITY_POLICY`'s
own doc comment in `legacy_bridge.rs` says it exists "for `docs/GITHUB-SYNC.md`/
`docs/MCP.md`-style operator docs to quote verbatim rather than paraphrase" — keeping it
out of the wire format would force every future consumer (a doc generator, a CLI, a second
UI) back to the Rust source to get that exact string. The marginal cost is one string field;
the UI renders it because leaving a fetched-but-unrendered field on the response would be
exactly the kind of orphaned wiring `.claude/scope-discipline.md` warns about.

## Revert proof

**Backend** (`crates/tack-api/src/handlers/orch.rs`, temporarily): replaced
`compatibility_label: LEGACY_DOCKET_COMPATIBILITY_LABEL.to_string()` with
`compatibility_label: "wrong-label".to_string()`. Ran
`cargo nextest run --workspace -E 'test(orch_link_carries_the_legacy_docket_compatibility_constants)'`:

```
FAIL [   0.056s] (1/1) tack-api::orchestration control_plane::resource::orch_link_carries_the_legacy_docket_compatibility_constants
  left: String("wrong-label")
 right: "legacy-docket:maintained-bridge-v1"
```

Reverted; the same command then reported `1 test run: 1 passed`.

**Frontend** (`frontend/src/features/settings/orchestration/CompatibilityPanel.tsx`,
temporarily): replaced `{props.link.compatibility_label}` with a hardcoded
`{"legacy-docket:maintained-bridge-v1"}` (the real constant's value, to prove the test
distinguishes "renders the API value" from "renders *a* correct-looking string"). Ran
`npx vitest run src/features/settings/orchestration/OrchestrationPanel.test.tsx`:

```
AssertionError: expected 'BudgetHealthy1.0k in / 500 out tokens…' to contain
'legacy-docket:test-fixture-v9'
```

Reverted; the same command then reported `4 tests passed`.

## Context spent

- Tokens read before the first edit (cold start): part-viii README header + VIII-B2 block
  (~1.1k tokens), TODO.md §VIII.0–§VIII.4 (~4.5k tokens via `sed`, never the whole file),
  root `CLAUDE.md` (already resident from system context), `legacy_bridge.rs` (top ~120
  lines), `handlers/orch.rs` (targeted greps + ~350 lines read across several ranges),
  `frontend/src/features/settings/orchestration/*` (all six files) and
  `orchestrationSettings/` directory listing only (not its file contents).
- Context size at handoff: mid-size session, well under the point of needing compaction.
- Files opened and not used: none beyond the above — `../rack-cli` was deliberately not
  opened (see below); no dead-end reads.
- Read-list lines that were wrong: none — the dispatch prompt's read order matched what was
  needed.

## What I read in `../rack-cli`

Nothing. This card surfaces an existing Rust constant (`LEGACY_DOCKET_COMPATIBILITY_LABEL`)
that already fully encodes ADR 0060's decision; it adds no new claim about what docket sends
or accepts over the wire, so §VIII.1 rule 2 ("docket's source is the contract") had nothing
for this card to verify against. `../rack-cli` was not opened.

## The question I did not answer

None — Acceptance 2 and 3 were mine to decide (see *Which response, and why* / *Where the
prose policy lives, and why* above), and no ADR-level decision was missing or ambiguous for
this card's scope.

## Amendments

*(none yet)*
