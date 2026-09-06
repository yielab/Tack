# VI-C22 handoff

**Verdict: the reset a prior session left in place is correct; verified rather than
rewritten.** `frontend/playwright.config.ts`'s API `webServer.command` deletes
`storage-e2e/` then `e2e.db*` before every `cargo run -p tack-cli -- serve`, so a run
that spawns its own server always starts from an empty database and an empty storage
dir. The order (storage first, database second) matters for the same reason VI-C21
cards: a credential in `storage-e2e/` naming a runner id absent from the database is
the failure mode, so an interruption between the two `rm`s must land on the database
outliving the credential (safe — the embedded runner self-provisions) and never the
reverse.

- Base SHA / branch: `develop` at `f06ce56` (unchanged from dispatch — see "What is
  left" for why `develop` has since moved and what that does and does not affect),
  branch `agent/vi-c22-e2e-db-lifecycle`, one commit (card rule: no merge/push).
- Files changed: `frontend/playwright.config.ts` (already-uncommitted diff from a
  prior session, verified and kept as-is — see "What I verified"), `docs/TESTING.md`
  (kept the prior session's two new sections, rewrote the one paragraph that quoted
  unmeasured numbers — see "The one thing I changed").
- Behavior implemented: none new. The reset already existed in the working tree
  before this session started; this card's work was verifying it holds, re-measuring
  every number attached to it, and running the suite enough times to trust both.

## What I verified (already correct, not rewritten)

The `rm -rf storage-e2e && rm -f e2e.db*` command, its ordering, its wildcard
(`e2e.db*` catches `-wal`/`-shm` and the migration runner's own
`e2e.db.before-<migration>.sqlite` snapshot — confirmed this file is actually
produced: it appeared after every run in this session), and its comments in both
`playwright.config.ts` and `docs/TESTING.md` were all correct on inspection. I did
not change any of it.

## The one thing I changed

`docs/TESTING.md`'s reset-cost paragraph quoted **"~17s from a clean state" and
"~47s at 383 accumulated runners"** with no command attached. Both numbers trace to
`TODO.md`'s VI-C22 card text itself, which also carries no command — an unmeasured
number repeating an unmeasured number. Re-measured (commands below) and replaced with
what I actually got, which is not the same shape of claim: the prior figures implied
a two-point clean/dirty comparison from one session; what I have is a reset-cost
measurement (trivial, size-independent) plus a growth trend from my own accumulation
run, with the caveat below about where that trend stopped being trustworthy.

## Measurements

All measured with `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C22` (a warm
target from a resumed session — no cold workspace build was needed) and
`nice -n 19` on every command.

**Reset cost** — timed directly against the exact command the config runs:

```
time (rm -rf storage-e2e && rm -f e2e.db*)
```

- Against a single run's leftovers (~900KB database, 40KB storage dir): **2ms**.
- Against ~284 accumulated `agent_runners` rows (~8.2MB database + snapshot, 240KB
  storage dir, built by repeatedly reusing one manually-started server so the row
  counts would climb): **8ms**.

`rm` unlinks; it does not read the file, so the cost does not scale with what is
being thrown away. This is the number the acceptance bar asked for, and it settles
the question the card's own "Tasks" section raised (measure before ruling a
per-run reset in or out on cost grounds) unambiguously in favor of resetting.

**Three consecutive full chromium runs, clean state, row counts** —

```
export CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C22
cd frontend && nice -n 19 npx playwright test --project=chromium --workers=2
```

then, after each run:

```
sqlite3 e2e.db "select 'agent_runners', count(*) from agent_runners
  union all select 'projects', count(*) from projects
  union all select 'items', count(*) from items
  union all select 'execution_requests', count(*) from execution_requests;"
```

| Run | Wall clock | agent_runners | projects | items | execution_requests | Result |
|---|---|---|---|---|---|---|
| 1 | 58.4s | 15 | 4 | 21 | 11 | 69 passed, 1 failed |
| 2 | 32.7s | 15 | 4 | 21 | 11 | 69 passed, 1 failed |
| 3 | 33.8s | 15 | 4 | 21 | 11 | 68 passed, 2 failed |

Row counts are byte-identical across all three runs — the acceptance bar's actual
question. Run 1's extra ~25s is a one-time compile-check `cargo run` pays that runs
2 and 3, sharing the same warm target, don't.

