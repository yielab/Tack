# Launch checklist

Prepared by card V-C3. **Nothing in this document has been published.** Every item
below was either verified by actually doing it on this machine (a real command, a real
install, a real HTTP request) or is explicitly marked as not verified. Where something
needs a human action outside this repository — a `git push`, a subreddit post, opening a
GitHub issue — it's listed under "Blocking pre-flight" or "Publish list", never done.

## Blocking pre-flight — do these before publishing anything

These aren't nice-to-haves; publishing before they're resolved sends the audience this
card is written for straight into a broken first impression.

**Three of the four have since been resolved. The detail of each is kept below as the
record of what was wrong and how it was found.**

| # | Item | State |
| --- | --- | --- |
| 1 | CI badge reads "failing" | **Resolved.** A push to `develop` now completes with all ten jobs green, E2E included; the E2E budget was raised on measured numbers rather than by guessing |
| 2 | Local `develop` ahead of the remote | **Resolved.** `origin/develop` matches the local tip |
| 3 | `install.sh` downloads the runner archive and fails | **Resolved.** The asset match excludes `tack-runner-` archives, and `scripts/verify-install-urls.sh` now runs the installer for real into a scratch directory and asserts a runnable `tack` lands, so this class of bug fails a check instead of a stranger's first command |
| 4 | The live GitHub repository description names a removed harness | **Open.** One command, listed in the publish list below |



### 1. The public repository's CI badge currently reads "failing"

Checked live, 2026-09-06, filtered to actual pushes to `develop` (not just the two most
recent workflow runs overall — those included an unrelated Dependabot PR run, discarded
below): the last two pushes to `develop` both show every job succeeding **except** `E2E
(Playwright, cross-browser)`, which was cancelled in both after running its full
configured timeout:

```bash
gh api "repos/yielab/tack/actions/workflows/ci.yml/runs?event=push&branch=develop&per_page=5" \
  --jq '.workflow_runs[] | "\(.id) \(.head_sha[0:8]) \(.conclusion) \(.created_at)"'
# → 33986418686 605b649c cancelled 2026-09-05T19:12:45Z   (current origin/develop tip)
# → 33982146985 129c6c66 cancelled 2026-09-05T17:49:22Z   (the push before that)

gh api repos/yielab/tack/actions/runs/33986418686/jobs \
  --jq '.jobs[] | "\(.name)\t\(.conclusion)\t\(.started_at)\t\(.completed_at)"'
# → every job success/skipped except:
#   E2E (Playwright, cross-browser)   cancelled   19:12:47 → 19:38:03   (25m16s)
# (33982146985's jobs show the identical pattern: E2E cancelled at 25m15s, everything
#  else success/skipped)

grep -A2 '^  e2e:' .github/workflows/ci.yml
# → timeout-minutes: 25

curl -s "https://github.com/yielab/tack/actions/workflows/ci.yml/badge.svg?branch=develop" \
  | grep -o 'passing\|failing'
# → failing
```

`Coverage` and `Embed SPA` show as `skipped`, not failed, in both runs — they're gated
by their own `if` condition to pull requests, pushes to `main`, and manual runs, so a
plain push to `develop` never runs them; this is expected, unrelated to E2E, and not a
second problem. **The badge reads "failing" because one job — E2E — reliably times out
at exactly its configured 25-minute ceiling on `develop`, not because the build is
broken**: `Rust (fmt + clippy + test)`, `Frontend`, `Desktop app`, `Docs`, `MSRV`,
`Security`, and `cargo-deny` all pass, both times. That's a materially different, more
tractable problem than "CI is red" suggests — either the E2E suite has genuinely slowed
past 25 minutes (raise `timeout-minutes`, after confirming that's not masking a real
hang) or something in it is actually hanging (diagnose which spec). Either way, **the
badge itself needs a genuinely green completed run before any post links to this
repository** — this is pre-flight regardless of which of those two it turns out to be.

