# Frontend npm-major dependency bumps — handoff

Lands Dependabot PR #42 (`dependabot/npm_and_yarn/frontend/npm-major-a40c5d7ac7`)
directly on top of `develop`, since Dependabot's own PR can't fix the code its bump
breaks. Base: `develop` at `cf6cbd9`. Branch: `agent/deps-npm-major`.

## Per-package table

| Package | Old | New | Breaking change | What was adapted |
|---|---|---|---|---|
| `@solidjs/router` | 0.16.3 | 1.0.0 | None. A version realignment: the package's own changelog states "the code is functionally identical to the 0.16 line". Confirmed by diffing the two npm tarballs directly (`npm pack @solidjs/router@0.16.3 @solidjs/router@1.0.0`) — the only files that differ are `dist/index.js` (a version-string bump) and `dist/routers/scrollRestoration.{js,d.ts}` (scroll restoration now settles with one scroll instead of retrying via a `ResizeObserver`; every public `.d.ts` file is byte-identical). | Nothing. This app's `Router` (`frontend/src/app/App.tsx:17`) does not use scroll restoration, and no file under `frontend/src` or `frontend/e2e` imports the scroll-restoration API (`grep -rn "scrollRestoration" src e2e` — no hits). All 60 files importing `@solidjs/router` (`useSearchParams`, `useNavigate`, `Route`, `A`, `MemoryRouter`, etc.) needed zero code changes; `npm run type-check` and `npx vitest run` are green unmodified. |
| `@types/node` | 24.13.3 | 26.4.1 | Type-only; no runtime effect. Pulled `undici-types` 7.18.2 → 8.3.0 as its sole transitive dependent. | Nothing — `npm run type-check` (`tsc -b`) is clean with no new errors. CI's actual Node runtime stays pinned at "20" (`.github/workflows/ci.yml`, four `setup-node` steps); this bump only changes what `tsc` type-checks against, not what runs. |
| `jsdom` | 29.1.1 | 29.1.1 (**not bumped**) | N/A — held back. | See "The jsdom decision" below. |
| `typescript` | ~5.9.3 | ~5.9.3 (**not bumped**) | N/A — held back. | See "The TypeScript decision" below. |

## The TypeScript decision

Checked before touching anything else, per the task's own instruction. `typescript`
5.9.3 is already the newest published 5.x release (`npm view typescript versions`
has no 5.x above 5.9.3), so "keep on 5.x" and "keep the current pin" are the same
thing here — no code change needed.

Peer-range evidence: every `openapi-typescript` release accepts only
`typescript@^5.x`, including the `next` dist-tag:

```
$ npm view openapi-typescript peerDependencies --json
{ "typescript": "^5.x" }
$ npm view openapi-typescript@next version peerDependencies --json
{ "version": "7.0.0-rc.1", "peerDependencies": { "typescript": "^5.x" } }
```

Forcing `typescript` to `~7.0.2` reproduces the PR's own `npm ci` ERESOLVE (a peer
conflict against `openapi-typescript`, which generates
`frontend/src/shared/api/schema.gen.ts` — a generated, pre-push-checked file this
repo cannot do without). No released or `next`-tagged `openapi-typescript` widens
that range. Per the task's own decision rule, `typescript` is left on `~5.9.3` and
an `ignore` was added to `.github/dependabot.yml`'s npm entry for the `typescript`
major, with the peer-range reason inline, removable once a released
`openapi-typescript` accepts TS 7.

## The jsdom decision (a second, unplanned hold-back)

The task briefed `jsdom` 30 as a routine vitest-environment bump (vitest's own
peer is `"jsdom": "*"` — no constraint). That peer check is real but insufficient:
`jsdom` 30.0.1 raises its own `engines.node` floor to
`^22.22.2 || ^24.15.0 || >=26.0.0`, and CI's `setup-node` pins `node-version: "20"`
across all four frontend-touching jobs in `.github/workflows/ci.yml`
(`coverage`, `frontend`, `security`, `e2e`).

This was proven to be a hard runtime failure, not just an `EBADENGINE` warning.
Using `nvm install 20` to get the exact CI runtime (resolved to 20.20.2) and
running the real suite against it:

```
$ nvm use 20 && node -v
v20.20.2
$ npm ci            # jsdom 30.0.1 installed, EBADENGINE warnings only
$ npx vitest run
 Test Files  no tests
      Tests  no tests
     Errors  91 errors
Caused by: TypeError: webidl.util.markAsUncloneable is not a function
 ❯ new CacheStorage node_modules/undici/lib/web/cache/cachestorage.js:20:17
 ❯ Object.<anonymous> node_modules/jsdom/lib/api.js:12:33
```

Every vitest worker fails to even load `jsdom` under Node 20 — the version
bundled inside `jsdom` 30's own dependency tree (`undici`) calls a `webidl`
helper Node 20's internals don't provide. `frontend/package-lock.json` confirms
the coverage-only `npx vitest run --coverage` job (the only CI job that actually
exercises jsdom) would fail outright at 100% of test files.