**The suite does not pass clean three times in a row, and the reason is out of
scope.** Every failure across all three runs is `e2e/a11y.spec.ts`, and specifically
a `color-contrast` finding on `--color-text-secondary` (`#5f736e`) against light
backgrounds — the same defect class VI-C13's handoff already named as "genuine,
pre-existing... unrelated UI, unrelated card, reproducible independent of
parallelism" (its own instance was a `Task` badge chip at 4.27:1; mine additionally
hit a rich-text-editor toolbar select at 4.43:1 and, once, a dialog heading and tab
control at the same 4.43:1 — same token, different elements). It reproduced on run
1, from a database this reset had just emptied, which rules out accumulation as the
cause: this is a standing frontend defect, not a database-lifecycle regression, and
not something this card's ownership (`playwright.config.ts`, `docs/TESTING.md`)
covers. I did not touch `RichTextEditor.tsx`, `index.css`, or any component file.

To give the acceptance bar a clean answer anyway, I re-ran with that one file
excluded (all 33 of its tests match `"accessibility violations"` in their title;
no other spec file does — checked with `grep -l`):

```
npx playwright test --project=chromium --workers=2 --grep-invert "accessibility violations"
```

Three consecutive runs: **34 passed, 34 passed, 34 passed** — clean, three times.

**Accumulation, measured directly (not just cited from `TODO.md`).** I started one
`tack serve` by hand against `e2e.db`/`storage-e2e` (same env the `webServer` block
uses) and reused it across repeated `--grep-invert "accessibility violations"` runs
with no reset between them, to watch `agent_runners` climb:

| agent_runners | Wall clock | Machine state |
|---|---|---|
| 0 | 17.1s | idle |
| 225 | ~24s (avg of 3) | idle (checked `uptime` before and after) |
| ~285 | ~113s, 16 tests failed (none also failing at 0 or 225) | **not idle** — see below |

Between the 225 and ~285 points, unrelated processes (multiple `sh -c while :; do
:; done` loops, parented to this user's systemd, cwd in the checked-out-out main
repo rather than any worktree — confirmed via `/proc/<pid>/environ` and
`/proc/<pid>/cwd` not to be mine or started by any command in this session) pushed
load average to ~17-18 on a 16-core machine. I did not kill them: I could not prove
ownership (the one rule that matters here), and per the load-caps instruction I only
kill what I can prove is mine. The ~285-runner/113s/16-failure data point is
therefore **directional corroboration, not a clean measurement** — it is consistent
with, but not an independent re-confirmation of, this cycle's earlier report of 383
runners producing 9 failures against 0 from clean. The 0→225 trend, taken before that
load spike, is clean and is what `docs/TESTING.md` now cites.

## The two-places-of-state problem

Handled by construction, not worked around: the command deletes `storage-e2e/`
*before* `e2e.db*`, so a process killed mid-reset always leaves the database gone
and the credential (if any) still present momentarily — never the reverse. A
credential surviving alone points at a database that no longer exists, which the
embedded runner already handles today by seeing nothing to attach to and
self-provisioning (this is VI-C21's territory, not this card's). The dangerous
half-state — a fresh database with a credential naming a runner id it has never
seen — cannot occur from this ordering, only from killing the process between the
first `rm` finishing and the second one *starting*, which is not a state either
`rm` invocation can be interrupted mid-way (each is a single syscall per path,
not a tree walk that pauses).

## What is left

**`develop` has moved since this branch's base, and one gate depends on that.** The
coordinator's gate correction asked for `.githooks/pre-push` (fmt, clippy, generated
file freshness) as the definition of done. Running it: `check-comments.sh` and
`check-test-hygiene.sh` pass; `cargo clippy --workspace --all-targets -- -D
warnings` passes clean; `npm run gen:api` regenerates `schema.gen.ts` with **no
diff** (already fresh). `cargo fmt --all --check` fails — but every file it flags
(`executions.rs`, `attempt_scoping.rs`, `executions_runner_admin.rs`,
`model_policy_contract.rs`) is one I never touched, and the fix already exists on
`develop` as `96c0fb9` ("style: rustfmt the tree"), landed after this branch's base
`f06ce56` and folded into VI-C21's merge commit `8cd2987`. `git merge`/`git
cherry-pick` to pick that commit up were both refused by this session's own
auto-mode git-safety check (a merge/rebase needs explicit approval this
non-interactive session can't give). Since this card's own diff touches no Rust
file, merging this branch into a `develop` that already has `96c0fb9` — which is
exactly what integration does — resolves this without any action on this branch;
flagging it here rather than leaving it silent, since "the hook passes" is not
presently a true statement of this branch's tip in isolation.

**The a11y `color-contrast` defect this card's runs kept hitting** is not mine to
fix (out of ownership, no product file touched), but it is the reason "the suite
passes three times consecutively" is answered two ways above rather than one. Worth
its own card if nobody already owns it — VI-C13's handoff flagged the same token
(`--color-text-secondary`) against the same class of light background and did not
fix it either.

**The ~285-runner corroborating run was not clean** (unrelated load spike, detailed
above) — if a precise, load-controlled re-measurement of the full accumulation curve
past 225 matters later, it needs a machine not shared with whatever spawned those
loops.