### 2. Local `develop` is ahead of `origin/develop` — nothing has been lost, but the public repo is stale

```bash
git ls-remote origin develop        # 605b649c...
git rev-parse develop               # a84a089... (this worktree's base, several commits ahead)
git merge-base --is-ancestor <remote-sha> <local-sha> && echo ok   # ok — local is a fast-forward of remote
```

The commits sitting locally-only include V-C2's own merge (the recovery-demo GIF and
its recording harness) and several board-bookkeeping commits after it. **This repo's own
worktree/agent discipline is why**: agents in this workspace don't push (that's this
card's own hard rule too). Someone needs to push the accumulated `develop` history
before V-C2's recording — the centerpiece of the demo pitch every draft below points at
— is even visible to a stranger who clones the repo.

### 3. The install command had a real, undiscovered bug — now fixed in this card's branch

Not a re-check of the already-fixed "branch didn't exist" 404 (that's V-A1, done, and
confirmed still fixed — `curl -sI https://raw.githubusercontent.com/yielab/tack/main/install.sh`
returns `HTTP/2 200` today). This is a **different, newer** bug, found by actually
running the installer against the live release, not by reading it:

```bash
$ curl -fsSL https://raw.githubusercontent.com/yielab/tack/main/install.sh | sh
Looking up the newest tack release for linux-x86_64…
Downloading tack-runner-v0.1.0-beta.7-linux-x86_64.tar.gz …
tack-install: no 'tack' binary found in archive
```

