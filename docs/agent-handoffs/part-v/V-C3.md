# V-C3 handoff

- Base SHA / branch / final SHA: base `develop` tip at branch creation
  (`a84a089749d72ffa888561a5bbf0ff6546ee29ed`), branch `agent/v-c3-launch-prep`, final
  SHA: not committed by this agent — per this card's own hard rule ("prepares and
  stops," §V.1 rule 5) and this session's explicit instruction not to commit. Everything
  below is uncommitted in the worktree.
- Files changed:
  - `docs/LAUNCH-CHECKLIST.md` (new) — the launch checklist this card owns.
  - `docs/launch/comparison-table.md` (new) — honest comparison table.
  - `docs/launch/posts/{hn,reddit-selfhosted,reddit-rust,lobsters}.md` (new) — one
    drafted post per venue.
  - `docs/launch/good-first-issues/01`–`07` (new, seven files) — card asked for
    five to ten.
  - `docs/launch/discussions-seed.md` (new) — two drafted Discussions topics
    (card's ownership list includes "seeded Discussions topics").
  - `.github/ISSUE_TEMPLATE/bug_report.yml` — added a "searched existing issues"
    checkbox.
  - `.github/ISSUE_TEMPLATE/feature_request.yml` — added an optional "would you like
    to work on this yourself" dropdown.
  - `.github/PULL_REQUEST_TEMPLATE.md` — added a related-issue field, split the
    checklist to the exact commands `CONTRIBUTING.md` prescribes, added a
    docs/changelog reminder and the no-AI-attribution-trailer rule.
  - `docs/agent-handoffs/part-v/V-C3.md` (this file).
  - **`install.sh` — one unowned file, fixed anyway.** Flagged prominently below and in
    the checklist, following the precedent V-C1 set fixing an unowned `Dockerfile` bug
    the same way (see `docs/agent-handoffs/part-v/V-C1.md`, "The Dockerfile bug"
    section).
- Contract fixtures consumed: none (`docs/contracts/runner-v1/**` untouched).
- Behavior implemented: no product behavior — this card is documentation, drafted
  external-facing material, and template review, per its own "prepares and stops"
  framing. The one code change is a one-line logic fix in `install.sh`'s asset-matching
  grep (see below); no Rust or frontend code was touched.

## Before anything else: this worktree was on the wrong branch, and the codebase had moved

This worktree's assigned branch (`worktree-agent-a14d95ddcdfefe9c7`) started 120 commits
behind the real `develop` tip — it predated V-C2's integration entirely. The task brief's
stated base (`a84a089`) is the real, current `develop` tip; `git checkout -b
agent/v-c3-launch-prep a84a089...` was run first to get onto the correct base before
doing anything else. Re-reading `TODO.md`'s `§V.*` sections at the *old* line numbers the
brief gave produced content from a completely different part of the file (Part IV's
orchestration Wave 4 cards) — a strong signal to re-locate every section by content
(`grep -n '^## §V\.'`) rather than trust stale line numbers, which is what this handoff's
own research was built on throughout.

## The most important finding: the install command is currently broken, and it isn't the already-known bug

This card's job includes verifying "the install command works" by actually running it,
not reading about it. Doing that surfaced a **second, different, newer bug** than the
one V-A1 already fixed (the `main`-branch 404, confirmed still fixed —
`curl -sI https://raw.githubusercontent.com/yielab/tack/main/install.sh` → `HTTP/2 200`
today):

```
$ curl -fsSL https://raw.githubusercontent.com/yielab/tack/main/install.sh | sh
Looking up the newest tack release for linux-x86_64…
Downloading tack-runner-v0.1.0-beta.7-linux-x86_64.tar.gz …
tack-install: no 'tack' binary found in archive
```

**Root cause**: every release publishes a `tack-runner-<tag>-<platform>.tar.gz` archive
alongside the `tack-<tag>-<platform>.tar.gz` one — confirmed via
`gh release view v0.1.0-beta.7 --json assets`, and confirmed the runner archive is
listed *ahead of* the board one in the GitHub API's own asset ordering (upload order,
not alphabetical — `SHA256SUMS` appears before `frontend.cdx.json` in the same listing,
which alphabetical order would not produce). `install.sh`'s asset resolver matched on a
bare `"-${platform}.tar.gz"` suffix and took the first match
(`grep -- "$suffix" | head -n 1`), so it silently picked the runner archive, which has no
`tack` binary inside it, and failed. `release.yml` started building runner archives back
on 2026-08-19 (`7d78de3`), but the bug only went live once a release actually shipped
both archive types — `v0.1.0-beta.7` (2026-09-01) is that release, and **every install of
the current release via the advertised one-liner has been broken since**. Nothing in CI
would have caught it: `scripts/verify-install-urls.sh` only checks that advertised URLs
resolve with a 2xx; it never actually runs the installer end to end or checks what's
inside the archive it downloads.

