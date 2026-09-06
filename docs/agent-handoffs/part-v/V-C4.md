# V-C4 handoff

- Base SHA / branch / final SHA: this worktree's assigned branch
  (`worktree-agent-a13e863c46c9f09d7`) started at `e5206c7`, well behind the real
  `develop` tip — the same "wrong branch" pattern two other cards in this wave hit.
  `git checkout -b agent/v-c4-checks-that-measure 3085ebb` (this card's stated base,
  confirmed to be `develop`'s actual tip) was run first, before touching anything else.
  Branch `agent/v-c4-checks-that-measure`, final SHA: not committed (this session's
  instructions: no commit/push/merge/rebase — everything below is uncommitted in the
  worktree).
- Files changed (matches this card's ownership — the `e2e` job in `ci.yml`,
  `verify-install-urls.yml`, `verify-install-urls.sh`, and this handoff):
  - `.github/workflows/verify-install-urls.yml` — header comment updated to describe the
    new behavior below; no trigger/permissions/timeout change.
  - `scripts/verify-install-urls.sh` — after the existing URL-resolution sweep, actually
    runs `install.sh` into a scratch directory and asserts a real, runnable `tack` binary
    lands.
  - `.github/workflows/ci.yml` — `e2e` job's `timeout-minutes: 25` → `40`, with a comment
    pointing at why (see "The budget decision" below).
  - `docs/agent-handoffs/part-v/V-C4.md` (this file).
  - `install.sh` — **not touched.** Confirmed byte-identical to `develop` (`git diff
    develop -- install.sh` empty, `sha256sum` matched) at the start and end of this
    session. It was reverted and restored exactly once, in place, purely to prove the
    install check (see below), never left modified.

## One — the install check, proven load-bearing

`scripts/verify-install-urls.sh` used to do exactly one thing: resolve every URL the docs
advertise (the raw `install.sh` URL, the releases page) and fail if any didn't return 2xx.
That is real, but it is not the same claim as "the install command works" — a URL can
resolve while the script behind it still downloads the wrong file and dies. That's exactly
what happened: every release publishes both `tack-<tag>-<platform>.tar.gz` and
`tack-runner-<tag>-<platform>.tar.gz`, sharing the same `-${platform}.tar.gz` suffix, and
the GitHub releases API lists the runner one first. Confirmed live this session, against
the real repository (no mock):

```
$ gh release view v0.1.0-beta.7 --repo yielab/tack --json assets --jq '.assets[].name'
...
tack-runner-v0.1.0-beta.7-linux-x86_64.tar.gz
...
tack-v0.1.0-beta.7-linux-x86_64.tar.gz
...
$ curl -s 'https://api.github.com/repos/yielab/tack/releases?per_page=20' \
    | grep -o '"browser_download_url": *"[^"]*linux-x86_64.tar.gz"'
"browser_download_url": ".../tack-runner-v0.1.0-beta.7-linux-x86_64.tar.gz"
"browser_download_url": ".../tack-v0.1.0-beta.7-linux-x86_64.tar.gz"
"browser_download_url": ".../tack-v0.1.0-beta.6-linux-x86_64.tar.gz"
```

The runner archive really does sort first. `install.sh`'s fix (already landed, not this
card's file) adds `grep -v '/tack-runner-'` to both asset-selection pipelines. The check
never exercised either branch of that pipeline — it only asked "does a URL resolve,"
never "what does the script that URL serves actually do."

**Fix**: `scripts/verify-install-urls.sh` now runs the real installer after the URL
sweep — `TACK_INSTALL_DIR=<scratch dir> sh install.sh` — and asserts a working `tack`
binary lands (`[ -x "$install_dir/tack" ]` and a successful `--version`). The
URL-resolution half is untouched.

### The acceptance, run fresh: revert the fix once, watch the check fail on the exact bug, restore it

```
$ git diff develop -- install.sh                          # before: confirm clean
(empty)

$ ./scripts/verify-install-urls.sh                         # baseline: green
...
Running install.sh for real (not just resolving its URL):
Looking up the newest tack release for linux-x86_64…
Downloading tack-v0.1.0-beta.7-linux-x86_64.tar.gz …
Installed tack to /tmp/tmp.LmluR4cbY1/tack
OK   install.sh installed a working tack: tack 0.1.0-beta.7
$ echo $?
0

$ sed -i "s/ | grep -v '\/tack-runner-'//g" install.sh     # revert, deliberately, for this proof only
$ git diff install.sh                                      # confirms only the two grep -v filters were removed
--- a/install.sh
+++ b/install.sh
@@
-  url="$(printf '%s\n' "$urls" | grep "/download/$VERSION/" | grep -- "$suffix" | grep -v '/tack-runner-' | head -n 1 || true)"
+  url="$(printf '%s\n' "$urls" | grep "/download/$VERSION/" | grep -- "$suffix" | head -n 1 || true)"
   [ -n "$url" ] || err "no asset for $VERSION on $platform"
 else
-  url="$(printf '%s\n' "$urls" | grep -- "$suffix" | grep -v '/tack-runner-' | head -n 1 || true)"
+  url="$(printf '%s\n' "$urls" | grep -- "$suffix" | head -n 1 || true)"

$ ./scripts/verify-install-urls.sh                         # the exact regression, live
...
Running install.sh for real (not just resolving its URL):
Looking up the newest tack release for linux-x86_64…
Downloading tack-runner-v0.1.0-beta.7-linux-x86_64.tar.gz …
tack-install: no 'tack' binary found in archive
FAIL install.sh exited non-zero — the advertised install command is broken
$ echo $?
1

$ git checkout -- install.sh                               # restore
$ git diff develop -- install.sh                           # empty: byte-identical to develop
$ sha256sum install.sh /tmp/install.sh.known-good
9f2316950058f5cd9433650664a3af9ac32c9f9f36ad885c5d8bfd677fe4c6fd  install.sh
9f2316950058f5cd9433650664a3af9ac32c9f9f36ad885c5d8bfd677fe4c6fd  /tmp/install.sh.known-good

$ ./scripts/verify-install-urls.sh                         # green again
...
OK   install.sh installed a working tack: tack 0.1.0-beta.7
$ echo $?
0
```

That is the actual bug (downloads the runner archive, `no 'tack' binary found in
archive`), reproduced by the check that's supposed to catch it, without editing the check
itself between the fail and the pass — only `install.sh` moved, and only for the length
of this proof. `install.sh` in the delivered tree is byte-identical to `develop` (diff
and checksum both empty/matching above).

`shellcheck scripts/verify-install-urls.sh` — clean, before and after this change. Total
run time for the whole script (URL sweep + real install): ~3s, well inside the job's
existing `timeout-minutes: 10` — no timeout change needed for that job.

## Two — the E2E job's budget

### The badge: what's actually cancelling, and why "cancelled" doesn't mean "nothing was failing"

Checked against real, live `develop` history (`gh api
repos/yielab/tack/actions/workflows/ci.yml/runs`), not assumed:

| Run | `head_sha` | Job conclusion | `Run E2E tests` step window |
|---|---|---|---|
| 33986418686 | `605b649c` | `cancelled` | 19:16:15Z → 19:38:00Z (**21m45s**, cut off mid-run by the job's own 25 min ceiling; job total 25m16s) |
| 33982146985 | `129c6c66` | `cancelled` | 17:53:11Z → 18:14:53Z (**21m42s**, same pattern; job total ~25m12s) |
| 33981964392 | `7e5a05e9` | `cancelled` | 17:49:05Z → 17:49:35Z (only 30s — superseded by the next push into the same `concurrency` group, `cancel-in-progress: true`; not a timeout, discarded) |

Both genuine-timeout runs show the same shape: ~3m26s-3m28s of fixed overhead before the
test step even starts (checkout, toolchain, rust-cache, pre-build server, node setup,
frontend deps, `npx playwright install --with-deps` at `ci.yml:451`), then the test step
is still running, unfinished, when the 25-minute ceiling lands.

**A "cancelled" conclusion is exactly what a suite with a real, unfixed failure inside it
would also produce here, not evidence against one.** `frontend/playwright.config.ts` sets
no `maxFailures`/bail; Playwright always runs every project to its natural completion
regardless of earlier failures (confirmed empirically below — this session's own local
runs kept going through dozens of failing tests without stopping). So a job that never
finishes within its 25-minute ceiling looks identical whether the delay is all "happy
path" content or partly a test that deterministically burns its own retries — either way,
the external timeout fires before Playwright ever gets to report its own conclusion, and
GitHub renders that the same red as `failure`. This card cannot independently confirm
whether the specific failure found below (scheduler-e2e.spec.ts) reproduces on GitHub's
own `ubuntu-latest` runner — that would need an actual push, outside this card's
authority — but the "cancelled, not failed" shape of the real history is consistent with
it, not evidence against it.

### What the suite actually spends its time on, measured on this machine

`make e2e` (`npm run test:e2e` → `playwright test`, no project filter) with `CI=true` set
(mirrors GitHub Actions exactly — `playwright.config.ts` branches on `process.env.CI` for
`workers`/`retries`/`reporter`), pre-built server warmed first (`cargo build -p
tack-cli`), `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/V-C4`.

**First finding, not a timing one: this measurement machine's Playwright browser cache
was stale for two of the three engines.** `~/.cache/ms-playwright` (shared across every
card/session on this box, per this card's own hard rule never to delete it) held
`firefox-1532`/`webkit-2311`, but the `playwright@1.62.1` package this worktree's fresh
`npm ci` installed wants `firefox-1538`/`webkit-2336`. The first full run showed every
firefox/webkit test failing in ~1ms with `Error: browserType.launch: Executable doesn't
exist at .../webkit-2336/pw_run.sh` — a local-environment version mismatch, not a real
product or CI finding (CI's own `npx playwright install --with-deps` step always installs
the exact versions its own `npm ci` just pulled, so this specific mismatch cannot happen
there). Fixed *additively* — `npx playwright install firefox webkit` downloads the
missing versions **alongside** the existing cached ones; nothing in the cache was
deleted.

**Second finding: webkit still cannot run here, for a different and unfixable-on-this-box
reason, and that is a fact about this sandbox, not about webkit.** With the correct
`webkit-2336` installed, launching it now fails on missing shared libraries:

```
$ ldd ~/.cache/ms-playwright/webkit-2336/minibrowser-gtk/bin/MiniBrowser 2>&1 | grep "not found"
	libbacktrace.so.0 => not found
	libjxl.so.0.8 => not found
	libavif.so.16 => not found
```

`ci.yml:451` (`npx playwright install --with-deps`) is exactly the step that installs
these system libraries in the real job — `--with-deps` runs `apt-get install` under root.
This sandbox has no passwordless `sudo` (`sudo -n true` fails), so that step cannot be
run here. **This is a gap in this measurement environment, not a property of webkit or of
the product — recorded as `not measured here`, not as a webkit finding**, precisely
because this project has been bitten before by a sandbox's own edge reported as a fact
about the code.

**Real, per-project timings — chromium and firefox, both measured; webkit, not
measurable on this box:**

| Project | Measured? | Real tests executed | Result | Summed test time |
|---|---|---|---|---|
| chromium | yes | 81 (list-reporter lines, verified count) | 77 passed, 4 failed (`scheduler-e2e.spec.ts`, all 3 attempts each) | **503.7s = 8.40 min** (summed from each test's own reported duration) |
| firefox | yes | 37 (23 passed, 4×3 failed attempts, 1×2 flaky attempts) | 1 flaky (`provider-key-panel.spec.ts` — see below), 4 failed (`scheduler-e2e.spec.ts`, same file, same shape) | **420.0s = 7.00 min** |
| webkit | **no** — `ldd`-confirmed missing system libraries this sandbox cannot install (see above) | — | — | **not measured** |

Plus a **once-per-job** fixed overhead of ~3.45 min (measured twice from real CI history
above, 206s/208s — this part is not extrapolated, it's the actual GitHub Actions
timeline), paid before any project's tests start.

**The dominant, load-bearing, reproducible cost is `scheduler-e2e.spec.ts`, and it is a
real failure, not a flake.** All 4 of its tests fail deterministically — every one of the
3 CI-configured attempts (1 try + 2 retries) — on **both** measurable browsers, at
~30.0-31.1s each (the test's own `timeout: 30_000`, not the assertion-level 10s
`expect.timeout` — some `await` in the shared setup path hangs past its own implicit
ceiling until the outer test timeout fires). That's 4 tests × 3 attempts × ~30s ≈ **6
minutes per browser project**, from one file, reproduced identically on chromium and
firefox. Traced (not fixed — this is application code, out of this card's scope) to a
step every one of the file's 4 tests shares: `await
expect(modal.getByText('Supported'|'Unsupported', { exact: true
})).toBeVisible()` after choosing a model in `RunWithAgentModal`'s live-capability gate.
`agents-page.spec.ts`'s "step 5" test proves the underlying claim→queued→leased pipeline
itself works fine (passes in 2.5-5.1s on both chromium and firefox) — its "Test run" form
doesn't go through this same "Choose a model" control, which narrows the failure to that
one UI gate, not the scheduler/runner-protocol pipeline in general. **Not independently
confirmed against GitHub's own runner** — this card doesn't push — but it is real,
deterministic, and reproduced on two different browser engines on this machine, so it is
handed over as a finding, not dismissed as an artifact of this sandbox.

**`provider-key-panel.spec.ts`'s firefox failure is a one-off UI-interaction flake, not a
credential leak — read against exactly what it asserts, not assumed:**

```
frontend/e2e/provider-key-panel.spec.ts:52   await page.locator('form').getByRole('button', { name: 'Save' }).click();
frontend/e2e/provider-key-panel.spec.ts:57   expect(await page.content()).not.toContain(secretValue);   # the leak assertion
```

The captured failure is `locator.click: Test timeout of 30000ms exceeded ... waiting for
locator('form').getByRole('button', { name: 'Save' })` — line 52, the click that starts
the save flow. The test never reached line 57, the assertion that the pasted secret never
appears in the page's HTML. So this is not evidence the Vercel AI Gateway key leaks into
the DOM under firefox; it's the Save button not becoming clickable within 30s on the
first attempt. It passed clean, fast, on retry #1 (2.7s) — the signature of a transient
UI-interaction race, not a deterministic product defect (contrast directly against
`scheduler-e2e.spec.ts` above, which fails all 3 attempts every time). Costs ~33s once
when it happens; doesn't recur predictably like the scheduler-e2e failures do.

### The budget decision

Central estimate, using only what's actually measured plus the one real (not guessed)
CI-history number:

```
fixed overhead (measured, real CI history):    ~3.45 min
chromium (measured, this machine):              8.40 min
firefox  (measured, this machine):              7.00 min
webkit   (not measured — sandbox gap above):    not available
                                                ─────────
measured-only subtotal (2 of 3 projects):      18.85 min
```

**Webkit cannot be measured here, so the total for all three projects is an
extrapolation, labeled as one, not a measurement.** But the decision does not actually
need webkit's exact number: the two projects that *are* measured, plus the one real
fixed-overhead figure from actual CI history, already sum to **18.85 minutes** — and
webkit still has to run a comparable amount of identical test content (same specs, same
assertions, very likely the same deterministic `scheduler-e2e.spec.ts` failure, since
nothing about that failure's traced cause — a UI live-capability-gate assertion — is
rendering-engine-specific). Even a webkit leg implausibly *faster* than firefox's measured
7.00 min pushes the running total past 25 minutes on its own; a webkit leg comparable to
chromium's 8.40 min (the more likely case, given identical content) puts the full-suite
estimate at **~27.3 minutes** — already over the current ceiling before any GitHub-runner
variance, network latency for the browser downloads, or an occasional extra flake (like
`provider-key-panel`'s) is added on top.

**Conclusion: the budget is wrong, not the suite.** `timeout-minutes: 25` did not fit even
under the most conservative reading available from real measurements. Changed to **`40`**
in `ci.yml` — enough margin above the ~27.3-minute central estimate to absorb real
CI-runner variance and the parts of this that couldn't be measured on this machine,
without being an arbitrary round-up. If `scheduler-e2e.spec.ts`'s failure is fixed later
(out of this card's scope), the suite's real cost would drop by roughly 18 minutes across
three browsers (6 min × 3), and `40` would then carry generous slack rather than being
tight — a follow-up card can re-measure and tighten it at that point; this card sizes to
the suite as it actually runs today.

## Not measured / handed over

- **webkit's real timing** — not measurable on this machine: its browser binary is
  missing system libraries (`libbacktrace.so.0`, `libjxl.so.0.8`, `libavif.so.16`) that
  only `ci.yml:451`'s `npx playwright install --with-deps` can install, and that step
  needs root, which this sandbox does not have (no passwordless `sudo`). This is a gap in
  the measurement environment, not a finding about webkit or the product. A CI push (or
  any host with the system libraries installed) would close this.
- **Whether `scheduler-e2e.spec.ts`'s failure reproduces on GitHub's own runner** — real,
  deterministic, and reproduced on two different browser engines on this machine, against
  this worktree's checkout of `develop`'s tip, but never independently run through actual
  CI (this card doesn't push). The "cancelled, not failed" shape of the real run history
  is consistent with this finding, not proof of it.
- **Whether `scheduler-e2e.spec.ts`'s failure is pre-existing on `develop` or newly
  introduced, and its root cause inside `RunWithAgentModal`'s live-capability gate** — not
  bisected or fixed. That's frontend/application work, outside this card's "the budget is
  yours, the code under test is not" boundary. Handed to whoever owns E2E test health
  next — this is very likely the single biggest lever on the job's actual runtime, bigger
  than any `timeout-minutes` number this card could pick.

## What the user should expect when they push

Nothing here has run through GitHub's own `ubuntu-latest` runner — everything above was
measured or extrapolated locally. When `origin/develop` moves to include this card's
changes: *Verify install URLs* should go green in about the same ~10s plus a few seconds
for the real install step, and should be seen to **fail** if anyone ever reverts
`install.sh`'s `grep -v` filters again — that's the whole point of today's change. *E2E
(Playwright, cross-browser)* should now get the room to actually finish inside its new
40-minute ceiling instead of being cut off mid-run — but finishing is not the same as
passing: `scheduler-e2e.spec.ts`'s deterministic failure is real on this machine and, if
it also reproduces on GitHub's runner, the job will very likely report a genuine `failed`
conclusion instead of `cancelled` on the next push. That would be the more honest outcome
this card was asked to produce, even though it isn't a green badge — a red badge that
means something beats a red badge that means "we ran out of time to find out."
