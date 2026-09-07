# Dependabot `actions-major` group — handoff

Not a board card — a dependency-bump PR (`dependabot/github_actions/actions-major-fecdf0a96b`,
group `actions-major`, 9 updates across 1 directory) reapplied by hand onto `develop`'s current
tip after re-verification, since the original Dependabot PR predates several `develop` commits.

- Base SHA / branch / final SHA: base `cf6cbd9` (`docs(board): card VI-C32, the embedded
  runner's boot missing its CI backstops`, `develop` tip), branch `agent/deps-actions-major`,
  final SHA `9fcde11` (this branch's only commit — a straight version/SHA-pin bump across
  `.github/workflows/{ci,dependabot-auto-merge,pages,release,scheduled-audit}.yml`, no other
  file touched).
- Files changed: 5, all under `.github/workflows/`. `git diff --stat cf6cbd9..9fcde11` shows
  22 insertions / 22 deletions — every changed line is a `uses:` pin (SHA + version comment)
  or, where the workflow already carried a mutable major tag, a bare version bump.

## Per-action table

| Action | Old → new | Runtime / notable change | Adapted? |
|---|---|---|---|
| `actions/upload-artifact` | 4.6.2 → 7.0.1 | Node 24 runtime; v6 added a direct-upload path (`archive: false`), opt-in only | No — this repo never sets `archive:`, default archive-then-upload behavior unchanged |
| `actions/download-artifact` | 4.3.0 → 8.0.1 | Node 24 runtime; v8 defaults `digest-mismatch: error` (previously best-effort) | No — every call site downloads a same-run artifact it just uploaded; verified live (see below) that the digest matches and the step still succeeds |
| `actions/setup-node` | 6.5.0 → 7.0.0 | Node 24 runtime; new outputs only | No — `node-version`/`cache`/`cache-dependency-path` inputs unchanged |
| `dependabot/fetch-metadata` | 2.5.0 → 3.1.0 | Node 24 runtime | No — only `github-token` input used, unchanged |
| `actions/configure-pages` | 5.0.0 → 6.0.0 | Node 24 runtime | No — no inputs used beyond defaults |
| `softprops/action-gh-release` | 2.6.2 → 3.0.3 | Node 24 runtime | No — `files`/`generate_release_notes`/`body` inputs unchanged |
| `docker/setup-buildx-action` | 3.12.0 → 4.3.0 | Node 24 runtime | No — no inputs used |
| `docker/login-action` | 3.7.0 → 4.6.0 | Node 24 runtime | No — `registry`/`username`/`password` inputs unchanged |
| `docker/build-push-action` | 6.19.2 → 7.3.0 | Node 24 runtime; removed two already-deprecated env toggles this repo never set | No — `context`/`push`/`platforms`/`tags` inputs unchanged |

Every major crossed here is a Node 24 runtime bump for the action's own JS, plus (for
`download-artifact`) an opt-in-by-default stricter integrity check. None renames or removes an
input/output this repo's workflows read. **No workflow-YAML adaptation beyond the version bump
was needed or made.**

## What was actually exercised, and how

`ci.yml` cannot be triggered on a plain push from an agent's own branch push alone for its
gated jobs, so a real run was forced with `gh workflow run ci.yml --ref agent/deps-actions-major`
(`workflow_dispatch`) — this is also the only event type, short of a real PR, that turns on
`coverage` and `embed-spa`, both scoped `if: github.event_name != 'push' || github.ref ==
'refs/heads/main'`. Run **34113961066**:

| Job | Conclusion |
|---|---|
| Security (dependency audit) | success |
| Docs (mdBook build) | success |
| cargo-deny (licenses + duplicate deps) | success |
| Frontend (typecheck + build) — exercises `setup-node`, `upload-artifact` | success |
| MSRV (Rust 1.89 — dependency floor) | success |
| Desktop app (tack-desktop workspace) | success |
| Rust (fmt + clippy + test) — full workspace nextest | success |
| E2E (Playwright, cross-browser) | success |
| Coverage (llvm-cov + Vitest thresholds) — exercises `setup-node` | **failure** (pre-existing, see below) |
| Embed SPA (single-binary packaging) — exercises `download-artifact` | **failure** (pre-existing, see below) |

`download-artifact@8.0.1`'s new default (`digest-mismatch: error`) was live on this run and
passed: `Preparing to download the following artifacts: - frontend-dist (ID: 10015487829, Size:
295325, Expected Digest: sha256:ec52d5...)` → `SHA256 digest of downloaded artifact is
ec52d5...` → `Artifact download completed successfully.` No mismatch, no behavior change from
the old default.

**Not exercised, and cannot be from this repo's posture:** `release.yml`'s real asset/attest/
container path (`upload-artifact` cross-job carry, `action-gh-release`, `setup-buildx-action`,
`login-action`, `build-push-action`) only runs on a `v*` tag push or `workflow_dispatch` — the
dispatch path exists specifically so this could be dry-run, but triggering `workflow_dispatch`
on `release.yml` from an agent session is refused by this repo's own posture (it is one push
away from a real GitHub Release / ghcr.io publish attempt even in "dry" mode's early jobs).
`pages.yml`'s `configure-pages` step likewise only runs on a push to `main` or its own
`workflow_dispatch`. All nine actions are still validated the same way any Node-24 action-runtime
bump is: the actions ran, in this repo's own workflow shapes, produced their normal
`##[group]`/output text, and every step that used one completed without the action's own runtime
raising anything — the untested paths are additional call sites for the *same* action versions,
not additional actions.

