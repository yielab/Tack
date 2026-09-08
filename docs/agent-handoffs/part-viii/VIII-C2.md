# VIII-C2 handoff

- Base SHA / branch / final SHA: `0002136` / `agent/viii-c2-live-proof` / *(this commit)*
- Files changed (must equal ownership list): `crates/tack-orch/src/adapters/docket.rs`
  (the "Verified live against a real docket server" module-doc section, rewritten; plus one
  stale comment inside `decide_approval` that pointed at the old, now-outdated version of
  that section — see "One line outside the strict ownership list" below),
  `crates/tack-orch/tests/docket_live_test.rs` (new, `#[ignore]`d), this handoff.
- Contract fixtures consumed: none from `docs/contracts/runner-v1/` — docket is a control
  plane adapter, not a runner-v1 wire type.
- Behavior implemented: none. No route, type, or existing test changed. One new
  `#[ignore]`d integration test (`docket_live_test.rs`) that spawns a real, isolated
  `docket serve` and drives `DocketAdapter::dispatch`/`get_run`/`provision_pod`/
  `enqueue_task` against it; it never runs under a plain `cargo nextest run --workspace`.
- Tests added and exact commands/results: see "Measured numbers" below — both the new
  live test and the full workspace suite.
- Failure/adversarial case proved: the live test's own second scenario — dispatching a
  project docket has never provisioned a pod for still succeeds synchronously (`200`, a
  run id) and only fails asynchronously (`DispatchError: no pod found for '<project>'`),
  proving `dispatch` does not gate on project existence the way this Part's own test suite
  had assumed. See "One assumption ... turned out false" below.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: a genuine `pre_input` **block** occurring
  inside a live `/dispatch/{project}` run is `not_measured` — see "Measured numbers" and the
  module doc's own new closing paragraph for why (structurally excluded from this route by
  source, and the one hook that *can* fire there, `pre_output`, needs a real, costed agent
  turn to reproduce, which this card's no-credential constraint forecloses).
- Secrets/logging review: no log line touched. The scratch `DOCKET_SERVE_TOKEN` used
  throughout is a disposable, hardcoded test fixture value, never a real credential, and
  the live test forwards no provider credential into the spawned process's environment at
  all (`env_clear()` before the three vars it does set — `PATH`, `DOCKET_HOME`,
  `DOCKET_SERVE_TOKEN`).
- Safe merge order and likely conflicts: no overlap with VIII-A3 — disjoint files (§VIII.2).
  Last card of the Part; nothing depends on this one merging.
- Checklist: no unowned files touched beyond the one disclosed comment, no live secret, no
  panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| `enqueue_task`'s response shape, the `pre_input` three-outcome/`trusted` behavior, and `provision_pod`'s happy/409/400/401 shapes are unchanged at `v0.2.0-beta.2` | curl transcripts below, re-run against the same isolated server; `provision_pod`'s re-run through the compiled adapter is `live_dispatch_against_a_real_docket_server`'s own setup step |
| `decide_approval`'s `deny` and 409 (`ApprovalNoop`)/404 split — previously read from source only — are now captured live | curl transcripts below |
| `DocketAdapter::dispatch` decodes a real `POST /dispatch/{project}` response correctly, end to end, through this crate's own compiled adapter | `live_dispatch_against_a_real_docket_server` |
| A dispatched run's outcome is only ever visible via a later `GET /runs/{id}`, never the `dispatch` response itself — ADR 0065's central claim | same test: `dispatch` returns before `get_run` ever reports a terminal state, on two independent scenarios (a provisioned pod with a queued task, and an unprovisioned project) |
| A dispatch with no provider credential reachable fails locally, for zero cost, never reaching a real provider | `costUsd: 0.0` on the task record throughout; the run's `error` names a local resolution failure (`no endpoint configured for model '<model>'`), never a network/auth error — curl transcript and the live test's own assertion on `final_run.error` |
| An unknown/unprovisioned project does **not** 404 on `/dispatch/{project}` — `dispatch_404_maps_to_not_found` in `docket_adapter_test.rs` models a shape this card could not reproduce against the real server | curl transcript below; the live test's second scenario reproduces the same "200, then async local failure" outcome through the compiled adapter |
| `~/.docket` was never touched by any of this | mtime unchanged before/after every capture — see below |