Confirmed the alternative: `jsdom` 29.1.1 (the currently pinned version) is
already the latest 29.x release, and its own `engines.node` is
`^20.19.0 || ^22.13.0 || >=24.0.0` — compatible with whatever current Node 20.x
`setup-node` resolves. Re-ran the full suite under the same Node 20.20.2 with
`jsdom` reverted to 29.1.1: `Test Files 91 passed (91)`, `Tests 851 passed (851)`.

`jsdom` is therefore left at `^29.1.1` (unchanged) and a second `ignore` entry
was added to `.github/dependabot.yml` alongside the `typescript` one, with the
Node-floor reason inline. Removable once `.github/workflows/ci.yml`'s
`node-version` pin moves to 22+ — a decision this task does not make, since it
reaches every frontend CI job, not just this dependency.

## Lockfile diff summary

Net change vs. `develop`'s `frontend/package-lock.json`, computed by diffing the
`packages` map of both lockfiles (`node_modules/<pkg>` → resolved version):

```
CHANGED 3
 ~ node_modules/@solidjs/router 0.16.3 -> 1.0.0
 ~ node_modules/@types/node 24.13.3 -> 26.4.1
 ~ node_modules/undici-types 7.18.2 -> 8.3.0
ADDED 0
REMOVED 0
```

No unrelated churn: `undici-types` is `@types/node`'s own sole transitive
dependent, moved to stay in step with it. `jsdom` and `typescript` (and
everything under them) are untouched since neither was bumped.

## Router migration surface

The task briefed the `@solidjs/router` 1.0 bump as "the real work" — every call
site, every router-mocking test, every E2E spec navigating by `?item=` search
params. Given the near-zero diff between 0.16.3 and 1.0.0 (see table above), this
turned out to be a verification pass, not a migration:

- `grep -rln "@solidjs/router" frontend/src frontend/e2e` — 61 files import it
  (60 in `src`, 0 in `e2e`; the E2E specs drive the running app by URL, not by
  importing the router package). None needed a code change.
- `frontend/src/app/App.tsx:17` — the app's only `<Router>` instance, no
  scroll-restoration option passed.
- E2E specs using `?item=`/URL search params
  (`e2e/agent-assets.spec.ts`, `e2e/scheduler-e2e.spec.ts`,
  `e2e/execution-attempt-detail.spec.ts`, `e2e/a11y.spec.ts`,
  `e2e/run-with-agent.spec.ts`) don't import `@solidjs/router` at all — they
  assert against the browser's own `page.url()` / `page.goto()`, which the
  router's internal-only scroll-restoration change cannot affect.
- 15 `*.test.tsx` files mock the router via `MemoryRouter`/`Route` — all pass
  unmodified (`npx vitest run`: 851/851).

## Local verification (all green, unmodified code)

```
$ npm run type-check          # tsc -b — clean
$ npx vitest run               # Test Files 91 passed (91), Tests 851 passed (851)
$ npm run build                 # tsc -b && vite build — succeeds
$ npm run lint:tokens            # raw color literals: 0, inline-style hex: 0
$ npm run gen:api                 # schema.gen.ts diff: none (openapi-typescript
                                    stayed at 7.13.0, already installed pre-bump)
```

Entry bundle gzip gate (CI's own script, run manually since it's inline in
`ci.yml` rather than an npm script): 20,445 bytes vs. the 30,720-byte (30 KB)
limit. No `dist/assets/routing-*.js` chunk exists on `develop` either (no
`manualChunks` config names one) — pre-existing, unrelated to this change.

`.githooks/pre-push` — exit 0, run twice (once mid-work, once on the final
committed state): comment gate, test-hygiene gate, `cargo fmt --all --check`
(workspace and `tack-desktop` separately), `cargo clippy --workspace
--all-targets -- -D warnings`, lockfile-freshness, `schema.gen.ts`-freshness —
all pass.

## CI run and failure classification

Reference baseline — `develop` at this branch's own base commit (`cf6cbd9`),
run `34069404035` (a `push` to `develop`, not this branch):

| Job | Conclusion |
|---|---|
| cargo-deny | success |
| Rust (fmt + clippy + test) | success |
| Desktop app | success |
| MSRV | success |
| Docs | success |
| Frontend (typecheck + build) | success |
| Security (dependency audit) | success |
| E2E (Playwright, cross-browser) | **failure** — 30 failed, 2 flaky, 104 passed, across firefox/webkit only |
| Coverage | skipped (job's own `if` only runs on PR/main, not a `develop` push) |
| Embed SPA | skipped (`needs: frontend`; skipped alongside Coverage) |

The E2E failure is pre-existing on `develop`'s own tip, unrelated to this
branch's dependency bumps (this run predates the branch existing).

This branch's own run: **[fill in after `gh run watch` completes — see below]**

<!-- Run id, per-job conclusions, and classification (bump-caused / pre-existing
     on develop / unrelated) go here once the CI run finishes. -->

## Commits

One per package, `.github/dependabot.yml` ignore entries as their own commit
(covers both the typescript and jsdom decisions together, since they're the same
kind of finding), `CHANGELOG.md` as its own commit, this handoff last. No
`Co-Authored-By` or AI-attribution trailer on any of them, per this repo's
convention.

## What a user notices

Nothing. Both landed bumps are dev-time (`@types/node`, type declarations only)
or functionally identical to the version already running (`@solidjs/router`).