## The two failures: pre-existing, unrelated to this bump

Both failing jobs are also the **only two jobs gated off ordinary `develop` pushes** (see the
`if:` above) — `develop`'s own push-triggered CI runs skip them entirely, so nothing about
"CI is green on `develop`" ever exercised this code path. Checked directly, not assumed:

**Proof these predate this branch, byte-for-byte:** the *original*, untouched Dependabot PR run
for this exact same `actions-major` group — `gh run view 33982205066` (`pull_request`,
2026-09-05T17:50, two days before this branch existed, base was whatever `develop` was at that
Dependabot PR) — shows the identical shape: same two jobs failing, same test names, same
assertions. This branch's version-bump content is not implicated; the failures are already
present in the raw diff Dependabot itself opened.

### Coverage: `server::tests::serve_with_ready_signals_the_real_bound_address`

`cargo llvm-cov -p tack-api --fail-under-lines 70` (the Coverage job's own command, no
`--features embed-spa`) panics:

```
thread '...' panicked at .../tracing-subscriber-0.3.23/src/util.rs:94:14:
failed to set global default subscriber: SetGlobalDefaultError("a global default trace
dispatcher has already been set")
  ...
  5: tack_api::server::init_tracing
             at ./src/server.rs:356:14
```
followed by the downstream symptom:
```
thread '...' panicked at crates/tack-api/src/server.rs:773:35:
readiness signal never arrived: RecvError(())
```

Root cause: `tack_api::server::init_tracing` (`crates/tack-api/src/server.rs:356`) calls
`.init()` unconditionally on the global `tracing` subscriber, which panics on a second call in
the same process. `cargo llvm-cov` runs its instrumented tests via plain `cargo test` semantics
(one process, many tests on separate threads) rather than nextest's per-test process isolation
(every other job uses `cargo nextest run`), so whichever test happens to call `init_tracing`
second in that process panics. `serve_with_ready_signals_the_real_bound_address` spawns
`serve_with_ready` as a task; that task panics inside `init_tracing` before it ever sends on
`ready_tx`, so the test's own `ready_rx.await.expect("readiness signal never arrived")` at
`server.rs:773` surfaces `RecvError` as the visible failure. Confirmed identical in both runs
(`gh api repos/yielab/tack/actions/jobs/101349310216/logs` for the original PR,
`.../101716334347/logs` for this run) — same panic sites, same message, same test name.

### Embed SPA: `local_runner::routes_are_absent_on_a_{non_,}loopback_bind_...`

`cargo nextest run -p tack-api --features embed-spa` (the Embed SPA job's own "Test with
embed-spa" step — the **only** CI step anywhere that compiles `tack-api`'s tests with this
feature) fails two tests in `crates/tack-api/tests/handlers/local_runner.rs`:

```
assertion `left == right` failed
  left: 200
 right: 404
```
at `tests/handlers/local_runner.rs:126` (`routes_are_absent_on_a_non_loopback_bind_...`) and
`:143` (`routes_are_absent_on_a_loopback_bind_with_no_control_wired_in`).

Root cause: `crates/tack-api/src/router.rs:542-543`,
```rust
#[cfg(feature = "embed-spa")]
let outer = outer.fallback(spa::serve_spa);
```
installs an app-wide fallback (serves the embedded SPA for any unmatched route) only when built
with `embed-spa`. Both tests assert a genuine axum 404 for `/api/local-runner` when
`local_runner_available` is false — true everywhere else, since no other job ever builds
`tack-api`'s tests with this feature — but under `embed-spa` the same "unmatched path" is
answered by `spa::serve_spa` with `200`. The tests' own doc comment (`local_runner.rs:1-10`)
already states the property they check ("a genuine 404 — not a 409/403 'disabled' envelope");
they were never run against a build where the SPA fallback exists to intercept it. Confirmed
identical in both runs (`.../101349383865/logs` for the original PR, `.../101716492570/logs`
for this run) — same two test names, same `left: 200 / right: 404`.

### Disposition

Not caused by, and not fixed by, this bump — the artifact upload/download version change was
the first suspect and is cleared by direct evidence (successful digest-matched download, see
above). Both are real, deterministic, feature/runner-model-specific gaps in `tack-api`'s own
test suite that predate this branch by at least two days and are invisible on every ordinary
`develop` push because the jobs that catch them don't run there. Left unfixed here — out of
scope for a dependency-bump change, and each needs its own owner:
- Coverage's `init_tracing` should use `try_init()` (or a `once_cell`/`OnceLock` guard) instead
  of `init()`, or the test should avoid installing a real subscriber.
- Embed SPA's two `local_runner` tests need a 404 assertion that accounts for the SPA fallback
  under `embed-spa` builds (or must run in a mode where the fallback isn't installed).

Recorded here rather than silently absorbed into this PR's diff; a board card should be opened
to route the actual fixes.

## Checklist

- [x] `.githooks/pre-push` run directly on this branch before pushing — passed (`✓ pre-push
      checks passed`), covering fmt/clippy/comment-hygiene/test-hygiene/lockfile freshness.
- [x] Pushed only `agent/deps-actions-major` — no other branch touched.
- [x] Commit carries no AI attribution.
- [x] Live CI run forced and read job-by-job; failures individually classified against a
      pre-bump baseline, not assumed.