A row with no evidence is a claim to delete, not a row to leave blank.

## Measured numbers

### How the server was built and version-confirmed

`../rack-cli` (`/home/ox/Sites/rack-cli`) HEAD at the time of this card was `0d3720a`, four
commits ahead of tag `v0.2.0-beta.2` (`git log --oneline v0.2.0-beta.2..HEAD`) — all four are
documentation/packaging-only (`README.md`, `CONTRIBUTING.md`, `ROADMAP.md`,
`docs/DOCKET.md`, `Formula/docket-cli.rb`, `scripts/metrics.py`); `git diff --stat
v0.2.0-beta.2..HEAD -- src/` is empty, so building from HEAD is building the tagged
release's own source.

```
cd /home/ox/Sites/rack-cli
uv build --out-dir /tmp/tack-agent-targets/viii-c2/docket-build-5Rk8/dist
# Successfully built docket-0.2.0b2.tar.gz and docket-0.2.0b2-py3-none-any.whl
uv venv /tmp/tack-agent-targets/viii-c2/docket-build-5Rk8/venv --python 3.11
uv pip install --python .../venv/bin/python .../dist/docket-0.2.0b2-py3-none-any.whl
.../venv/bin/docket --version
# docket 0.2.0b2
```

`0.2.0b2` is Python packaging's normalized spelling of `v0.2.0-beta.2` (`pyproject.toml`'s
own `version = "0.2.0-beta.2"`) — confirmed from the running installed package, not assumed
from the tag name. `git status --porcelain` in `../rack-cli` was empty both before and
after `uv build` (`--out-dir` was pointed outside the repository specifically so the build
writes nothing into it).

### `DOCKET_HOME` used, and the `~/.docket` proof

Manual exploration used `/tmp/tack-agent-targets/viii-c2/docket-build-5Rk8/docket-home`
(created by this card, `/tmp/tack-agent-targets/...` — the pinned `CARGO_TARGET_DIR`
partition, off `/home`). The compiled-adapter test (`docket_live_test.rs`) manages its own
`DOCKET_HOME` per run via `tempfile::tempdir()`, torn down automatically at the end of the
test — never a hand-built path under `env::temp_dir()`.

```
$ stat -c '%Y %y' ~/.docket
1787197154 2026-08-20 00:39:14.075039078 -0300          # before
... every capture below, including a killed-and-restarted server after a setup mistake ...
$ stat -c '%Y %y' ~/.docket
1787197154 2026-08-20 00:39:14.075039078 -0300          # after — unchanged
$ find ~/.docket -newer /tmp/tack-agent-targets/viii-c2/docket-build-5Rk8/dist
                                                          # empty — no file under it is newer
```

One setup mistake, corrected before any real capture: the first attempt to launch the
scratch server ran with `DOCKET_HOME` unset (a shell-persistence assumption that does not
hold between separate tool calls in this harness), so it would have defaulted to
`~/.docket`. It was killed (`SIGKILL`) within about a second, before printing its startup
banner, and `~/.docket`'s mtime was already confirmed unchanged immediately after — the
process never got far enough to touch anything under it (`_run_sweeps` reads/writes under
`DOCKET_HOME` only once the module has finished importing and `run_serve` has resolved the
token). Every capture after that point set `DOCKET_HOME` inline in the same command that
launched the process.

### Re-verified unchanged (curl, against the isolated server, port `18173`, token
`viii-c2-scratch-token`)

