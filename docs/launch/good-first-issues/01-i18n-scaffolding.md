# good first issue: i18n scaffolding for the frontend

**Suggested labels:** `good first issue`, `enhancement`, `frontend`

## The gap, checked directly

Tack's UI is English-only with zero locale infrastructure. This is not the same thing as
the existing "vocabulary" system (`crates/tack-core/src/vocabulary.rs`), which renames
*domain terms* per project type ("Sprint" → "Phase" for a construction project) — it
never translates a *language*. Checked directly: `grep -rn 'i18n\|locale' frontend/src`
turns up nothing but `.localeCompare()` calls used for deterministic string sorting
(e.g. `frontend/src/features/table/Table.tsx:49`), not translation. There is no
`i18next`/`@solid-primitives/i18n`-style library in `frontend/package.json`, no message
catalog, and every user-facing string in `frontend/src/features/*` (each UI area lives
in its own folder there — `board/`, `dashboard/`, `settings/`, etc.) is a literal
English string in JSX.

## Why this is worth doing

Tack's whole value proposition (self-hosted, no cloud, own your data) is attractive
outside English-speaking teams too, but right now there's no seam to hang a translation
on — someone who wants German or Japanese labels has to hand-edit every component.

## Where to start

1. Pick an i18n library that works with SolidJS (`@solid-primitives/i18n` is a
   reasonable, actively maintained option — check its current state before committing to
   it, don't assume).
2. Scaffold **one** locale pair (English as the source, plus one real target locale —
   pick any language you can verify translations for) to prove the pattern, not all
   locales at once. A partial, correct scaffold that only covers the Board view is more
   useful than an attempt to translate the whole app in one PR.
3. Extract strings from one view — `frontend/src/features/board/Board.tsx` is the
   most-used one — into message keys, wire up the provider, and add a locale switcher
   somewhere reachable (Settings is the natural place — see
   `frontend/src/features/settings/GlobalSettings.tsx`).
4. Decide where the active locale is stored (a user preference has nowhere to live yet
   — there's no per-user identity, see `docs/adr/0059-single-operator-identity-posture.md`
   — so this is realistically a browser-local setting, not a server-side one, until that
   changes).

## What "done" looks like for a first PR

A working locale switcher that changes at least one full page's strings, with a
regression test (`frontend/src/**/*.test.tsx`, Vitest — see `cd frontend && npm test`)
proving the switch actually changes rendered text. It does not need to cover every page
— a reviewer would rather merge "Board is translatable, everything else still falls
back to English" than wait for a PR that tries to do all of it at once.

## Before you start

Read `CONTRIBUTING.md`'s "Pull Request Process" section (branch off `develop`, run
`cd frontend && npm run type-check && npm test` before pushing). This issue does not
require reading this repository's internal planning board — everything needed to start
is above and in the linked files.
