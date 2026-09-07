# VI-C43 handoff

- Base SHA / branch / final SHA: `8db262b` / `agent/vi-c43-fleet-roster` / `cc6963d`
- Files changed (must equal ownership list): matches the ownership list plus two files
  the card did not name — see "Known limitations" below for why each was touched.
  - `frontend/src/features/agents/runnerFleet/FleetsPanel.tsx` (owned)
  - `frontend/src/features/agents/runnerFleet/FleetsPanel.test.tsx` (owned)
  - `frontend/src/shared/execution/api.ts` (owned — `fleetsApi` additions)
  - `frontend/src/shared/execution/api.test.ts` (owned — additions only)
  - `docs/book/src/user-guide/agent-runners.md` (owned — the fleet-gap bullet removed)
  - `CHANGELOG.md` (owned — one `[Unreleased]` line)
  - `frontend/src/features/agents/runnerFleet/RunnerHealthCard.tsx` (not in Owns — see below)
  - `frontend/src/features/agents/runnerFleet/RunnerFleetSection.test.tsx` (not in Owns — see below)
- Contract fixtures consumed: none. No backend change, no new endpoint;
  `docs/openapi.json` and `frontend/src/shared/api/schema.gen.ts` are untouched
  (verified: `git status --porcelain` on both is empty after the change, and
  `pre-push`'s own `npm run gen:api` regeneration diff is clean).
- Behavior implemented: `FleetsPanel` fetches `GET /runners` alongside the existing
  `GET /runner-fleets`, and for each fleet derives its roster by filtering runners
  whose `fleet_ids` contains that fleet — the read-back path the card's Context
  section named, since no endpoint returns a roster directly. Each member row shows
  its name and a state badge (`Active`/`Pending enrollment`/`Revoked`, from the
  runner's real `state` field) and a *Remove* button calling
  `DELETE /runner-fleets/{fleet_id}/members/{runner_id}`. A `Select` of runners not
  yet in the fleet plus an *Add* button calls
  `POST /runner-fleets/{fleet_id}/members`. Every add/remove refetches `GET /runners`
  and renders the server's own roster afterward — never an optimistic client-side
  list. `already_member` (the idempotent re-add response) shows as a `toast.info`
  notice, not an error. An empty fleet says "No members yet." in plain text; a fleet
  with no eligible runners to add says "No other runners available to add." instead
  of rendering an empty, useless `<select>`.
- Tests added and exact commands/results:
  - `cd frontend && npm run type-check` — clean (`tsc -b`, no output).
  - `cd frontend && npx vitest run` — 856 passed (0 failed), up from 855 on `develop`
    at the base SHA. `FleetsPanel.test.tsx` grew from 4 to 7 tests (rewrote the
    stale "states the membership gap" test into "lists a fleet with its concurrency
    cap and an empty roster"; added "derives the roster from fleet_ids: a runner in
    two fleets appears under both", "add calls POST .../members with the runner id
    and refetches the roster", "remove calls DELETE .../members/{runner_id} and
    refetches the roster"). `api.test.ts` gained `addMember() POSTs to
    /api/runner-fleets/{fleet_id}/members with the runner id` and `removeMember()
    DELETEs /api/runner-fleets/{fleet_id}/members/{runner_id}`.
  - `cd frontend && CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C43
    CARGO_BUILD_JOBS=4 npx playwright test agents-page a11y --project=chromium
    --workers=2` — 43 passed (0 failed), against the suite's own throwaway
    `frontend/e2e.db`/`storage-e2e` on ports 3399/5199. Includes "agents page —
    creating a fleet via the form has no accessibility violations", which hits the
    real, unmocked server and now scans the roster UI (empty fleet, empty runner
    list) for a11y violations.
  - `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C43 CARGO_BUILD_JOBS=4
    .githooks/pre-push` — green (comments, test hygiene, `cargo fmt --all --check`
    for both workspaces, `cargo clippy --workspace --all-targets -- -D warnings`,
    lockfile + `schema.gen.ts` freshness).
- Failure/adversarial case proved: reverting `FleetsPanel.tsx`'s change while keeping
  the new tests fails all three new roster tests (`membersOf is not a function` /
  `runners is not defined`) — the roster assertions are load-bearing on the actual
  fetch-and-filter logic, not on a mock artifact. Also proved the opposite direction:
  before this card, `grep -rn "/members" frontend/src --include='*.ts'
  --include='*.tsx' | grep -v test` had zero hits outside `schema.gen.ts`; after,
  it finds the two callers in `api.ts`, matching the card's Acceptance grep exactly.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields:
  - **`RunnerHealthCard.tsx` gained an exported `runnerStateBadge` helper, not the
    `RunnerConnectionStatus` chip the card's read list pointed at.** That existing
    enum (`unconfirmed`/`stale`/`healthy`/`unconfigured`) describes what *this
    browser session's own enrollment history* has confirmed — `EnrollmentPanel.tsx`
    never calls `GET /runners`, so it can only ever report what it personally
    enrolled or revoked. `FleetsPanel`'s roster is fed by real `GET /runners` data,
    which carries a different, genuinely-server-side field (`RunnerSummary.state`:
    `pending_enrollment`/`active`/`revoked`). Reusing `healthy`'s label/tone for
    merely `state === 'active'` would overclaim a connection check this component
    never performs — the opposite of the "never claim Healthy without genuine
    confirmation" discipline `RunnerHealthCard.tsx`'s own doc comment and adversarial
    test enforce. `runnerObservations.ts`'s `countOtherActiveRunners` doc comment
    makes the adjacent point explicitly: "staleness has no agreed threshold anywhere
    else in this tree, and inventing one here would be a hard-coded status this
    codebase's rules forbid." So the roster chip reuses the same `Badge` component
    and the same file (co-located with `STATUS_TONE`, mirroring the existing
    `typeBadgeTone` pattern in `shared/ui/TypeBadge.tsx`), colored/labeled by the
    real `state` value instead of forcing it through an enum built for a different,
    session-local claim.
  - **`RunnerFleetSection.test.tsx`'s "switches to the Fleets tab on click and loads
    fleets" test broke and was fixed, though the file is not in this card's Owns
    list.** `FleetsPanel` now issues a second `fetch` (`GET /runners`) alongside the
    existing `GET /runner-fleets` one; that test's `mockResolvedValue` returned the
    *same* `Response` object for both, and a `Response` body can only be read once —
    the second `.json()` call threw `Body is unusable: Body has already been read`,
    which left the fleets resource never settling and the assertion failing. Fixed
    by branching the mock on request URL (matching the pattern already used
    elsewhere in this same test file and in `FleetsPanel.test.tsx`), returning a
    fresh `Response` per call. The other three tests in that file stay on the
    default "Runners" tab, which mounts `EnrollmentPanel` (no `GET /runners` call on
    mount, per that file's own first test), so they were never affected.
  - **A stale comment exists elsewhere, left untouched.** `frontend/e2e/a11y.spec.ts`
    line ~1149 says "`agent_fleet_members` has no write route on any API surface" as
    the reason one test targets an exact runner instead of a fleet. That claim was
    already wrong before this card — the write routes existed in the backend since
    migration 041, just with no UI caller — and is more wrong now that a UI caller
    exists. The test's actual design choice (avoid fleet-membership setup, use an
    exact runner) still holds regardless; only the stated reason is stale. Not
    touched — outside this card's Owns list, and `frontend/e2e/**` a11y test
    rationale wasn't named in the read list.
  - Concurrent add/remove across different fleets in the same panel is serialized:
    `busyKey` is a single signal, so any in-flight add/remove disables every other
    add/remove button in the panel until it resolves. Deliberate, not measured as a
    UX cost — it trades a small amount of cross-fleet parallelism for avoiding any
    possibility of two in-flight roster mutations racing the same refetch.
- Secrets/logging review: n/a — no secret-bearing data in this card's surface
  (fleet id, runner id, runner name, runner state only).
- Safe merge order and likely conflicts: no shared files with VI-C41 or VI-C42 (the
  other two Wave 18 cards) — expect a clean merge. `docs/book/src/user-guide/
  agent-runners.md`'s "Known gaps" section is otherwise untouched by any other Wave
  18 card, so no conflict there either.
- Checklist: no unowned files carrying new behavior (the two extra files are a
  helper export and a test-mock fix, not new surface), no live secret, no panic
  stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| An operator can see which runners belong to a fleet from the UI | `FleetsPanel.test.tsx`: "derives the roster from fleet_ids: a runner in two fleets appears under both" |
| An operator can add a runner to a fleet from the UI | `FleetsPanel.test.tsx`: "add calls POST /api/runner-fleets/{fleet_id}/members with the runner id and refetches the roster"; `api.test.ts`: "addMember() POSTs to /api/runner-fleets/{fleet_id}/members with the runner id" |
| An operator can remove a runner from a fleet from the UI | `FleetsPanel.test.tsx`: "remove calls DELETE /api/runner-fleets/{fleet_id}/members/{runner_id} and refetches the roster"; `api.test.ts`: "removeMember() DELETEs /api/runner-fleets/{fleet_id}/members/{runner_id}" |
| The roster reflects the server's own state after a write, never an optimistic guess | both add/remove tests assert a `refetchRunners` round trip (a distinct mocked `GET /runners` response) before the DOM assertion, not a client-side splice |
| An empty fleet, and a fleet with no eligible runners to add, say so in plain words rather than showing nothing | `FleetsPanel.test.tsx`: "lists a fleet with its concurrency cap and an empty roster" asserts both "No members yet." and "No other runners available to add." |
| The roster UI is accessible, including against a real, freshly-created, zero-member fleet | `e2e/a11y.spec.ts` "agents page — creating a fleet via the form has no accessibility violations" — real server, real empty fleet, chromium, 0 violations |
| No backend change, no new endpoint | `git status --porcelain -- docs/openapi.json frontend/src/shared/api/schema.gen.ts` empty after the change; `pre-push`'s `npm run gen:api` diff clean |

A row with no evidence is a claim to delete, not a row to leave blank.

## Measured numbers

- `grep -c "  it(" frontend/src/features/agents/runnerFleet/FleetsPanel.test.tsx` → 7
  (was 4 before this card).
- `grep -c "  it(" frontend/src/shared/execution/api.test.ts` → 22 (was 20).
- `cd frontend && npx vitest run` → 856 passed, 0 failed (was 855 passed at the base
  SHA, before this card's tests were added — net +3 from the two files above plus
  no change elsewhere).
- `cd frontend && npx playwright test agents-page a11y --project=chromium
  --workers=2` → 43 passed, 0 failed, 1.7m wall time.

## What a stranger still cannot do

Add a runner to a fleet's roster from anywhere except the Fleets tab of the Agents
page's Advanced section — there is still no standalone "manage fleet membership"
page, and the roster editor only ever shows runners this same page's `GET /runners`
call already knows about (so a runner enrolled from a different machine's terminal
appears the next time this page's runner list refetches, not instantly). Trust a
runner's `Active` badge in the roster as a live connection check — it reflects the
server's stored `state` column only, with no heartbeat-staleness signal, by the same
"no agreed threshold" rule `runnerObservations.ts` already states for the rest of
this page.

## Surface-map delta

§VI.0's surface map did not carry a dedicated row for fleet-member management; it was
tracked instead as a §VI Known gaps bullet in `docs/book/src/user-guide/
agent-runners.md` ("`agent_fleet_members` has a write route; nothing in the UI calls
it yet"), which this card removes since the claim is now false. No surface-map table
row to update.

## Context spent

- Tokens read before the first edit (cold start): read the dispatch README header +
  Wave 18/VI-C43 block, the card's `TODO.md` section, `VI-D1.md`'s named section,
  `FleetsPanel.tsx` and its test whole, `api.ts`'s header + `fleetsApi`/`runnersApi`
  sections, `api.test.ts`'s matching describe blocks, the named `schema.gen.ts`
  greps, and `RunnerHealthCard.tsx` whole (186 lines — smaller than the ~12k budget
  implied, read whole rather than ranged since the file itself is short). Roughly in
  line with the block's ≈12k estimate; a modest overrun from also reading
  `RunnerFleetSection.tsx` (not named in the read list) to understand where
  `FleetsPanel` mounts and what "the Agents page already renders" meant concretely —
  needed to make the health-chip decision above, and worth recording since a future
  card touching this same area will hit the same question.
- Context size at handoff: well under the 120k ceiling.
- Files opened and not used (each one is a finding for the dispatch README):
  `RunnerFleetSection.tsx` was opened but not edited — read only to resolve the
  "health chip" ambiguity described in Known limitations above; the read list did
  not name it, and in hindsight it would be worth adding to VI-C43's read list for
  the next card that touches this seam.
- Read-list lines that were wrong (a range that missed, a size that was off): none —
  every named file/range was exactly where and what the block said.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*