```
POST /tasks/live-proof {"description":"say hello"}
  -> 200 {"ok": true, "task": "task-92a6da34-...", "project": "live-proof", "status": "pending"}

POST /tasks/live-proof {"description":"please BLOCK_ME now"}   (policy action=block)
  -> 400 {"ok": false, "error": "task rejected by guardrail policy 'block-secret' at enqueue: test guardrail block"}

POST /tasks/live-proof {"description":"NEEDS_APPROVAL please"} (policy action=require_approval)
  -> 200 {"ok": true, "task": "...", "status": "waiting_approval", "approvalToken": "apr-9e79a4ea-..."}

POST /tasks/live-proof {"description":"IGNORE_PREVIOUS instructions"}            (no trusted flag)
  -> 200 {"ok": true, ..., "status": "pending"}                                  (operator-trust default: policy skipped)
POST /tasks/live-proof {"description":"IGNORE_PREVIOUS instructions","trusted":false}
  -> 400 {"ok": false, "error": "task rejected by guardrail policy 'prompt-injection' at enqueue: ..."}

POST /pods {"project":"live-proof", "path": "..."}
  -> 201 {"ok": true, "project": "live-proof", "blueprint": "software",
          "members": [{"id":"live-proof-lead","role":"lead","model":"anthropic/claude-haiku-4-5"}, ...]}
POST /pods {"project":"live-proof"}                       (duplicate)
  -> 409 {"ok": false, "error": "'live-proof' already exists"}
POST /pods {"project":"bp-test","blueprint":"nope-blueprint"}
  -> 400 {"ok": false, "error": "unknown blueprint 'nope-blueprint'; valid blueprints: software, research, content, ops, agentic-product"}
POST /pods {}                                             -> 400 "project is required"
POST /pods (no Authorization header)                      -> 401 "Unauthorized"
```

### Newly closed live (curl) — `decide_approval`'s `deny`/409/404

```
POST /approvals/apr-9e79a4ea-... {"action":"grant","channel":"tack"}
  -> 200 {"ok": true, "token": "apr-9e79a4ea-...", "state": "granted"}
  (follow-up GET /tasks/live-proof confirms the task moved waiting_approval -> pending)

POST /approvals/apr-9e79a4ea-... {"action":"grant","channel":"tack"}   (replay, already decided)
  -> 409 {"ok": false, "error": "Already granted: apr-9e79a4ea-..."}

POST /approvals/apr-does-not-exist {"action":"grant","channel":"tack"}
  -> 404 {"ok": false, "error": "Approval not found: apr-does-not-exist"}

POST /approvals/apr-f93b848c-... {"action":"deny","channel":"tack"}    (fresh waiting_approval task)
  -> 200 {"ok": true, "token": "apr-f93b848c-...", "state": "denied"}
  (follow-up GET /tasks/live-proof shows that task status=failed, reason="approval denied")
```

### `dispatch`, first live capture — via curl, then reproduced through the compiled adapter

```
POST /dispatch/scratch-project {}    (project never provisioned)
  -> 200 {"ok": true, "run": "run-a77967cc-...", "project": "scratch-project", "status": "dispatched"}
GET /runs/run-a77967cc-...  (1s later)
  -> {"state": "failed", "error": "DispatchError: no pod found for 'scratch-project'", ...}

POST /dispatch/live-proof {}         (real pod, one queued task "say hello", no provider credential anywhere)
  -> 200 {"ok": true, "run": "run-387307a1-...", "project": "live-proof", "status": "dispatched"}
GET /runs/run-387307a1-...  (~1s later) -> state: running
GET /runs/run-387307a1-...  (~4s later) -> state: failed
  error: "1 returned task(s) failed: task-92a6da34-...: lead hop failed: no endpoint configured for model 'anthropic/claude-haiku-4-5'"
GET /tasks/live-proof -> the same task: status=failed, costUsd=0.0, hops[0].error = the same message
```

`resolve_endpoint` in `../rack-cli/src/docket/edges/adapters/llm.py` (read below) explains
why this is a *local* failure and not a rejected call to a real provider: with no stored
provider config and no `ANTHROPIC_API_KEY` anywhere it looks, the `anthropic` provider has
no entry in `_HOSTED_GATEWAY_BASE_URLS` at all — `resolve_endpoint` returns `None` before
any URL is ever built, so nothing is sent over the network.

Then, through the compiled adapter (`live_dispatch_against_a_real_docket_server`):

```
$ TACK_LIVE_DOCKET_BIN=/tmp/.../venv/bin/docket \
  CARGO_TARGET_DIR=/tmp/tack-agent-targets/viii-c2 \
  cargo nextest run --workspace --run-ignored ignored-only \
    -E 'test(live_dispatch_against_a_real_docket_server)'
...
     Summary [   6.547s] 1 test run: 1 passed, 1504 skipped
```

