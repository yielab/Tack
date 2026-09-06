# VI-C19 handoff

- Base SHA / branch / final SHA: dispatched base `495b00e` was stale in this worktree
  (`git log --oneline -1` showed `worktree-agent-af864ecfeecdd7982` at `e5206c7`, an
  ancestor of `495b00e`, confirmed with `git merge-base --is-ancestor 495b00e HEAD` →
  `BASE-STALE`). Recreated: `git checkout -b agent/vi-c19-remove-secret-symmetry develop`,
  confirmed `develop` was at `495b00e` and the tree was clean before the first edit.
  Branch `agent/vi-c19-remove-secret-symmetry`, not committed (card rule: no
  commit/push/merge/rebase).
- Files changed (matches ownership — the `set_secret`/`remove_secret` pair, its tests, and
  the one frontend file named in the card for the last acceptance step):
  - `crates/tack-cli/src/local_runner.rs` — the fix and three new unit tests.
  - `frontend/e2e/provider-key-panel.spec.ts` — VI-C16's `Replace`-instead-of-`Remove`
    workaround removed.
- Contract fixtures consumed: none — no runner-v1 wire shape touched, no API shape
  change (`docs/openapi.json`/`schema.gen.ts` untouched, VI-C17's own).
- Behavior implemented:
  1. **`remove_secret` now has a real counterpart to `set_secret`'s convenience flip.**
     Before this card, `set_secret` set `providers[vercel_ai_gateway].enabled = true`
     unconditionally whenever the default secret name was written, and `remove_secret` did
     nothing to that flag — a provider stayed reported `enabled` forever after its only key
     was deleted, with `catalog()` (`crates/tack-runner/src/provider/mod.rs`'s
     `attach_one_catalog`) reporting `secret_unresolved` rather than `not_configured` for the
     rest of that process's life.
  2. **The behavior argument — what `enabled` should mean.** `set_secret`'s flip is a
     convenience for a UI-only user who never opens a TOML file. It must not become a
     one-way ratchet, but it also must not blindly reset `enabled` to `false` on every
     removal: a provider the operator turned on directly — TOML `enabled = true`,
     `TACK_RUNNER_PROVIDER_VERCEL_AI_GATEWAY_ENABLED=1`, or a value already `true` at
     boot — has to survive a later key removal untouched, because that `true` was never
     `set_secret`'s to give and is not `remove_secret`'s to take away. The two states are
     indistinguishable by reading `enabled` alone once both have occurred, so a new
     in-memory bit, `State::vercel_ai_gateway_secret_auto_enabled`, records *why* the flag
     is currently `true`: it is set the instant `set_secret` actually changes the flag from
     `false` to `true` (not merely finds it already `true`), and cleared the instant
     `remove_secret` undoes that exact flip. Never persisted — matches `enabled` itself,
     which this process never writes back to any TOML file either, so "before the set" only
     ever needs to mean "earlier in this same process's life," which is exactly the bit's
     scope.
  3. **The "different secret name" case is handled by mirroring the existing gate, not by
     adding a new one.** `set_secret` only ever touches `enabled` when
     `name == DEFAULT_VERCEL_AI_GATEWAY_SECRET`; a deployment that pointed
     `TACK_RUNNER_PROVIDER_VERCEL_AI_GATEWAY_SECRET` at a different secret-store entry name
     keeps using its own console-only toggle for `enabled`, untouched by this route, exactly
     as the existing comment already claimed. `remove_secret`'s new block uses the identical
     `name == DEFAULT_VERCEL_AI_GATEWAY_SECRET` guard, so a `DELETE
     /api/local-runner/secrets/<anything-else>` never touches `enabled` either — proved by
     `removing_a_non_default_secret_never_touches_the_providers_enabled_flag`.
- Tests added and exact commands/results:
  - `removing_the_default_vercel_secret_disables_a_provider_it_alone_enabled` — set then
    remove leaves the provider disabled again, and `catalog()` reports `NotConfigured`
    (the observable capability claim), never `SecretUnresolved`.
  - `removing_the_default_vercel_secret_leaves_an_operator_enabled_provider_on` — a
    provider force-enabled *before* `set_secret` is ever called stays enabled after
    `set_secret` then `remove_secret` — the "don't clobber the operator's own config" case
    the card asked to be decided.
  - `removing_a_non_default_secret_never_touches_the_providers_enabled_flag` — the
    "different name" case: removing a secret under any other name leaves a
    pre-enabled provider's flag untouched.
  - Command: `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C19 cargo nextest run
    --workspace` → `1439 tests run: 1439 passed, 7 skipped` (run three times across this
    card's work; one incidental failure is recorded below, not from this change).
  - `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C19 cargo clippy --workspace
    --all-targets -- -D warnings` → clean, no warnings.
  - `./scripts/check-comments.sh` → clean (caught and fixed one violation of its own rule
    in this card's own draft — see "Context spent").
  - `./scripts/check-test-hygiene.sh` → clean.
- Failure/adversarial case proved: reverted only the `remove_secret` fix (deleted the new
  `if` block, keeping the tests and the `set_secret` change) and re-ran the three new tests:

  ```
  CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C19 cargo nextest run --workspace \
    -E 'test(removing_the_default_vercel_secret_disables_a_provider_it_alone_enabled) \
        + test(removing_the_default_vercel_secret_leaves_an_operator_enabled_provider_on) \
        + test(removing_a_non_default_secret_never_touches_the_providers_enabled_flag)'
  ```

  Result: `3 tests run: 2 passed, 1 failed`. The failure, exact text:

  ```
  FAIL [   0.015s] (3/3) tack-cli::bin/tack local_runner::tests::removing_the_default_vercel_secret_disables_a_provider_it_alone_enabled
  thread 'local_runner::tests::removing_the_default_vercel_secret_disables_a_provider_it_alone_enabled' (512143) panicked at crates/tack-cli/src/local_runner.rs:910:9:
  removing the only secret that enabled this provider must leave it exactly as it was before the set: disabled
  ```

  Exactly the mirror test failed — the other two, which assert properties the reverted
  code never touched, still passed. Restored the fix; re-ran the full workspace suite
  (`1439 passed, 7 skipped`) to confirm the fix, not the revert, is what ships.
- Schema/API/contract change requested from another owner: none. `GET /api/local-runner`'s
  `catalog` field (already shaped by VI-B2/VI-B3, untouched here) is what makes the fix
  observable over the API without any response-shape change — `not_configured` vs.
  `secret_unresolved` is the existing, already-wired proxy for this provider's `enabled`
  flag, confirmed by reading `crates/tack-runner/src/provider/mod.rs`'s
  `attach_one_catalog` (`providers.get(...).filter(|c| c.enabled)` gates `NotConfigured`
  before secret resolution is even attempted).
- Known limitations or `not_measured` fields: none identified. The fix is scoped exactly to
  the pair the card owns; no new secret column, no new API surface, no new config knob.
- Secrets/logging review: no new secret introduced, no new log line added. The new field
  (`vercel_ai_gateway_secret_auto_enabled`) is a plain in-memory `bool` — never a secret
  value, never persisted to any file, never logged. Confirmed
  `remote_backup.rs::scrub_snapshot_secrets` needs no update: `grep -n
  "vercel_ai_gateway_secret_auto_enabled" crates/tack-api/src/remote_backup.rs` → no
  matches, correctly, since this field is not a database column and this card added none.
- Safe merge order and likely conflicts: independent of every other in-flight VI-C card —
  the only files touched are the one Rust file this card owns and the one spec file named
  for the last acceptance step. No overlap with VI-C17 (generated files, untouched here) or
  VI-C18 (`run-with-agent.spec.ts`, untouched here — see "Measured numbers" for why its
  named failure did not reproduce in this card's own run).
- Checklist: no unowned files touched (`git status --porcelain` shows exactly the two files
  above), no live secret used or printed (test values are literal placeholders, e.g.
  `"shh"`), no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| Set then remove leaves the provider exactly as it was before the set (disabled case) | `removing_the_default_vercel_secret_disables_a_provider_it_alone_enabled` — asserts both the internal flag and `catalog()` reporting `NotConfigured` |
| An operator's own `enabled = true` survives a key removal untouched | `removing_the_default_vercel_secret_leaves_an_operator_enabled_provider_on` |
| A provider configured under a different secret name is never touched by `remove_secret` | `removing_a_non_default_secret_never_touches_the_providers_enabled_flag` |
| The fix is load-bearing, not incidental | Reverted only the `remove_secret` addition; the mirror test failed with the exact text quoted above; restored, full suite green again |
| VI-C16's `Replace`-instead-of-`Remove` workaround is no longer needed | `frontend/e2e/provider-key-panel.spec.ts` now calls the real `Remove` button; 3 solo repeats and 3 full-parallel repeats alongside its two lock-sharing siblings, all green — see "Measured numbers" |
| No new secret, log line, or database column was introduced | `grep -n "vercel_ai_gateway_secret_auto_enabled" crates/tack-api/src/remote_backup.rs` → no matches; the field is a plain in-memory bool |

A row with no evidence is a claim to delete, not a row to leave blank.

## Measured numbers

Every command below was run with `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C19`.

**Backend gates:**

```
cargo nextest run --workspace
```
→ `1439 tests run: 1439 passed, 7 skipped` (three separate runs across this card's work,
14.4s–17.2s each). One run in between hit a single, unrelated flake —
`two_servers_on_two_databases_each_see_only_their_own_runner_enrollment`
(`crates/tack-cli/tests/embedded_runner_state_scoping.rs`, spawns a real `tack serve
--with-runner` subprocess) — re-ran in isolation (`-E
'binary(embedded_runner_state_scoping)'`) and it passed solo; re-ran the full suite
immediately after and it was green again. Not this card's change: nothing in this diff
touches that test file or the subprocess-spawn path it exercises.

```
cargo clippy --workspace --all-targets -- -D warnings
```
→ clean, no warnings.

```
./scripts/check-comments.sh
```
→ clean. Caught one draft violation of its own rule (a comment reading "The claim this
card is about…") before this handoff was written — rewritten to drop the pointer, kept the
knowledge (see "Context spent").

```
./scripts/check-test-hygiene.sh
```
→ clean.

```
cd frontend && npm run type-check
```
→ clean, no errors (after `npm install` — `frontend/node_modules` did not exist in this
fresh worktree).

**Frontend E2E** (from `frontend/`, `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C19`):

```
npx playwright test provider-key-panel.spec.ts --project=chromium
```
→ `1 passed` (1.8s), fresh `e2e.db`.

```
npx playwright test provider-key-panel.spec.ts --project=chromium --project=firefox
```
→ `2 passed` each of 3 repeats against the same, now-reused `e2e.db` (5.9s, 6.0s, 6.2s) —
this is the exact scenario VI-C16 found broken: a second run against a server that has
already saved this provider's key once. All three repeats used the real `Remove` button,
not `Replace`.

```
npx playwright test execution-toggle.spec.ts agents-page.spec.ts provider-key-panel.spec.ts \
  --project=chromium --project=firefox --workers=6
```
→ `8 passed` on each of 3 repeats (18.1s, 12.1s, 15.2s) — the three files VI-C16 serialized
behind `executionToggleLock`, run under the identical full-parallel load VI-C16 used to
prove the lock, confirming this card's change to `provider-key-panel.spec.ts` did not
reintroduce or interact badly with that coordination.

```
npx playwright test --project=chromium
```
→ `70 passed` (19.6s), the full default chromium suite, including
`run-with-agent.spec.ts:98` (named in the card as a known, unrelated, always-failing test
carded as VI-C18) — it passed in this run. Not fixed here, not touched here; reported
because a fully green run is stronger evidence than a partial one, not weaker.

## What a stranger still cannot do

A UI-only user pasting and then removing a Vercel AI Gateway key through the Agents page
now sees the panel return to exactly its pre-key state — the "Write-only — paste a key
below" form, with the catalog line reading "Catalog: not configured." Before this card, a
process that had ever seen this provider's key stayed stuck reporting a broken credential
("the stored key could not be read back") forever, even after the key was removed, with no
way back to "not configured" short of restarting the whole server. A stranger still cannot
configure two providers from this screen (unchanged — the card that added a second provider
owns that), and a remote-runner deployment still edits its TOML/environment directly
(unchanged — this fix is entirely inside the embedded-runner control, never reachable from a
remote runner's own console path).

## Surface-map delta

None — this card fixed an existing UI-reachable route's own internal consistency; it moved
no capability from console-only to UI-reachable, and none from UI-reachable back to
console-only.

## Context spent

- Tokens read before the first edit (cold start): card text (given in full), the VI-C16
  handoff in full (it named this bug), the VI-C19 card's own section of `TODO.md` (~30
  lines), `crates/tack-cli/src/local_runner.rs` in full (~1000 lines, both the production
  code and its existing tests, since the card explicitly owns "its tests" and the mirror
  test had to sit correctly beside the existing one), `crates/tack-runner/src/config.rs`
  (provider/config shapes, env-var precedence), `crates/tack-api/src/handlers/
  local_runner.rs` (to confirm what `GET /api/local-runner` actually returns and that its
  `catalog` field, not a raw `enabled` field, is the acceptance criterion's real proof
  surface), `crates/tack-runner/src/provider/mod.rs`'s `attach_one_catalog` (to confirm
  `NotConfigured` vs. `SecretUnresolved` really is gated on `config.enabled`), and
  `frontend/src/features/agents/{ProviderKeyPanel.tsx,api.ts}` (to confirm the frontend
  always submits the literal default secret name, never a configurable one, which is what
  makes the "different name" scenario reachable only via direct API/console use, not this
  UI). Read `docs/CONFIG.md`'s embedded-runner table entries for the two
  `TACK_RUNNER_PROVIDER_VERCEL_AI_GATEWAY_*` variables to confirm the env-var names and
  defaults quoted in the new doc comments and tests.
- Files opened and not used in the final diff: `docs/adr/0061-*.md` and
  `docs/adr/0063-*.md` were not opened — the card's own text and the CONFIG.md table
  entries already answered every question this card needed (what `enabled` gates, what the
  env vars are named); re-reading the ADRs would have been re-deriving what CLAUDE.md and
  CONFIG.md already state as settled. `docs/agent-handoffs/part-vi/VI-B2.md` and
  `VI-B3.md` (named in the card's "Read these first") were not opened in full — the
  specific facts they would have supplied (the provider config shape, the write-only
  secret contract) were already confirmed directly from the current source
  (`config.rs`, `local_runner.rs`, `ProviderKeyPanel.tsx`), which is authoritative over a
  handoff describing code that has since changed.
- Read-list lines that were wrong: none — the card named exactly the right file and line
  number for the code to change (`local_runner.rs`, around line 485 for `set_secret`, ~510
  for `remove_secret`; both matched what the file actually held at this branch's base).

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*