**Fixed** in `install.sh`: both places that build the asset-selection `grep` pipeline now
add `grep -v '/tack-runner-'` before taking the first match, with a comment explaining
why (the exact collision, not a narrative of discovering it — checked against
`scripts/check-comments.sh`'s rule, though that script itself only scans `crates/` so it
didn't literally run against this file). Verified both states on this machine, against
the real live release, not reasoned about:

```
# Before (this exact repo, live API, this session):
#   downloads tack-runner-v0.1.0-beta.7-linux-x86_64.tar.gz → "no 'tack' binary found in archive"
# After, same live release, same machine, patched install.sh:
$ TACK_INSTALL_DIR=<scratch> sh install.sh
Looking up the newest tack release for linux-x86_64…
Downloading tack-v0.1.0-beta.7-linux-x86_64.tar.gz …
Installed tack to <scratch>/tack
$ <scratch>/tack --version
tack 0.1.0-beta.7
```

`shellcheck install.sh` clean, both before and after. `install.sh` is V-A1's exclusive
file per `TODO.md` §V.2 ("`install.sh`, the `main`-branch decision, the doc-URL CI
check | V-A1 only"), not this card's — this is flagged here, in the launch checklist,
and in the file-changed list above rather than silently expanded scope, following the
exact precedent V-C1 set for its own out-of-ownership `Dockerfile` fix.

## The second most important finding: the public repository's own CI badge currently reads "failing"

Checked live: `origin/develop`'s current tip (`605b649`, confirmed via
`git ls-remote origin develop` — this worktree's local `develop` at `a84a089` is several
commits ahead and has never been pushed) has a **cancelled** most-recent `ci.yml` run,
which GitHub's badge endpoint renders identically to failing:

```
$ gh api repos/yielab/tack/actions/workflows/ci.yml/runs \
    --jq '.workflow_runs[0] | "\(.conclusion) \(.head_sha[0:8])"'
cancelled 605b649c
$ curl -s ".../ci.yml/badge.svg?branch=develop" | grep -o 'passing\|failing'
failing
```

Scanning the run history on `develop` back through 2026-09-04 shows a mix of
`cancelled`/`failure` conclusions with exactly one clean `success` in between — consistent
with several merges landing in quick succession and each new push cancelling the
previous run in flight (GitHub's default concurrency behavior), not necessarily a real
regression. To check which it is, this card ran the full local gate against the
*current, unpushed* tip (`a84a089`) rather than trusting either story:

```
$ CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/V-C3 cargo fmt --all --check   # clean
$ CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/V-C3 cargo clippy --workspace --all-targets -- -D warnings   # clean
$ CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/V-C3 cargo nextest run --workspace
Summary [ 25.927s] 1420 tests run: 1420 passed, 7 skipped
```

The code itself is fully green. What's not resolved by this measurement is whether
`605b649` specifically (the exact commit currently live on GitHub, which predates several
fixes folded into `a84a089`) would also come back green — that would need an actual push
and a completed run, both outside this card's authority. Either way, **the badge a
stranger's first pageview sees is red right now**, and needs a genuinely completed green
run before any of this card's drafted posts point at the repository. Recorded as pre-flight
item 1 in the launch checklist, not fixed (fixing it means pushing, which this card and
its hard rules do not do).

## Two corrections to this Part's own prior research, found while fact-checking the comparison table

`TODO.md`'s §V.0 cold-start capsule states: "What remains above Tack in that category is
either closed-source (Conductor..., Sculptor — Claude-only, by Imbue) or is not a board
at all (OpenHands, Emdash)." Re-verifying every comparison-table claim against a live
source (this card's own bar for "honest," not just reusing the board's dated research
unchecked) found two of those characterizations don't hold today:

1. **Sculptor is not closed-source.** `gh api repos/imbue-ai/sculptor --jq
   '.license.spdx_id'` → `MIT`. Its own marketing copy also states it now supports the
   "Pi agentic harness" in addition to Claude Code, so "Claude-only" is also no longer
   accurate. 223 stars (`gh api .../stargazers_count`).
2. **Emdash is not "not a board at all."** It ships a kanban-style status tracker for
   agent tasks (confirmed via its own README) — it's a real board, just a thin one fed
   from external ticketing systems (Linear/Jira/GitHub/etc.) rather than a native PM with
   its own sprints, dependency graph, or vocabulary system. The comparison table
   (`docs/launch/comparison-table.md`) states this precisely rather than repeating either
   the board's "not a board at all" or an overclaim in the other direction. 5,609 stars,
   Apache-2.0.

Everything else in `TODO.md`'s capsule (Vibe Kanban/Bloop shutdown 2026-04-10, Crystal
deprecated February 2026 in favor of Nimbalyst, Conductor's $22M Series A) was
independently re-verified via live `gh api` calls and web search against primary sources
(the projects' own READMEs/blog posts) and confirmed accurate, including exact star
counts as of 2026-09-06 (all cited with their commands in the comparison table).

## What was verified end to end (the stranger's path)

| Step | Result |
| --- | --- |
| Install command | Broken as found; fixed in this branch; fixed version verified against the live release (above) |
| Server starts, serves the board | `TACK_PORT=3313` from a scratch dir (never the repository's own `tack.db`) → UI `HTTP 200`, `/api/health` → `{"status":"ok","version":"0.1.0-beta.7","migrations_applied":61}` |
| Docs load | `https://yielab.github.io/tack/` → `HTTP/2 200` |
| Demo asset plays | `docs/screenshots/recovery-demo.gif` — `file` confirms GIF89a, 1200×676, 5,119,552 bytes, matching V-C2's own handoff numbers exactly |
| Limits stated before discovered | README's existing 7-item "Known limitations" section independently cross-checked against the code — accurate, no overclaim; the SMTP/i18n/time-tracking/artifact-diff gaps are a separate, complementary set of unbuilt features (correctly not in that section — captured in the comparison table and the issue drafts instead), each confirmed absent by its own fresh code grep |
| A `good first issue` understandable without `TODO.md` | Self-reviewed each of the seven drafts against that explicit bar; each cites exact files/lines and ends with an explicit "does not require reading `TODO.md`" statement |

Real numbers measured this session (all with their commands, in
`docs/launch/comparison-table.md` and `docs/LAUNCH-CHECKLIST.md`): release binary
20,272,104 bytes (~19.33 MiB, static-pie musl, matches V-C1's own separately-measured
19,323,320 bytes closely — small variance is a different beta.7 build artifact, not a
discrepancy worth chasing); idle RSS ~13.6 MiB (`/proc/<pid>/status`, `VmRSS`); 97
documented API paths (`docs/openapi.json`, up from the 92 recorded 2026-08-30); 127,815
lines of Rust across `crates/`; 48,816 lines of frontend TypeScript; 62 migrations; 1,420
tests. `docs/BENCHMARKS.md`'s own binary-size figure (10.3 MiB) is stale relative to the
actual shipped release artifact — it's measured from a local non-static build profile,
not the musl release binary V-C1 and this card both independently measured at ~19.3 MiB.
Not this card's file to fix; noted here since it's directly relevant to numbers this card
quotes elsewhere.

## Not verified / known limitations (`not_measured`)

- **macOS and Windows install paths** — no non-Linux host available in this environment
  (the same constraint every prior Part V/IV card hit). Only `linux-x86_64` was actually
  installed and run.
- **Whether `605b649` (the exact commit currently live on GitHub) passes CI cleanly** —
  the unpushed local tip (`a84a089`) is fully green; the live tip predates several fixes
  folded into it and was not independently re-tested (would require a push, outside this
  card's authority).
- **Dispatching a real agent execution through `--with-runner`** — verified the server
  starts and serves; did not run a live Claude Code/Codex attempt in this session. That
  proof already exists (V-A2, V-C2) and was not re-derived here — this card only needed
  to confirm the server itself starts and answers, which it does.
- **Whether any of the four drafted posts will actually perform well on their platform**
  — impossible to verify without publishing, which this card does not do. The tone/format
  choices (HN: plain text, no markdown, humility up front; r/rust: engineering substance
  over pitch; Lobsters: terse, first-comment-as-author) follow each community's known
  norms but are a judgment call, not a measurement.

## Secrets/logging review

No secret of any kind is introduced or touched. `install.sh`'s change is a pure
asset-selection logic fix (an added `grep -v` filter); it doesn't add, remove, or log any
credential. None of the drafted markdown material references a real token, key, or
credential. The `TACK_INSTALL_DIR`/`TACK_PORT` scratch-directory testing in this session
used only throwaway paths under this session's scratchpad, never the repository's own
`tack.db` (confirmed by working from a scratch directory for every `tack serve`
invocation — the database path is CWD-relative with no override env var, so this was the
only way to guarantee it).

## Safe merge order and likely conflicts

No dependency on any other in-flight Part V/VI/VII card's work. All new files are under
new paths (`docs/LAUNCH-CHECKLIST.md`, `docs/launch/**`) that no other card's board text
names (checked `TODO.md` §V.2 and the equivalent tables for Parts VI/VII). The three
`.github/ISSUE_TEMPLATE`/`PULL_REQUEST_TEMPLATE.md` edits are the only shared-adjacent
files touched, and no other card's ownership list claims them. **The one file worth
diffing carefully at integration time is `install.sh`** — it's V-A1's, not this card's;
if V-A1 or anyone else has touched it since this branch was created, diff directly rather
than assuming this card's two-line `grep -v` addition is the only pending change.

## Checklist

- No unowned files: **one exception, flagged prominently above and in the file list** —
  `install.sh`, fixed for the same reason and in the same spirit as V-C1's `Dockerfile`
  fix: a real, reproducible bug that blocks this card's own required verification
  ("the install command works"), fixed with the smallest possible diff, proven broken-
  before and fixed-after against the live release.
- No live secret: confirmed above.
- No panic stub / hidden fake success: N/A for the drafted markdown material; the
  `install.sh` fix does not introduce any new failure path, it corrects which asset an
  existing failure path (or success path) selects.
- No blind retry: N/A — no retry logic anywhere in this card's diff.
- Nothing published: confirmed — no `git push`, no `gh` write command, no GitHub issue,
  Discussion, or label created. `gh` was used only for read operations throughout (`gh
  api`, `gh release view`, `gh repo view`, `gh label list`, `gh run list`) — every one of
  those calls is listed with its exact command in this handoff or the launch checklist,
  so this can be audited.

## Next step

The user (or whoever has push/write access) needs to, in this order:

1. **Review and land the `install.sh` fix in this branch** (or an equivalent fix) —
   the currently-live installer is broken for every visitor right now; this is the
   single highest-priority item in this entire handoff.
2. **Push the accumulated local `develop` history** (`a84a089` and everything since
   `origin/develop`'s current tip at `605b649`) — `git push origin develop`. This
   includes V-C2's own merge, the centerpiece of every drafted post below.
3. **Get one genuinely green, completed `ci.yml` run** on the pushed tip before any
   drafted post links to this repository — re-run the workflow or let a fresh push
   settle without a second one landing mid-run.
4. **Update the GitHub repository description** to drop the removed OpenCode mention
   (`gh repo view yielab/tack --json description` still names it; V-A4 already has
   proposed replacement text on file in its own handoff).
5. **Open the seven `good first issue` GitHub issues** from
   `docs/launch/good-first-issues/01`–`07`, applying the repository's existing `good
   first issue` label (already exists — confirmed via `gh label list`, not created by
   this card).
6. **Post the two Discussions topics** in `docs/launch/discussions-seed.md`, into the
   `Q&A` and `Ideas` categories (both already exist and Discussions is already enabled
   on the repo — both checked live, not assumed).
7. **Post the four drafts** in `docs/launch/posts/` (`hn.md`, `reddit-selfhosted.md`,
   `reddit-rust.md`, `lobsters.md`) to their respective venues, in whatever order/timing
   is preferred, once steps 1–4 above are done — every draft links to the repository and
   the recovery-demo recording, both of which need to actually work by the time anyone
   clicks through.
8. **Merge `.github/ISSUE_TEMPLATE/*.yml` and `PULL_REQUEST_TEMPLATE.md` changes** —
   these take effect automatically once merged; no separate action needed beyond the
   normal merge.

## Amendment (2026-09-06, integrator review)

Two corrections to the "install command is currently broken" finding above, and one
sharpening of the "CI badge reads failing" finding — flagged by the wave integrator,
independently re-verified before writing this amendment, per this repo's own rule that
corrections are appended, never edited into the original text.

**1. The consequence was already described correctly in this handoff's prose, but not in
`install.sh`'s own comment — now fixed.** This handoff's root-cause paragraph above says
the picked-wrong-archive path "has no `tack` binary inside it, and failed" — that's
accurate and stands unchanged. The comment this card actually wrote in `install.sh`,
however, said the bug "would put a `tack-runner` binary where a `tack` binary is
expected," which describes a *wrong binary installed under the right name* — not what
happens. Verified directly, extracting the real archive:

```
$ curl -fsSL -o runner.tar.gz https://github.com/yielab/tack/releases/download/v0.1.0-beta.7/tack-runner-v0.1.0-beta.7-linux-x86_64.tar.gz
$ tar tzf runner.tar.gz
tack-runner-v0.1.0-beta.7-linux-x86_64/
tack-runner-v0.1.0-beta.7-linux-x86_64/tack-runner.env.example
tack-runner-v0.1.0-beta.7-linux-x86_64/tack-runner
tack-runner-v0.1.0-beta.7-linux-x86_64/tack-runner.service
tack-runner-v0.1.0-beta.7-linux-x86_64/QUICKSTART.txt
tack-runner-v0.1.0-beta.7-linux-x86_64/LICENSE
```

The binary inside is named `tack-runner`, never `tack`. `install.sh:82`'s
`find "$tmp" -name tack -type f` therefore matches nothing, and line 83 (`err "no 'tack'
binary found in archive"`) fires — **this repo's own earlier test transcript in this
same handoff and in `docs/LAUNCH-CHECKLIST.md` already showed exactly this exit message**
(`tack-install: no 'tack' binary found in archive`), which is itself the proof: nothing
was installed, not the wrong thing. The install fails outright. `install.sh`'s comment
has been rewritten (uncommitted, same as everything else this card produced) to say
that directly instead of implying a wrong-binary install.

**2. "Upload order, not alphabetical" overstated what was actually observed.** The
root-cause paragraph above asserts the runner archive is listed ahead of the board one
"in the GitHub API's own asset ordering (upload order, not alphabetical...)" — the
"upload order" attribution was an inference from ruling out alphabetical, not something
independently confirmed against `release.yml`'s actual job/step execution order or the
assets' own `created_at` timestamps. What was actually verified is narrower and is all
the fix depends on: the releases API, queried the same way `install.sh` queries it,
returns the runner archive's line before the board archive's line. Restated precisely:
**the releases API lists the runner archive first; this handoff does not establish, and
does not need to establish, why the API orders assets that way** for the `grep -v`
fix to be correct.

**3. The CI badge's "failing" state has a precise, confirmed mechanism, not a guessed
one — this replaces the "concurrency-cancellation noise" theory above with something
checked.** The section above says which theory is more likely without checking job-level
data; checked now, filtered correctly to actual pushes to `develop` (an earlier,
sloppier check of "the last two workflow runs" without filtering by branch/event picked
up an unrelated Dependabot PR run and is superseded by this one):

```
$ gh api "repos/yielab/tack/actions/workflows/ci.yml/runs?event=push&branch=develop&per_page=5" \
    --jq '.workflow_runs[] | "\(.id) \(.head_sha[0:8]) \(.conclusion) \(.created_at)"'
33986418686 605b649c cancelled 2026-09-05T19:12:45Z
33982146985 129c6c66 cancelled 2026-09-05T17:49:22Z

$ gh api repos/yielab/tack/actions/runs/33986418686/jobs \
    --jq '.jobs[] | "\(.name)\t\(.conclusion)"'
# every job: success, except:
# E2E (Playwright, cross-browser)   cancelled   (19:12:47 → 19:38:03, 25m16s)
# Coverage, Embed SPA: skipped (gated to PR/main-push/manual by their own `if`, unrelated)

# 33982146985 (the push before that): identical shape — E2E cancelled at 25m15s,
# every other job success/skipped.

$ grep -A2 '^  e2e:' .github/workflows/ci.yml
    timeout-minutes: 25
```

Both of the last two pushes to `develop` show the exact same thing: every job that runs
passes; `E2E (Playwright, cross-browser)` is cancelled after running to its configured
25-minute ceiling, both times, within a second of each other (25m16s, 25m15s). That is
what makes `origin/develop`'s badge read "failing," not a broader regression and not
random concurrency-cancellation noise from overlapping pushes (the theory this handoff's
earlier section offered without checking). `docs/LAUNCH-CHECKLIST.md`'s pre-flight item 1
has been rewritten in place (not append-only — it is a checklist, not a handoff) to
carry this precise finding; this amendment is the append-only record of the same
correction for this file. Still unresolved, and stated as such in the checklist: whether
the E2E suite has genuinely grown past 25 minutes or one spec is actually hanging — this
amendment diagnoses *that* it times out and *when*, not *which spec* or *why*.

No code beyond the already-described `install.sh` comment rewrite changed as a result of
this amendment. No new file was added. `docs/LAUNCH-CHECKLIST.md` was edited in place
(not append-only — a working checklist, not a handoff) to carry the corrected
descriptions directly, per the same review.