The test provisions a fresh pod (`provision_pod`), enqueues one task (`enqueue_task`),
dispatches it (`dispatch`), polls `get_run` to a terminal state, and asserts `state ==
Failed` with `error` containing `"no endpoint configured"` — then repeats the
dispatch/poll pair against a project with no pod at all and asserts `error` contains
`"no pod"` — then confirms an unauthenticated `dispatch` call is rejected before anything
is created. `~/.docket`'s mtime was re-checked immediately after this run too (see above);
`ps aux | grep 'docket serve'` showed no leftover process (the `ServeGuard` drop impl kills
the spawned child even on a panicking assertion).

### Full workspace, and the docket-scoped slice

```
$ cargo nextest run --workspace -E 'binary(docket_adapter_test) or binary(docket_wire_contract_test) \
    or binary(docket_tick_contract_test) or binary(docket_live_test)'
     Summary [   0.318s] 60 tests run: 60 passed, 1 skipped   (the 1 skipped is the new live test, correctly ignored by default)

$ cargo nextest run --workspace
     Summary [  17.844s] 1497 tests run: 1497 passed, 8 skipped

$ cargo fmt --all -- --check          # clean
$ cargo clippy --workspace --all-targets -- -D warnings   # clean
$ ./scripts/check-comments.sh         # ✓ no board archaeology
$ ./scripts/check-test-hygiene.sh     # ✓ tests take their temporary paths from a guard
$ .githooks/pre-push
✓ pre-push checks passed
```

## One line outside the strict ownership list

`decide_approval`'s own doc comment (inside its function body, not the module doc) said "see
the module doc's 'Verified live' section for the grant/404 facts and the read-from-source
409/404 split this implements" — true before this card, false after it, since that split is
now a live capture rather than a source reading. Ownership (§VIII.2) names only the module
doc's "Verified live" section itself, not this inline comment, but leaving it would have put
a demonstrably wrong claim ("read-from-source") three lines below prose this same card just
made true. Corrected the wording (still inside `docket.rs`, still describing the same three
facts) rather than widen scope any further — matches VIII-A1's own precedent for exactly
this shape of one-line fix.

## What a stranger still cannot do

Nothing about this card changes what a stranger can do with Tack — it owns evidence, not
behavior. What it changes is what a reader of `docket.rs` can trust: before this card, the
"Verified live" section carried no version marker at all, so a reader had no way to know it
was two releases and thousands of changed lines behind the repository `../rack-cli` builds
from. A stranger reading the module doc today gets, for every claim, whether it was
re-confirmed at `v0.2.0-beta.2`, newly captured, or explicitly not re-run and why.

## One assumption this card found and left uncorrected in code

`docket_adapter_test.rs`'s `dispatch_404_maps_to_not_found` mocks a `404` response for
`/dispatch/{project}` and asserts the adapter maps it to `OrchError::NotFound`. Reading
`serve.py`'s `/dispatch/` branch and reproducing it live both show no code path in that
route returns `404` — an unknown/unprovisioned project succeeds synchronously (`200`, a run
id) and only fails later, asynchronously. `OrchError::NotFound`'s mapping is not a decode
bug (a `404` from this route, if a future docket build ever sent one, would still decode
correctly), so the "one exception" clause in this card's own instructions does not apply,
and the existing test was left exactly as it is — it documents defensive handling, not an
observed behavior, and this card owns no test file to correct it in. This is the finding to
carry into a new card if that test's framing (docstring/comment) should say so explicitly.

## Context spent

- Tokens read before the first edit (cold start): the dispatch prompt itself carried nearly
  all of this card's Acceptance and "Must not" content verbatim (see "Read-list lines that
  were wrong" below for why), so the first real reading was `docs/agent-handoffs/part-viii/README.md`'s
  header (VIII-C2 had no block there either — see below), the module doc's existing
  "Verified live" and "Write methods" sections plus `dispatch`/`decide_approval`/
  `provision_pod` in `docket.rs`, `RemoteRun`/`NewRemoteTask`/`ProvisionPodParams` in
  `lib.rs`, ADR 0065's "A block is not synchronously observable" section, and
  `../rack-cli/src/docket/serve.py`'s full `do_GET`/`do_POST`. Substantially more of
  `../rack-cli` was read than the dispatch prompt's item 6 implied it would take — see
  "What I read in `../rack-cli`" below for the full list, driven by the safety requirement
  (verifying, from source, that no provider credential path could be reached before
  spending anything live).