Every release also publishes a `tack-runner-<tag>-<platform>.tar.gz` archive, which
ends in the same `-linux-x86_64.tar.gz` suffix the installer greps for. The releases
API returns it before the `tack-<tag>-<platform>.tar.gz` one the installer actually
wants (confirmed via `gh release view v0.1.0-beta.7 --json assets` — this is the order
the API lists assets in, not a confirmed claim about the release job's upload order). A
bare suffix match picks the runner archive first, downloads it, and then fails to find
a `tack` binary inside it — confirmed by extracting that archive directly
(`tar tzf tack-runner-v0.1.0-beta.7-linux-x86_64.tar.gz`): every path inside is under
`tack-runner-v0.1.0-beta.7-linux-x86_64/`, and the binary in it is named `tack-runner`,
never `tack`. The install fails outright; it does not install the wrong binary under
the right name. `release.yml`
started building runner archives back on 2026-08-19 (`7d78de3`), but the bug only became
live the moment a release was actually cut with both archive types present — that's
`v0.1.0-beta.7` (2026-09-01, the first tag cut after that change; `beta.6` predates it and
never had a colliding asset). **Every install since `beta.7` shipped has been broken this
way**, and nothing in CI would have caught it: `scripts/verify-install-urls.sh`
only checks that the advertised URLs resolve with a 2xx, never that the installer
actually extracts a working `tack` binary from what it downloads.

**Fixed in this branch** (`install.sh`, excluding `tack-runner-` assets from the match).
Verified both states, not just reasoned about them:

```bash
# Before the fix (reproduced against the live release, this exact repo, this session):
#   downloads tack-runner-v0.1.0-beta.7-linux-x86_64.tar.gz → "no 'tack' binary found in archive"
# After the fix, same live release, same machine:
$ TACK_INSTALL_DIR=/tmp/.../install-test sh install.sh
Looking up the newest tack release for linux-x86_64…
Downloading tack-v0.1.0-beta.7-linux-x86_64.tar.gz …
Installed tack to /tmp/.../install-test/tack
$ /tmp/.../install-test/tack --version
tack 0.1.0-beta.7
```

`install.sh` is V-A1's exclusive file per this Part's ownership table
(`TODO.md` §V.2), not this card's — flagged prominently here and in the handoff rather
than silently expanded scope, following the precedent V-C1 set fixing an unowned but
release-blocking `Dockerfile` bug the same way. This is left uncommitted in this
worktree per this card's own hard rules; someone needs to actually land it before the
install command in any of the drafts below is true.

### 4. The live GitHub repository description still names a removed harness

```bash
gh repo view yielab/tack --json description --jq .description
# → "Tack assigns board items to AI coding agents — Codex, Claude Code, or OpenCode —
#    and keeps the run as part of the project's history, in one self-hosted binary."
```

`opencode.rs` was removed from `crates/tack-runner/src/harness/` on 2026-09-05 (ADR
0063, decision 8) — confirmed by its absence from that directory today. The live
description is stale by one day relative to `develop`. This is V-A4's file to change
(`TODO.md` §V.2: "GitHub repo description / topics / homepageUrl... V-A4 only"), not
this card's — noted here as a pre-flight item, not fixed.

## What was verified end to end on this machine (stranger's path)

Every claim below is a command actually run in this session, not a re-statement of
another card's handoff.

| Step | Result | Command |
| --- | --- | --- |
| Install command resolves and runs (after the fix in item 3 above) | ✅ real `tack 0.1.0-beta.7` binary installed and runs `--version` | see item 3 |
| Server starts and serves the board | ✅ `HTTP 200` on `/`, `{"status":"ok","version":"0.1.0-beta.7","migrations_applied":61}` on `/api/health` | `TACK_PORT=3313 ./tack serve` from a scratch dir, then `curl` both endpoints |
| Docs load | ✅ `https://yielab.github.io/tack/` → `HTTP/2 200` | `curl -sI https://yielab.github.io/tack/` |
| The demo asset exists and is a valid, playable image | ✅ `docs/screenshots/recovery-demo.gif`, GIF89a, 1200×676, 5,119,552 bytes — matches V-C2's own handoff numbers | `file docs/screenshots/recovery-demo.gif` |
| Limits are stated, not just claimed | ✅ README's existing "Known limitations" section (7 items — shared token, one SQLite writer, no offline mode, legacy docket bridge, imported-cost estimates, no mobile app, not code-signed) was independently cross-checked against the code and found accurate, no overclaim; the four unbuilt-feature gaps this card's `good first issue` drafts are grounded in (SMTP, i18n, time tracking, artifact diff) are a different, complementary set — correctly absent from a "known limitations of the shipped product" section, captured instead in the comparison table and the issue drafts | manual read of the README section + independent `grep`/source read for each of the eleven distinct items across both sets |
| A `good first issue` draft needs no `TODO.md` context | ✅ each of the seven drafts under `docs/launch/good-first-issues/` cites exact files/line numbers and closes with "does not require reading this repository's internal planning board" | self-review of each draft against that bar |
| Full local gate is green (unpushed `develop` tip, `a84a089`) | ✅ `cargo fmt --all --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; 1,420 tests passed, 7 skipped, 0 failed | `cargo nextest run --workspace`, `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/V-C3` |

Not verified — stated as `not measured`, not rounded up:

- **macOS and Windows install paths.** No non-Linux host available in this environment
  (the same constraint every prior Part V card hit). Only the `linux-x86_64` install
  path was actually run.
- **The `--with-runner` embedded-runner path end to end with a real harness call.**
  Verified the server starts and serves; did not dispatch a real Claude Code/Codex
  execution in this session (that's V-A2's and V-C2's already-recorded proof, not
  re-derived here).
- **Whether the E2E job's 25-minute timeout (item 1) is a genuinely slow suite or an
  actual hang in one spec.** Confirmed *that* it times out at its configured ceiling on
  both of the last two pushes to `develop`; did not run the Playwright suite locally in
  this session to isolate which spec, so this is diagnosed as far as the run data goes,
  not further. `cargo nextest run --workspace` (Rust unit/integration tests) is fully
  green on the local, unpushed tip — that gate doesn't include the E2E/Playwright suite
  at all, so it says nothing about this specific job one way or the other.

## Material prepared (drafts only, nothing posted)

- `docs/launch/comparison-table.md` — honest comparison against every real competitor
  named in this Part's context, including where Tack itself is behind. Every
  competitor fact (stars, license, feature claims) was checked live via `gh api` or a
  web fetch on 2026-09-06, not copied from this board's own 2026-08-30 research
  without re-checking — two corrections came out of that re-check (Sculptor is MIT and
  open-source, not closed-source as this Part's cold-start capsule states; Emdash ships
  a lightweight kanban-style tracker, so "not a board at all" overstates it slightly —
  both fixed in the table, both flagged in this card's handoff).
- `docs/launch/posts/hn.md`, `reddit-selfhosted.md`, `reddit-rust.md`, `lobsters.md` —
  one draft per venue, each written in that venue's actual format and tone (HN takes no
  markdown at all; Lobsters is title+URL+a short first comment; Reddit and the rest take
  full markdown). Every number in every draft is one measured in this session.
- `docs/launch/good-first-issues/01`–`07` — seven drafts (card asked for five to ten):
  i18n scaffolding, SMTP notifications, time tracking, in-UI artifact diff review (the
  four named on the board), plus three more grounded the same way (a new project-type
  preset, unifying the CLI's 89 duplicated `--json` flags into one `--format` flag, a
  new custom-field type) — each cites exact files and line numbers and was checked to
  not require reading `TODO.md`.
- `.github/ISSUE_TEMPLATE/bug_report.yml` — added a "searched existing issues" checkbox.
- `.github/ISSUE_TEMPLATE/feature_request.yml` — added an optional "would you like to
  work on this yourself" dropdown, pointing at `CONTRIBUTING.md`; a small, direct way to
  convert incoming requests into contributor candidates for a one-contributor project.
- `.github/PULL_REQUEST_TEMPLATE.md` — added a "Related issue" field, split the
  checklist to match the exact commands `CONTRIBUTING.md` prescribes (fmt, clippy,
  nextest, frontend type-check/test/gen:api, docs/changelog), and added the
  no-AI-attribution-trailer rule, which a first-time contributor has no way to know
  about otherwise short of reading all of `CONTRIBUTING.md`.
- `docs/launch/discussions-seed.md` — two draft Discussions topics, in the repository's
  real categories (`Q&A`, `Ideas` — checked live via the GraphQL API, not guessed;
  Discussions is already enabled on the repo, also checked rather than assumed).
- `install.sh` — one bug fixed (item 3 above), outside this card's ownership,
  flagged prominently rather than silently expanded scope.

## Publish list — everything a human has to actually do

Nothing here is posted, tagged or labeled by any card. Items 1 to 3 of the pre-flight
above are done; what is left, in order:

1. **Tag the release.** Nothing since `v0.1.0-beta.7` is downloadable, and that tag
   predates the desktop bundles entirely — the releases page carries no `.AppImage`,
   `.deb`, `.dmg` or `.msi` today, so every download link in the README and the quick
   start resolves to a page without the app on it. Bump the workspace version, then
   `git tag vX.Y.Z && git push origin vX.Y.Z`; `release.yml` builds and publishes the
   archives, the four desktop bundles and the SBOMs, and refuses the tag outright if it
   does not match `Cargo.toml`.
2. **Update the GitHub repository description** to drop the removed OpenCode mention —
   resolves pre-flight item 4. A one-line `gh repo edit --description "..."`, or the
   repository settings UI.
3. **Open the seven `good first issue` GitHub issues** from the drafts in
   `docs/launch/good-first-issues/`, applying the existing `good first issue` label
   (already exists on the repo — confirmed via `gh label list`, not created by this
   card).
4. **Post the four drafts** in `docs/launch/posts/` to their respective venues, in
   whatever order/timing is preferred — nothing about them is time-sensitive relative
   to each other, but all four assume the items above are already done (every draft
   links to the repo and the recovery-demo recording).
5. **Post the two Discussions topics** in `docs/launch/discussions-seed.md`, into the
   `Q&A` and `Ideas` categories respectively (both already exist on the repo — no
   category needs creating).
