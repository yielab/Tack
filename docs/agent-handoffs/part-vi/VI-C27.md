# VI-C27 handoff

Worked directly on `develop` by the integrator, at the user's request to audit the
defect to its origin rather than dispatch it. No card branch.

- Base SHA / branch / final SHA: `fa8f9cc` / `develop` / the merge commit that carries this file.
- Files changed (must equal ownership list): `crates/tack-cli/src/local_runner.rs` (the
  card's owned file) and its tests; **plus, outside the card's ownership line and
  declared here rather than hidden:** `crates/tack-runner/src/engine.rs` (one match arm
  and one method, with a test), the two adapter comments in
  `crates/tack-runner/src/harness/{claude_code,codex}.rs` that misstated the engine's
  ordering, `crates/tack-api/src/handlers/local_runner.rs` (trait doc comments only),
  `frontend/src/features/agents/ProviderKeyPanel.tsx` (one comment),
  `docs/book/src/user-guide/quick-start.md` (one sentence), `CHANGELOG.md`, and a new
  integration test `crates/tack-cli/tests/embedded_runner_live_secret.rs`. See "Why two
  layers" for why the engine change is part of this defect and not scope creep.
- Contract fixtures consumed: `docs/contracts/runner-v1/completion.request.json` (shape
  of a terminal report; nothing changed). No fixture edited.
- Behavior implemented: (1) a change to the credential of a configured provider, while
  the embedded runner is running, restarts the runner before the request returns — old
  task joined first. (2) A harness rejection at `validate`, arriving after the attempt was
  announced as `preparing`, is reported as a `failed` attempt with `harness_rejected` as
  its reason, through the existing terminal outbox.
- Tests added and exact commands/results: see "Claim → evidence".
- Failure/adversarial case proved: both halves, by reverting each once — see below.
- Schema/API/contract change requested from another owner: none. The `harness_rejected`
  reason code is new vocabulary inside `terminal_reason`, which the contract types as an
  opaque object; the fixture's `code`/`message` shape is kept.
- Known limitations or `not_measured` fields: see that section.
- Secrets/logging review: see "Secret-path proof".
- Safe merge order and likely conflicts: none pending; VI-C28's partial worktree touches
  `websocket.rs` only.
- Checklist: no unowned files beyond those declared above, no live secret, no panic stub,
  no blind retry.

## Why two layers

The card named one defect. Auditing it to its origin found two, and the observed
symptom needs both.

**Layer 1 — `EmbeddedRunnerControl` (tack-cli).** `start()` hands `RunnerConfig` to
`bootstrap::run` **by value**; the adapters take `providers.clone()`, and the capability
snapshot the runner enrolls with is computed once. `set_secret` then mutated `enabled`
on the *control's* copy — which the spawned task never sees — while `catalog()`
recomputes from that same copy, so the Agents page reported live models from a view the
runner did not share. Secrets never had this problem: `SecretStore` is a shared handle
read at use time. Provider *enablement* was the one runtime-mutable fact living in the
by-value configuration, and the `vercel_ai_gateway_secret_auto_enabled` bookkeeping
existed only to service that copy.

**Layer 2 — `RunnerEngine::run_claimed` (tack-runner).** `validate` runs after the
journal record is persisted, after `report_start(Preparing)` and after the checkout is
provisioned. On `HarnessError::Rejected` the `?` propagated it as an engine error: no
terminal report, journal record left `prepared`, attempt left in `preparing` under a
lease nothing would ever complete — the server's health watch only logs, retention is off
by default, and only a runner restart's recovery scan would ever revisit the record. The
adapters' own comments claimed `validate` runs "before any journal record or workspace
exists"; the engine violated its adapters' contract. That is why the failure was
*silent*, and it applied to every pre-spawn rejection (a binary that vanished, a policy
the harness cannot honour), not only to this one.

## The choice, and what it costs

The card offered two shapes: a shared configuration handle, or a restart. The runner's
model is already "configuration is fixed for the life of one task" — by-value config,
capabilities computed once, session established at boot. A shared handle would have to
thread through thirteen adapter construction sites *and* add a "capabilities changed"
signal into the client loop so the server-side snapshot refreshes; without the second
half, the Run-with-agent modal's "Supported" gate (which reads the stored snapshot)
would stay wrong. That is new machinery to fight the runner's own model.

So the control honours the model instead: **a change to a configured provider's
credential is a new runner lifetime.** `set_secret`/`remove_secret`, when the runner is
running and `name` is an entry a configured provider resolves, request shutdown, **await
the old task** (bounded at 10 s, abort on expiry — there is no journal lock, so two engines
on one state directory must be excluded by construction), then `start_locked` with the
updated configuration. `catalog()` and the running runner then agree because they read
the same struct. The server-side `capability_snapshot` refreshes when the new lifetime
resumes its session. An in-flight attempt at the moment of restart goes through the
already-proven crash-recovery path. A secret no provider resolves (a `secret_reference`
a request names) is read live and triggers nothing.

Cost: one restart per provider-credential change. What that costs in wall-clock is
whatever the runner's boot costs — harness discovery (`--version` probes; measured
elsewhere at up to ~24 s with both real harnesses installed) plus one catalog fetch. In
the integration test, with a fake harness and a loopback gateway, the whole restart,
re-enrollment and catalog fetch completed inside the test's 1.05 s. It is exactly what
"Re-check" already cost when the operator clicked it; now nothing reports success before
it has happened. A restart that fails (`start_locked` returns an error) leaves the runner
`stopped` and returns the error from the request — honest, never a runner that still
serves the old table.