- Context size at handoff: large single-card session — the credential-safety investigation
  into `edges/adapters/llm.py`/`provider.py` and the systematic re-verification of every
  existing "Verified live" bullet both cost real reading beyond the dispatch prompt's own
  scope, deliberately, since Acceptance 5 requires each claim marked confirmed/changed/not
  re-run rather than assumed.
- Files opened and not used: `../rack-cli/src/docket/core/pod_provisioning.py` was grepped
  for its rollback-on-partial-failure contract (mentioned in `serve.py`'s own
  `_handle_post_pods` doc comment) but never read line-by-line — not needed once the live
  `POST /pods` capture matched the existing module doc exactly.
- Read-list lines that were wrong: **`TODO.md §VIII.4`'s VIII-C2 card block did not exist
  in this worktree.** The dispatch prompt's `sed -n '431,487p' TODO.md` landed on §VIII.5 and
  the start of Part VII instead — a real line-number drift, not a typo — and a direct search
  for `### VIII-C2` in both `TODO.md` and `docs/agent-handoffs/part-viii/README.md` found
  nothing: VIII-C2 had an ownership row, a wave-table row, and a slot in the dependency
  graph, but no `§VIII.4` block and no README card block, the same gap VIII-A2 had before it
  was dispatched. The actual card text exists at commit `40fd9ee` ("docs(board): card out
  VIII-C2, which had a slot in the graph but no card"), reachable from `develop` but **not**
  an ancestor of this branch's base (`0002136` — `40fd9ee`'s own commit message confirms
  this: "Wave 26's base moves to 0002136"). Read it via `git show 40fd9ee -- TODO.md`
  without merging or cherry-picking anything from it. Its Acceptance and "Must not" text
  turned out to match the dispatch prompt's own content almost verbatim (both were written
  from the same source), so this did not change what the card asked for — only where its
  authoritative text actually lived.

## What I read in `../rack-cli`

Repository: `/home/ox/Sites/rack-cli`, HEAD `0d3720a` at the time of reading (four commits
past `v0.2.0-beta.2`, all documentation/packaging — confirmed via `git diff --stat
v0.2.0-beta.2..HEAD -- src/`, empty). Read-only throughout; nothing written or committed
there, confirmed by `git status --porcelain` before and after.

- **`src/docket/serve.py`, full `do_GET`/`do_POST`.** Re-read against `v0.2.0-beta.2`
  rather than trusted from the prior capture — confirmed every route this adapter uses
  still exists at the same paths with the same auth gating. Found the `/dispatch/` branch
  (~line 1047–1104) has no code path that returns `404`: the only checks before the `200`
  response are "project non-empty" (`400`), "body is valid JSON" (`400`), "body is a JSON
  object" (`400`), and `resolve_variables` against the pipeline's declared variables
  (`400` via `VariableError`) — nothing checks whether `project` names a real pod.
