# VI-C40 handoff — CI moves to Node 22 LTS, and `jsdom` 30 follows

**Decision (user, 2026-09-07):** Node 22 LTS — the current maintenance line, not the newest major.

## What changed

- Every `node-version: "20"` became `"22"`: four jobs in `.github/workflows/ci.yml`
  (Coverage, Frontend, E2E, Embed SPA), three in `release.yml`, one in
  `scheduled-audit.yml`. `actions/setup-node` resolves `"22"` to the latest 22.x, which
  is `v22.23.2` today (`nvm ls-remote --lts=jod | tail -1`).
- `jsdom` `^29.1.1` → `^30.0.1` in `frontend/package.json` and the lockfile
  (`npm install --save-dev jsdom@^30.0.1`: 1 added, 2 removed, 5 changed).
- The `jsdom` semver-major ignore in `.github/dependabot.yml` is gone; its reason was the
  Node 20 pin, and the pin is gone.
- `README.md` and `CONTRIBUTING.md` say Node.js 22+ where they said 20+.

## Proof

Run from the card's worktree on Node 22.23.2 (`nvm use 22.23.2`):

| Check | Result |
|---|---|
| `npm run type-check` | clean |
| `npx vitest run` | 91 files, 851 tests passed, 8.06 s |

The same suite under the machine's own Node **22.17.1** — below `jsdom` 30's declared
floor of `^22.22.2` — also passes, 851/851 in 8.46 s. The `engines` field is a declared
floor, not an enforced one, and the runtime failure Dependabot's ignore documented
(`webidl.util.markAsUncloneable is not a function` inside undici) is specific to Node 20's
undici. A developer on 22.17.1 is therefore not blocked today, but the floor is honest and
`nvm install 22` is the one-line fix (this machine now also has 22.23.2 installed through
nvm; its default node was left untouched).

The three `Not implemented: navigation to another Document` lines vitest prints are not
new: the same three appear on `develop` with `jsdom` 29 (`npx vitest run 2>&1 | grep -c
"Not implemented: navigation"` → 3 on both).

Playwright was not run here: another card owned ports 3399/5199 at the time, and the E2E
suite does not load `jsdom`. CI's E2E job on the card branch is the proof for that part.

## Out of scope, noticed

- `typescript` 7 is still ignored in Dependabot because `openapi-typescript`'s peer range is
  `^5.x`; nothing in this card changes that.
- `frontend/package.json` declares no `engines` field of its own, so nothing warns a
  developer on Node 20 before vitest crashes. Worth adding when the next frontend floor
  moves.

## Proposed board row

**VI-C40 integrated 2026-09-07** — CI's eight `node-version` pins moved from 20 to 22 LTS
(resolving to 22.23.2 today), `jsdom` 30 landed and its Dependabot ignore is gone; 851/851
unit tests on 22.23.2, and also on the machine's 22.17.1 despite `jsdom`'s declared
`^22.22.2` floor — the Node 20 crash the ignore recorded was undici's, not a version gate.