For layer 2, the smaller of two correct fixes: keep `validate` where it is and route
the rejection into the existing outbox (`persist_pending_terminal_report` →
`send_pending_terminal_report` → cleanup), which is crash-safe and reuses the completion
path unchanged. Moving `validate` before any side effect would also avoid a wasted
checkout, but a completion from `leased` with no prior start report is a path nothing
exercises today; left as a possible later improvement, noted here rather than made.

## Claim → evidence

| Claim | Evidence |
|---|---|
| A key stored while the runner is running is used by the very next dispatch, with no Re-check | `nice -n 19 cargo nextest run --workspace --build-jobs 4 --test-threads 4 -E 'binary(embedded_runner_live_secret)'` → `1 passed` in 1.05 s. The test starts a real `tack serve --with-runner`, waits for `active`, stores the key over `PUT /api/local-runner/secrets/vercel-ai-gateway%2Fdefault`, waits until the *serving* runner's `capability_snapshot` advertises the fake gateway's model (only possible after a boot with the key), dispatches with `selector_kind: exact_runner`, and asserts `succeeded` with `actual_execution.harness_version == "9.9.1"` — the fake harness's own version, which only a spawned process can report |
| The harness is actually reached, and with the provider the key enabled | The fake `claude` writes the `ANTHROPIC_BASE_URL` it was spawned with beside itself; asserted equal to `<fake gateway>/claude-code`. Never the credential |
| The key, not the old configuration, is what the runner booted with | The fake gateway answers `/v1/models` only when the bearer equals the pasted key and counts those hits; asserted `0` before the key is stored and `≥ 1` after |
| Direct absence, pre-fix: the harness is never invoked | Engine unit test `a_rejection_at_validate_is_reported_failed_and_never_spawns`: `FakeAdapter.start_calls == 0` while a `failed` completion with `code == "harness_rejected"` is reported and `journal.unresolved()` is empty |
| The old task never survives a provider-credential change | Control unit test `setting_a_provider_secret_while_running_stops_the_old_task_before_anything_else`: a stand-in task that exits on its shutdown signal records that it was stopped; asserted `true`, status `Stopped` (the restart's `start` fails typed against the test's unreachable database, before any spawn), and the provider `enabled` in the configuration the next task reads |
| An unrelated secret leaves the running task alone | `setting_an_unrelated_secret_while_running_leaves_the_task_alone`: stopped flag `false`, status still `Running` |
| Removing a provider's credential restarts too | `removing_a_provider_secret_while_running_stops_the_old_task` |
| A key set while the runner is **stopped** keeps working | Unchanged path: `restart_for_provider_change_locked` returns immediately when `running` is `None`; the next `start` reads the current configuration. Pinned by the existing `setting_the_default_vercel_secret_enables_that_provider` (stopped control) |
| Layer 1 is load-bearing | `restart_for_provider_change_locked` reverted to a no-op → the integration test fails after 60.75 s at the snapshot wait (`the serving runner must advertise the gateway's catalog — a runner that never restarted keeps the empty snapshot it booted with`). Restored → passes |
| Layer 2 is load-bearing | The `validate` match reverted to `?` → the engine test fails in 0.01 s (`a refused request is a settled cycle, not an engine error`). Restored → passes |
| Nothing else moved | `cargo nextest run --workspace` green; `.githooks/pre-push` green; chromium E2E suite green — numbers in the board entry |

## Measured numbers

- Integration test wall-clock, fix in place: 1.05 s (command above).
- Integration test wall-clock, layer 1 reverted: 60.75 s to fail (the bounded wait).
- Engine unit test, layer 2 reverted: 0.01 s to fail.
- Gates on the final tree: `.githooks/pre-push` exit 0; `cargo nextest run --workspace`
  → 1458 passed, 7 skipped, 26.4 s; chromium E2E (`--workers=2`) → 76 passed, 44.0 s,
  including the two specs that store a key while execution is on.
- Restart cost with real harnesses installed: **not measured here**; the only figure on
  record is harness discovery at up to ~24 s (`tack runner doctor`, noted in
  `embedded_runner_orphaned_credential.rs`), which is a boot cost this change neither
  adds nor removes.

## What a stranger still cannot do

Nothing new is withheld. The Agents page's own order — turn on, paste key, choose
default model, run — now works without the "Re-check" click the stranger transcript had
to insert. The page does not yet *say* that saving a key restarts the runner; the
`since` timestamp on the execution switch is the only visible trace.

## Surface-map delta

None. No route, field or page added.

## Secret-path proof

- The engine's new `warn!` carries `attempt_id` only. The rejection reason (adapter
  text such as "provider endpoint could not be resolved") goes into the terminal
  report's `terminal_reason.message` — operator-facing data, never a log line from this
  code; the adapters already log their own reason at `warn` and that is unchanged.
- The control's new `info!`/`warn!` lines carry no id, path or value.
- The integration test's key is a literal that is not a credential anywhere; the fake
  harness records the base URL only, never `ANTHROPIC_AUTH_TOKEN`.
- No new secret column; `scrub_snapshot_secrets` unaffected.

## Vocabulary check

`harness_rejected` (new `terminal_reason.code`), `requested_not_confirmed` /
`not_observed` (existing `model_observation_source` values, reused), `unsupported` with
a reason for every unexercised feature capability, `not_measured` for every usage
figure. No `$0.00`, no zero standing in for unknown.

## Context spent

- Read before the first edit: the control, the engine's claim path, the adapters'
  `validate`, the provider resolution, the transport's capability reporting, the server's
  completion guard, the lifecycle table, and the E2E specs that store a key. Roughly
  70 targeted reads; no file read whole except the two sibling integration tests.
- Files opened and not used: `tack-orch/src/execution/mod.rs` (re-exports only).
- Read-list lines that were wrong: the card's "Owns" line named one file; the defect
  lived in two crates.

## Amendments

(none yet)