- **`src/docket/core/dispatch.py`, module docstring (~line 57–66) plus
  `_enqueue_pre_input_gate`/`enqueue_task` (~line 302–390) and `effective_pipeline`/
  `dispatch_pod` (~line 478–517, ~2319–2390).** The module doc states plainly that
  `pre_input` is evaluated **once, at enqueue, and never re-evaluated** before a later hop
  or a later dispatch — confirmed structurally (no call to `policy_eval_detail` with hook
  `pre_input` anywhere in `dispatch_pod`'s call graph) and confirmed live (the `trusted`/
  policy captures above show the gate firing only at the `POST /tasks/{project}` call, never
  again when `live-proof`'s tasks were later dispatched). This directly narrows ADR 0065's
  "guardrail evaluation included" wording: what actually runs off-thread on this route is
  never a `pre_input` block, because `pre_input` cannot fire there at all — see the module
  doc's own new closing paragraph. `dispatch_pod` also confirmed `pod_pipeline(project)`
  (front-loaded lead/pod validation) is the only thing that can fail before a task is even
  claimed — which is exactly the `DispatchError: no pod found for '<project>'` path this
  card captured live for the unprovisioned-project scenario.
- **`src/docket/core/runs.py`, `create_run`/`execute` (~line 256–330, ~662–710).** Confirmed
  the run record is created and its id returned **before** `execute` invokes the real
  dispatch — and that `execute` catches every exception from its callback and folds it into
  a `failed` state with the exception text as `error`, never letting one escape or crash the
  daemon thread. This is the mechanism the whole "run id is not a promise of permission"
  claim depends on, and this card is the first to have watched it happen against a real
  server for `dispatch` specifically (`enqueue_task`/`provision_pod` don't go through it —
  they answer synchronously with their own real outcome).
- **`src/docket/edges/adapters/llm.py`, `resolve_endpoint`/`client_for`
  (~line 399–496), and `provider.py`'s `_PROVIDER_CREDENTIALS`.** Read specifically to
  satisfy this card's no-cost constraint *before* dispatching a real pod, not after: with a
  fresh `DOCKET_HOME` (no stored provider config) and no `ANTHROPIC_API_KEY`/equivalent in
  the process environment, `resolve_endpoint("anthropic/claude-haiku-4-5")` returns `None`
  before constructing a base URL at all, because `_HOSTED_GATEWAY_BASE_URLS` has no
  `"anthropic"` entry (only `openrouter`/`ai-gateway` are wired as direct hosted gateways in
  this adapter) — so `client_for` returns `None` and the caller
  (`edges/adapters/docket_runtime.py:258`) raises "no endpoint configured for model
  '<model>'" **before any network call is attempted**, not after one fails. This is what
  made it safe to actually dispatch a real provisioned pod's real queued task rather than
  only reproducing the cheaper "no pod" scenario — read and confirmed before that dispatch
  was run, not discovered by accident afterward.
- **`src/docket/core/policy.py`, module doc and `policy_eval_detail`/`POLICIES_DIR`
  (~line 1–130).** Confirmed policies are files under `$DOCKET_HOME/policies/*.json`, read
  fresh on every call (no caching to invalidate), which is why three scratch policy files
  dropped into the isolated `DOCKET_HOME` (`block-secret`, `needs-approval`,
  `prompt-injection`) were enough to re-verify `pre_input`'s three outcomes and the
  `trusted` boundary live without needing a server restart.
- **`src/docket/core/approval.py`** was not re-read line-by-line this time (already read by
  whichever earlier capture wrote the existing module doc's `ApprovalNoop` reasoning) —
  its predicted 409/404 split was checked against behavior instead, live, and matched
  exactly, so no further source reading was needed to close that gap.
- **What changed because of this reading:** the module doc's "Verified live" section now
  states the docket version every claim was captured against (it stated none before), marks
  the previously-source-only `decide_approval` 409/404/`deny` split as live-confirmed, adds
  the first-ever live capture for `dispatch`, and corrects the pre-existing assumption that
  an unknown project 404s on `/dispatch/{project}` — replaced with what the server actually
  does. No adapter code changed as a result of any of this reading; every finding was a
  documentation correction or a `not_measured` label, per the card's "fix nothing but a
  proven decode bug" constraint.

## Revert proof

Not applicable in the classic sense — this card fixed no defect in `dispatch`'s decode (the
one exception clause never triggered; see "One assumption ... turned out false" above,
where the finding was about the real server's behavior, not about this crate misreading a
real response). There is no code change to revert-prove. The new test's own load-bearing-ness
is instead shown by construction: `live_dispatch_against_a_real_docket_server` asserts on
the *shape* docket actually sent (`error` containing `"no endpoint configured"` / `"no
pod"`, `state == Failed`), not on a value this crate invented, so a future change to
`DocketAdapter::dispatch`'s decode that broke it would fail this test the next time someone
opts in and runs it — which is exactly what an `#[ignore]`d live oracle is for.

## Question I did not answer

None. §VIII.1 rule 1 did not stop this card — every finding above came from reading and
reproducing, not from a new design decision, and the one place the acceptance criteria
anticipated a possible ADR-level finding (a live server contradicting ADR 0065's central
claim) resolved in the ADR's favor: the claim held, this capture only sharpens which
guardrail hook it actually concerns.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*
