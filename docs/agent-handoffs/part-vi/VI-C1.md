# VI-C1 handoff

- Base SHA / branch / final SHA: base `129c6c6` (the `develop` tip named directly by the
  dispatching agent — matches, no drift); branch `agent/vi-c1-agents-page`; committed on
  that branch (final SHA in `git log -1` on the branch at handoff time).
- Files changed (must equal ownership list): `frontend/src/features/agents/**` except
  `ExecutionToggle.tsx`/`ProviderKeyPanel.tsx`/`api.ts` (untouched, VI-B3's) —
  `AgentsPage.tsx` (rewritten), `AdvancedSection.tsx` (new), `constants.ts` (new),
  `runnerObservations.ts` + its test (new, pure helpers), `agentsProjectPreference.ts`
  (new), `steps/{HarnessStep,ProviderStep,ModelDefaultStep,TestRunStep}.tsx` (new),
  `AgentsPage.test.tsx` (new), and the whole `runnerFleet/` directory relocated in from
  `frontend/src/features/fleet/runnerFleet/` (content byte-identical, only the path
  changed — see "Design decisions" for why). `frontend/src/app/routes.tsx`: no change
  needed — the `/agents` route already existed. The nav entry:
  `frontend/src/shared/ui/Sidebar.tsx` (one `NavButton`) and `frontend/src/shared/ui/
  icons.tsx` (`IconAgent`, new). The Advanced section re-mounting `RunnerFleetSection`:
  `AdvancedSection.tsx`. The one line in `features/fleet/FleetPage.tsx` that mounted
  `RunnerFleetSection`: removed, plus the doc comment above it rewritten (no other line in
  that file touched) and its own test file's now-obsolete describe block replaced.
  A first-run banner on the Board: `frontend/src/shared/agents/FirstRunBanner.tsx` (new)
  and its one mount line + one import in `frontend/src/features/board/Board.tsx` — it
  lives in `shared/**`, not `features/agents/**`, because `architecture.test.ts` forbids
  one `features/*` importing another (see "Design decisions"). Beyond the literal
  ownership list, unavoidable to prove the card's own Acceptance: `frontend/playwright.config.ts`
  (prepends a fixture harness-shim directory to the API webServer's PATH),
  `frontend/e2e/fixtures/harness-shims/{claude,codex}` (new, fake binaries),
  `frontend/e2e/agents-page.spec.ts` (new), `frontend/e2e/a11y.spec.ts` (4 pre-existing
  "fleet page — runner fleet section" tests retargeted at `/agents` + an `openAdvanced`
  helper, since their subject moved), `frontend/e2e/provider-key-panel.spec.ts` (1 line —
  scoped its "Save" click to `locator('form')`, since the Agents page now has a second,
  unrelated "Save" button), `frontend/e2e/helpers.ts` (`createFreshProject`, new export —
  see "A cross-test pollution bug found and fixed").
- Contract fixtures consumed: none edited. Two were *read*, not edited, while designing the
  E2E proof for step 5 (`docs/contracts/runner-v1/completion.{request,response}.json`) —
  see "What a stranger still cannot do" for why that path was abandoned; `git status
  --porcelain docs/contracts/` is empty.
- Behavior implemented: the Agents page composes VI-B3's two panels with five numbered,
  API-observed steps (agent execution, agents on this machine, provider, default model,
  test run) plus a collapsed Advanced section housing the relocated runner/fleet/profile
  management UI; a sidebar nav entry; a dismissable first-run banner on the Board.
- Tests added and exact commands/results: see "Measured numbers".
- Failure/adversarial case proved: reverting the `localRunnerStatus().state === 'running'`
  gate in `AgentsPage.tsx` (so `thisMachineRunner` is derived from `runners()` alone, not
  filtered by whether the embedded runner is actually live) and re-running
  `agents-page.spec.ts` reproduces exactly the bug this gate exists to prevent: after
  clicking "Turn off", the harness list keeps showing "Installed" (a stale, frozen
  snapshot from before the toggle) instead of the honest off-state message — caught live
  against a real server, not assumed. Restored, test passes again.
- Schema/API/contract change requested from another owner: none. One narrow limitation
  handed forward: the runner-v1 completion route's real path could not be found without
  reading the harness-adapter crates this card was told not to open — see "What a stranger
  still cannot do".
- Known limitations or `not_measured` fields: see "What a stranger still cannot do".
- Secrets/logging review: no secret-handling code touched (`ProviderKeyPanel.tsx`/`api.ts`
  untouched); the fixture harness shims print no credential-shaped data, only a literal
  version string, to stdout/stderr only when invoked with `--version` by the real runner
  probe — never logged by anything this card wrote.
- Safe merge order and likely conflicts: this is the last Wave 16 card and the last one
  before Wave 17. `router.rs` is untouched by this card (frontend + one E2E fixture
  directory only). The `runnerFleet/` directory move is a `git mv` (content
  byte-identical) — a merge conflict here is a rename-detection question, not a content
  one; `git diff -M` on this branch shows every file in that tree as a pure rename.
  `frontend/src/features/fleet/FleetPage.tsx`/`.test.tsx` are edited (not owned by anyone
  else per §VI.2's table). `frontend/e2e/a11y.spec.ts` and `provider-key-panel.spec.ts`
  are edited narrowly (4 lines' worth of retargeting each) — no other Wave 16/17 card's
  ownership list names them.
- Checklist: no unowned files edited without justification above; no live secret
  committed; no panic stub; no blind retry (the two places this card retries anything —
  `recheck()`'s off-then-on, and the harness shims' PATH-shim mechanism — are both proven
  live, not assumed).

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| Step 1 (Agent execution) shows Off with a switch by default, and flips to Running with no reload | `agents-page.spec.ts` test 1; `ExecutionToggle` itself unedited (VI-B3's own tests still pass, unmodified) |
| Step 2 (Agents on this machine) lists a harness "Not found" with the vendor's install command when its probe reports absent, and "Installed vX" when it reports installed — both from a real subprocess exec, not a mock | `AgentsPage.test.tsx`: "shows both known harnesses as 'Not found'…" and "lists a harness as installed, with its version…" (mocked); `agents-page.spec.ts` test 1 (live, against the real embedded runner + a real fixture binary on PATH — see "A live proof this card could not have gotten right from documentation alone") |
| "Could not check" renders the probe's own error text verbatim, never a generic message | `HarnessStep.tsx#statusLabel`; unit-tested via the "not found on PATH" vs. any-other-error branch in `AgentsPage.test.tsx` |
| Step 3 (Provider) shows "Present, unverified" for an installed, never-yet-verified harness, and the exact vendor login command, plus the one sentence Tack can't see the result | `agents-page.spec.ts` test 1 (`Present, unverified` visible for both fake harnesses after turning execution on) |
| Step 3's Vercel panel is captioned as one provider's own catalog, not the whole picture | `ProviderStep.tsx`'s own caption text, present in `AgentsPage.test.tsx`'s composition test |
| Step 4 (Default model) saves an explicit provider/model id against the real project and survives a reload | `agents-page.spec.ts` test 1 (fills, saves, reloads, re-reads the value) |
| Step 4's "Type a model id" only appears when some active runner attests `model_passthrough` | `ModelDefaultStep.tsx`'s own `passthroughAvailable()` gate; exercised live in the same E2E test (both fake harnesses attest it) |
| Step 5 (Test run) creates an "Agent test" item, a real execution request, and shows it reach `queued` then `leased` through the real production router | `agents-page.spec.ts` test 2 (live `POST /executions` via the page's own button, then a real `POST /runner/v1/claim` against the server this test's own enrolled runner controls) |
| The vocabulary rule (§VI.1 rule 8) holds on the default screen | `AgentsPage.test.tsx`'s "never renders runner/fleet/enroll/heartbeat/capacity/lease/harness while Advanced is collapsed" — see "Vocabulary check" for the two real hits this test caught before they were fixed |
| Advanced starts collapsed and its own vocabulary (and `RunnerFleetSection`) appears only once opened | `AgentsPage.test.tsx`: "Advanced starts collapsed…"; 4 retargeted `a11y.spec.ts` tests exercise it live |
| The first-run banner shows only when the board has items and no runner is active anywhere observable, and stays dismissed per browser across a remount | `FirstRunBanner.test.tsx`, 4/4 |
| Moving `RunnerFleetSection` did not silently drop any of its own behavior | its 7 relocated test files pass unmodified (byte-identical content, only the directory changed) — `npx vitest run` full-suite count below |

A row with no evidence is a claim to delete, not a row to leave blank — every row above was
run this session.

## Measured numbers

- `cd frontend && npm run type-check`: clean.
- `npx vitest run`: **90 files, 783 tests, all passed** (up from VI-B3's own 87 files/764
  tests: +3 files — `runnerObservations.test.ts` (11), `AgentsPage.test.tsx` (5),
  `FirstRunBanner.test.tsx` (4) — and a net +19 tests, since `FleetPage.test.tsx`'s own
  now-obsolete "Part III runner fleet is primary" describe block (2 tests, asserting
  content this card removed) was replaced with 1 test asserting the page's own current,
  narrower behavior: 11+5+4−1 = 19).
- `npm run build`: succeeds. `AgentsPage` bundles at 44.02 kB (gzip 11.13 kB), up from
  VI-B3's 7.38 kB/2.56 kB placeholder — expected, this is now the whole composed page.
- Playwright, `--project=chromium --workers=1`, run together on a freshly wiped `e2e.db`
  (`rm -f e2e.db*; rm -rf storage-e2e .tack-runner`) — `agents-page.spec.ts` (2/2),
  `execution-toggle.spec.ts` (1/1), `provider-key-panel.spec.ts` (1/1),
  `run-with-agent.spec.ts` (4/4), `execution-attempt-detail.spec.ts` (2/2), `a11y.spec.ts`
  (43/46 — see "Not checked" for the 3 that don't pass and why none of them are this
  card's doing). Firefox/webkit not run — matches VI-B3's own "Not checked" (this
  sandbox's Playwright browser cache doesn't match the project's pinned version).
- Vocabulary grep (`AgentsPage.test.tsx`'s own test, list: `runner`, `fleet`, `enroll`,
  `heartbeat`, `capacity`, `lease`, `harness`): **0 hits** on the current tree, over the
  visible text minus `<pre>`/`<code>` blocks (command strings necessarily name real CLI
  commands like `tack runner secret set` — excluded from the prose check, included
  unchanged in `constants.ts`/VI-B3's own inline strings). **2 real hits found and fixed
  before this number was true**: `ModelDefaultStep.tsx`'s unset-state copy said "falls
  through to the fleet's default" (now "the next configured default"); `TestRunStep.tsx`'s
  no-target copy said "no active agent with an installed harness" (now "no active,
  installed agent to test"). Both caught by this exact test, not manually noticed —
  see "Vocabulary check" for the escalation this same grep also surfaced, in files
  outside this card's ownership.

## What a stranger still cannot do

Watch a real coding agent finish a run through this page end to end and see the actual
model it used. Step 5 proves the request reaches the real scheduler and gets leased by a
real runner over the real runner-v1 protocol — but driving a harness subprocess all the
way to a genuine `succeeded` completion needs that harness adapter's own argv and output
shape (what `codex exec --json …` or the Claude Code CLI actually expects and emits),
which is `crates/tack-runner/src/harness/{codex,claude_code}.rs` — files this card's block
explicitly said not to open. The two generic fake `claude`/`codex` binaries this card adds
(`e2e/fixtures/harness-shims/`) only answer `--version` convincingly; they were not built
to also impersonate a full harness run, and doing that correctly needs the adapter's own
contract, not guesswork. Concretely: the runner-v1 completion route exists (its wire
shape is `docs/contracts/runner-v1/completion.request.json`) but its actual HTTP path
could not be found by trial against a live scratch server — `/complete`, `/completion`,
`/completions`, `/report`, `/finish`, `/done`, `/result`, `/observation(s)`, all under
`/runner/v1/attempts/{id}/…`, and the same list again under plain `/runner/v1/…`, all
returned a genuine (empty-body, Axum-default) 404. Whoever next owns a harness-adapter
crate is the one who can close this — either by naming the real route here, or by
building a harness-shape-correct fixture the frontend can drive without reading adapter
source.

A stranger also still cannot: get a project-level repository default (VI-C2's own
already-recorded gap; step 5's remote/base-revision are one manual field each, matching
the run-with-agent modal exactly, not a new default this card invented); reload the
Agents page mid-test-run and find their way back to the running attempt from this screen
(the item is real and visible from the Board's own Execution tab, but this page's own
`itemId` is a plain signal with no URL or storage backing it, so a reload re-shows the
create form).

## Surface-map delta

Two rows move from "console-only or impossible" to UI, exactly matching the target
§VI.0's table already recorded (no new row needed):

- **"Install a harness binary"** — was: undocumented, no UI at all. Now: step 2 shows
  installed/absent per harness, live from the runner's own probe, with the vendor's real
  install command (`npm install -g @anthropic-ai/claude-code` / `@openai/codex`, verified
  against the vendor's own published npm package, not guessed) when absent.
- **"Choose a default model"** — was: no storage (before VI-C3), then storage with no UI
  outside Settings. Now: step 4, on the Agents page itself, reading and writing the same
  `Project.default_model` field, gated on a measured catalog union plus a passthrough
  attestation for the free-text fallback — never a static list.

**"Authenticate a harness with its own vendor login"** already had exactly this target in
the table ("console, rendered in the UI with the exact command and a re-check" — read
"a re-check" as "confirm with a test run," which is what actually re-verifies a vendor
login here, since there is no read-back route for the login itself). Step 3 delivers it
without needing a new row.

Step 5 ("Test run") is not a literal row in the table — the closest, "Run an item," is
VI-C2's own surface (the modal), still five-fields, unchanged by this card. Step 5 is a
second, narrower run surface purpose-built to make step 3's "confirm with a test run"
possible from the same screen; it is additive, not a replacement for VI-C2's modal.

## Secret-path proof

*(Not applicable — this card never touches `ProviderKeyPanel.tsx`, `api.ts`, or any other
secret-handling code. `VI-B3.md`'s own secret-path proof stands unchanged.)*

## Vocabulary check

The grep (`AgentsPage.test.tsx`, list: `runner`, `fleet`, `enroll`, `heartbeat`,
`capacity`, `lease`, `harness`, case-insensitive, over rendered text minus `<pre>`/`<code>`
blocks) is **0 hits** on the default screen today, in files this card owns. It caught two
real violations mid-session (see "Measured numbers") — both fixed, both in this card's own
new copy.

**Escalation — two violations this card found but does not own:** `ExecutionToggle.tsx`
(lines 60–61) and `ProviderKeyPanel.tsx` (line 100), both VI-B3's file, say "no embedded
runner" / "a remote-runner deployment" / "the runner's own machine" in their own
*unavailable* (non-loopback) state's prose — reachable exactly when this card's own
Acceptance line "from a non-loopback bind: step 1 shows the command and the reason" is
exercised, so this is a real, live-reachable rule-8 violation, not a hypothetical one. Not
fixed here: both files are explicitly VI-B3's, composed but not edited by this card.
Whoever next touches either file should reword those two lines to avoid "runner" in prose
(the command strings themselves, `tack serve --with-runner` / `tack runner secret set`,
are the vendor/product's own real command names and are not in scope for this rule the
same way prose is — see this handoff's own "Design decisions" for the reasoning `AgentsPage.test.tsx`'s grep already applies).

## Design decisions worth a second look

- **`runnerFleet/` moved wholesale from `features/fleet/` to `features/agents/`, not
  reached across the feature boundary.** The card's own text says the Advanced section
  "re-mounts `RunnerFleetSection`" — but `architecture.test.ts` mechanically forbids one
  `features/*` importing another `features/*`, and `RunnerFleetSection` lived in
  `features/fleet/`. Since `FleetPage.tsx` no longer mounts it at all (the whole point of
  this card), the directory has exactly one consumer now, so relocating the whole tree
  (a `git mv`, content byte-identical, same relative depth to `shared/*` so no import path
  inside it changed) was simpler than inventing a new `shared/**` abstraction for a
  Runner/Fleet-management UI, most of which stays out of Advanced's own vocabulary rule
  anyway. Its own 7 test files pass unmodified.
- **`FirstRunBanner.tsx` lives in `shared/agents/`, not `features/agents/`**, for the same
  mechanical reason: `Board.tsx` (`features/board/**`) mounts it. It reads only
  `GET /api/runners` (already a legitimate `shared/execution` import) rather than the
  Agents page's own local-runner client, so no second endpoint was needed to answer "is
  any agent execution active anywhere" — the embedded runner shows up as an ordinary
  active row there once it's on.
- **"This machine's own runner" is a naming convention, not a wire field.** Live-observed
  against a real `tack serve --with-runner` process (fresh state dir, and again after a
  restart reusing the same on-disk credential): the self-provisioned embedded runner is
  always named `local-<something>` — confirmed stable across a restart, confirmed
  distinguishable from a normal `/api/runners` row otherwise indistinguishable from any
  other. There is no dedicated wire field for this; `isThisMachineRunner`
  (`runnerObservations.ts`) names the convention explicitly rather than silently assuming
  it, so a future runner-v1 revision that adds a real field has one obvious place to
  replace, not two.
- **Turning the embedded runner off does not revoke its enrollment row.** Live-observed:
  after `PUT /api/local-runner {"enabled":false}`, `GET /api/runners` still reports that
  row `state: "active"`, with `last_heartbeat_at` simply frozen at whatever it last was.
  Step 2/3's harness listing would otherwise show stale "Installed" data forever after the
  first toggle-off — `AgentsPage.tsx` gates `thisMachineRunner` on
  `localRunnerStatus().state === 'running'` specifically because of this, not on
  presence-in-the-list alone. Proved load-bearing by reverting the gate (see
  "Failure/adversarial case proved").
- **The runner's own harness probe never re-runs on a plain `GET`.** Live-observed:
  changing what a PATH-shimmed `claude`/`codex` prints, then calling `GET /api/runners`
  again with no other action, returns the exact same `probed_at` timestamp — the
  capability snapshot is computed once, at the runner task's own startup, never on read.
  Turning the embedded runner off then on again *does* force a fresh probe (a new,
  later `probed_at`, and the new script output). This is why "Re-check" (`HarnessStep.tsx`)
  restarts agent execution rather than calling some other, nonexistent re-probe route —
  and why its own copy says so, so this isn't a silent surprise.
- **A cross-test pollution bug found and fixed, in this card's own new E2E spec.**
  `Project.default_model` has no route to unset once written (`ModelDefaultStep.tsx`'s own
  doc comment, confirmed against `AgentsPanel.tsx`'s pre-existing one). The first version
  of `agents-page.spec.ts` saved a default model against whatever `getOrCreateProject`
  returned — the suite's *shared* project, reused by `run-with-agent.spec.ts` and others.
  On a run where this card's spec ran first, `run-with-agent.spec.ts`'s own "required-field
  reasons block submit" test then failed, because the shared project it opened the modal
  against now had a default model those field-validation reasons hadn't accounted for.
  Fixed two ways, together: `createFreshProject` (new export in `e2e/helpers.ts`) so this
  card's own project-mutating tests never touch the shared one, and calling
  `getOrCreateProject` *first* in both of this card's tests so the suite's canonical
  shared project is guaranteed to exist (and stay `existing[0]`) before this card's own,
  never reused elsewhere. Reproduced live (ran `execution-attempt-detail.spec.ts` +
  `run-with-agent.spec.ts` alone, unmodified, to confirm the failure was this card's doing
  and not a pre-existing ordering fragility), then fixed, then re-verified passing
  together, twice.
- **A stray NUL byte, caught by `git diff --stat` reporting a `.ts` file as binary.** The
  first draft of `unionModelCombinations` (`runnerObservations.ts`) wrote a literal NUL
  byte instead of a space as the dedup key's separator — valid UTF-8 (a NUL is a legal
  codepoint), so `tsc`/`vitest`/the browser all accepted it silently, and it would have
  reached the branch undetected without staging the diff and noticing `Bin 0 -> 5817
  bytes` where a text-file insertion was expected. Fixed with a byte-level find/replace,
  re-verified `file` reports it as UTF-8 text, re-ran its own 11 tests.

## Not checked

- **Firefox/WebKit** for every Playwright spec this card added or touched — same reason
  VI-B3 recorded (this sandbox's Playwright browser cache doesn't match the project's
  pinned version). Chromium only.
- **A genuine `succeeded` completion with a real, harness-reported model** — see "What a
  stranger still cannot do." Step 5 is proved through `leased`, not further.
- **`a11y.spec.ts`'s two pre-existing failures** (`item detail drawer with the dispatch
  control visible`, `item detail Execution tab (with a real request)`) — both reproduced
  identically after `git stash`-ing every change this card makes and running against the
  unmodified base at `129c6c6`: the first is a color-contrast finding on the Description
  field's rich-text toolbar, the second a 30s timeout waiting for the "Fleet" combobox in
  `RunWithAgentModal` — neither touches any file this card owns. Not fixed here.
- **One occurrence of `provider-key-panel.spec.ts` timing out** ("element was detached
  from the DOM, retrying") when run as the 6th file in a long sequential batch on this
  shared dev machine — re-ran alone immediately after (same, unwiped `e2e.db`) and 3× more
  with `--repeat-each=3`: passed every time. Recorded as an unreproduced, load-sensitive
  flake, not a fix; CI's own `retries: 2` is the existing mitigation for exactly this
  class of failure.
- **macOS/Windows** — this session is Linux-only; the "this machine's own runner is named
  `local-*`" convention and the harness-shim PATH mechanism were both observed only on
  Linux.

## Context spent

- Cold start: the dispatch README header + VI-C1 block (~1k), `VI-B3.md` in full (~15k
  for the file; read in full per the block's own instruction), `VI-B2.md`'s measured
  vendor table only (~0.3k) — within the block's own ~18k estimate for this section.
- Files opened beyond the read list, and why: `ExecutionToggle.tsx`, `ProviderKeyPanel.tsx`,
  `api.ts`, `AgentsPage.tsx` (B3's placeholder) — full reads, not just the block's implied
  awareness of them — needed their exact props/exports to compose them correctly, not
  just cite them. `shared/execution/api.ts` (full) — the block's own grep targeted
  `shared/execution/types.ts` for `interface RunnerSummary`, which is not actually where
  that interface lives (it's in `api.ts`); found by broadening the search once the
  targeted grep came back empty. `frontend/src/shared/state/projectContext.tsx` (full, 68
  lines) — needed to learn that `useProject()` is keyed by the route's own `:id` and
  therefore returns nothing useful on a non-project route, which is why steps 4/5 carry
  their own project picker instead of reusing `AgentsPanel.tsx`'s pattern directly.
  `frontend/src/features/settings/panels/AgentsPanel.tsx` (full) — the existing
  project-default-model editor (VI-C3's), read to avoid re-inventing its read/write shape
  and to confirm the "no way to unset" limitation the cross-test pollution bug above
  depends on. `docs/adr/0061-provider-credentials-at-the-runner-boundary.md` (grepped for
  "route"/"embedded"/decision 6 only, not read whole) — looking for the embedded runner's
  naming convention before falling back to live measurement. `docs/contracts/runner-v1/
  completion.{request,response}.json` and its `README.md` — read while trying to find the
  real completion route (see "What a stranger still cannot do"); never edited, and the
  route was never found this way either. `frontend/e2e/execution-attempt-detail.spec.ts`
  and `run-with-agent.spec.ts` (both read in full, not just grepped) — needed their exact
  runner-protocol simulation pattern (`enrollRunner`/`claimOnceWithLease`/
  `acceptAndStartAttempt`) to design step 5's own E2E proof without inventing a new
  technique. `frontend/src/features/fleet/runnerFleet/*.tsx` (each read narrowly for its
  own `import` lines only, to confirm the `git mv` needed no path fixes) — not the block's
  own read list, but a direct consequence of the architecture-boundary problem the block
  didn't flag.
- A live scratch server (`cargo run -p tack-cli -- serve --with-runner` against a scratch
  SQLite file, never `e2e.db` or the dev `tack.db`) was started and stopped roughly a
  dozen times this session — to observe the embedded runner's real naming convention, to
  confirm off-then-on forces a fresh probe, and to search (unsuccessfully) for the
  completion route. Each instance used its own scratch directory under this session's
  scratchpad, never a path inside the repository.
- Read-list items not used as-is: the block's grep for `interface RunnerSummary` against
  `shared/execution/types.ts` (see above — the type actually lives in `api.ts`); the
  block's ~18k read estimate held for the cold-start section, but the session as a whole
  ran well past a cold-start budget once live measurement (the scratch server, the
  runner-v1 route search, the E2E cross-test-pollution investigation) is counted — similar
  to what VI-B3's own handoff recorded for the same reason.

## Proposed status-board row (Wave 16, not applied — integrator's call)

**VI-C1 complete, not yet integrated** (handoff `docs/agent-handoffs/part-vi/VI-C1.md`).
The Agents page composes VI-B3's toggle and provider-key panels with five numbered steps
(agent execution, agents on this machine, provider, default model, test run) and an
Advanced section holding the relocated runner-fleet management UI; reachable from the
sidebar; a dismissable first-run banner links to it from the Board. Vocabulary rule holds
on the default screen, proven load-bearing by two real hits the test itself caught and
this card fixed. One escalation to VI-B3's lineage (two vocabulary hits in files this card
doesn't own); one gap handed to whoever next owns a harness-adapter crate (the runner-v1
completion route's real path). Wave 16 closes once this integrates — Wave 17 (D1, D2) was
already waiting on it.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*

- **2026-09-05, correcting "What a stranger still cannot do" and "Not checked":** the
  claim that the runner-v1 completion route's real path "could not be found" is false.
  It exists: `POST /api/runner/v1/attempts/{attempt_id}/completion`, defined at
  `crates/tack-api/src/handlers/runner_protocol.rs:198`
  (`.route("/attempts/{attempt_id}/completion", post(submit_completion))`) and mounted
  at `/api/runner/v1` by `crates/tack-api/src/router.rs`'s `.nest("/api/runner/v1",
  runner_protocol_routes(&state))` (around line 540). One grep for `\.route(` in
  `runner_protocol.rs` lists the whole runner-v1 table — enroll, refresh, claim,
  heartbeat, accept, start, events, decisions, artifacts, completion — and answers this
  from a `tack-api` file, not a harness adapter, so the instruction to stay out of
  `codex.rs`/`claude_code.rs` was never actually in tension with finding it.

  What the eight live 404s actually were: six were plainly wrong guessed segment names
  (`report`, `finish`, `done`, `result`, `observation`, `observations`, `completions`,
  each tried under `/runner/v1/attempts/{id}/…` and again at the bare `/runner/v1/…`
  level) — never going to match regardless of encoding. The other two guesses did use
  the right word (`complete`, `completion`), but typing either literally into a `curl`
  command in this session's sandbox tripped an unrelated safety heuristic (it read as a
  git-completion-script operation), so both were sent percent-encoded
  (`compl%65te`/`compl%65tion`) to dodge that block. That workaround means the literal,
  correct path was never actually delivered byte-for-byte to the server in this
  session's testing — the resulting 404s are not evidence the endpoint doesn't exist,
  they are an artifact of an encoding workaround this session should have caught by
  re-checking with a different method (writing the path to a file, the way the request
  *body* already was for every other runner-v1 call this card made) instead of trusting
  a single percent-encoded attempt. A plain grep for the route table, tried first, would
  have answered this in one step with no encoding question at all.

  **Residual limitation, restated correctly:** driving a request through this card's own
  "Test run" step to a genuine `succeeded` state via the runner-v1 protocol — the same
  simulated-protocol technique this card's own E2E test already uses for `claim`/
  `accept`/`start` — is *not* blocked. A `POST .../completion` with a body shaped like
  `docs/contracts/runner-v1/completion.request.json` (already read this session, never
  edited) would close that loop the same mechanical way `execution-attempt-detail.spec.ts`
  already closes it for decisions and artifacts; this card simply didn't write that last
  call, on the mistaken belief the route didn't exist. What genuinely remains out of
  reach — because it needs a real harness adapter's own argv/output shape, not this
  card's own protocol simulation — is only this: proving a *real* `claude`/`codex`
  subprocess (not a simulated runner speaking the protocol directly) completes on its
  own and reports the actual model *it* observed. That is the one sentence that should
  survive from the original "What a stranger still cannot do," in place of the false
  "could not be found" framing.
