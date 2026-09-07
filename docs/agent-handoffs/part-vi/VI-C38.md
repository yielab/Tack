# VI-C38 handoff

- Base SHA / branch / final SHA: worktree dispatched at `9b251bd` (`docs(board): cards
  VI-C38, VI-C39 and VI-C40 — the three decisions the user took today`) on branch
  `agent/vi-c38-graphite`, `develop`'s own tip — no rebase needed. Final SHA is HEAD of
  this branch after the single commit that includes this file.
- Files changed: `frontend/src/index.css` (graphite-light's `--color-primary-600` and
  `--color-on-accent`), `frontend/src/shared/ui/Sidebar.tsx` (the hardcoded Graphite swatch
  preview), `frontend/src/shared/ui/Button.tsx` (the `primary` variant comment),
  `frontend/e2e/a11y.spec.ts` (folded graphite/light into `OTHER_MODES_AND_PALETTES`,
  removed `GRAPHITE_LIGHT_KNOWN_ISSUES` and its dedicated test), and this handoff. No
  other file — matches the card's ownership list exactly.
- Contract fixtures consumed: none.
- Behavior implemented: graphite becomes dark olive (`#3f6a0c`) with white `on-accent`
  (`#ffffff`), the same "dark primary + white text" pattern teal and clay already use —
  option 1 of VI-C26's escalation, decided by the user 2026-09-07. Before: a bright lime
  primary (`#84cc16`) with dark `on-accent` (`#1a2e05`), which failed WCAG AA whenever
  `primary-600` was used as foreground text on a light surface (WorkTabs' active tab,
  Breadcrumb's current-section label, `EmptyProjectGuide`'s step link, and ~15 other call
  sites sharing the same token). After: `primary-600` is dark enough to serve as both
  foreground text on every light surface and as a background under white on-accent text,
  because both roles now push in the same direction (a dark color needed either way) —
  the two-role conflict VI-C26 proved unsolvable with the old bright value no longer
  exists once `on-accent` is white.
- Tests added and exact commands/results: no new test *files* — `graphite/light` moved
  from its own dedicated, partially-suppressed test into the shared
  `OTHER_MODES_AND_PALETTES` loop, so it now runs the exact same unsuppressed `scan(page)`
  every other cell in that loop runs. All six mode × palette cells are unsuppressed gates
  as of this card.
  - `cd frontend && npx playwright test e2e/a11y.spec.ts --project=chromium --workers=2`
    → `41 passed (18.7s)`.
  - `cd frontend && npx playwright test --project=chromium --workers=2` (full chromium
    suite) → `78 passed (36.3s)`.
  - `cd frontend && npm run type-check` → clean (`tsc -b`, no output).
  - `cd frontend && npx vitest run` → `Test Files 91 passed (91)` / `Tests 851 passed
    (851)`.
  - `nice -n 19 .githooks/pre-push` (repo root, `CARGO_TARGET_DIR` set to this worktree's
    isolated target dir, `CARGO_BUILD_JOBS=4`) → `✓ pre-push checks passed` (comment gate,
    test-hygiene gate, `cargo fmt --all --check` for the workspace and for
    `crates/tack-desktop` separately, `cargo clippy --workspace --all-targets -- -D
    warnings`, lockfile/schema freshness — all clean on the first run).
- Failure/adversarial case proved: fail-then-pass, in that order, on the real committed
  test (`board view (graphite/light) has no accessibility violations`, now inside the
  `OTHER_MODES_AND_PALETTES` loop, fully unsuppressed):
  - **Fail** — with the CSS change reverted (`git diff` of `index.css` stashed to a patch
    file, then `git checkout -- frontend/src/index.css` to restore the old
    `#84cc16`/`#1a2e05` pair), ran
    `npx playwright test e2e/a11y.spec.ts --project=chromium --workers=2 -g "board view
    \(graphite/light\)"`: `1 failed`, axe reporting `color-contrast`, `"fgColor": "#84cc16",
    "bgColor": "#ffffff"`, `"Expected contrast ratio of 4.5:1"`, message `"insufficient
    color contrast of 1.97"` — the identical violation VI-C26 first found, now caught by
    an unsuppressed assertion instead of one that disabled the rule.
  - **Pass** — reapplied the stashed patch (`git apply`) to restore the new
    `#3f6a0c`/`#ffffff` pair, reran the exact same command: `1 passed (2.7s)`.
  - This proves the coverage genuinely detects the old violation (not a vacuous pass) and
    genuinely clears it on the new tokens (not a suppressed pass) — the acceptance
    criterion's "fails on the old tokens and passes on the new ones, in that order" is a
    real repair proof, unlike VI-C26's proof of its suppression mechanism working.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: none new. VI-C26's other deliberately-left
  gaps (non-board surfaces scanned only under teal/light, latent token pairings no
  current component reaches — e.g. clay-light `primary-600` on `bg-sidebar` at `4.38:1`,
  still under AA and still unreached by any component) are unchanged by this card and
  remain open per that handoff.
- Secrets/logging review: n/a — no log line, secret, or config value touched.
- Safe merge order and likely conflicts: `frontend/e2e/a11y.spec.ts` is touched only in
  the palette-coverage section this card owns (per VI-C26 and the dispatch plan, no other
  named Part VI/VII card owns this file); a conflict is plausible only against another
  card also editing that same section, which none currently does. `index.css`,
  `Sidebar.tsx` and `Button.tsx` changes are each scoped to graphite's own block/line —
  low conflict risk with other palette or component work.
- Checklist: no unowned files touched; no live secret; no panic stub; no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| Graphite/light's `primary-600`-as-text violation (VI-C26's finding) is fixed, not suppressed | Fail-then-pass proof above; `GRAPHITE_LIGHT_KNOWN_ISSUES` and its dedicated test are deleted from `a11y.spec.ts` |
| Every pairing VI-C26's table measured still clears AA (or is unchanged and still fails exactly as VI-C26 recorded it, for the one pairing out of scope) | See "Measured numbers" below |
| Graphite/dark is unaffected | `.dark[data-palette="graphite"]` block (separate CSS rule, its own `on-accent`/`primary-600` values) untouched; `board view (graphite/dark)` passes in both the scoped and full a11y runs |
| No raw hex entered a component outside the pre-existing swatch-preview pattern | `Sidebar.tsx`'s `PaletteSwatch` calls were already hardcoded hex per palette (teal `#0d9488`, clay `#c2410c`) before this card; only the Graphite value was updated to match its new token, following the same pattern |
| All six mode × palette cells are now unsuppressed gates | `OTHER_MODES_AND_PALETTES` has 5 entries (`teal/dark`, `clay/light`, `clay/dark`, `graphite/dark`, `graphite/light`) plus the implicit `teal/light` every other test in the file already runs under = 6; none pass an `extraDisabled` argument to `scan()` |
| Full chromium suite is green | `78 passed (36.3s)` |

## Measured numbers

Tool: `/tmp/claude-1000/.../scratchpad/contrast.js` (session-local, recreated from
VI-C26's handoff description — standalone WCAG relative-luminance formula, no axe
dependency), `node contrast.js "<label>" "<fg-hex>" "<bg-hex>"`.

| Pairing | Ratio | Command |
|---|---|---|
| graphite-light `primary-600` (`#3f6a0c`, new) on `bg-base` (`#ffffff`) | **6.41:1** | `node contrast.js "graphite-light primary-600 on bg-base" "#3f6a0c" "#ffffff"` |
| graphite-light `primary-600` on `bg-sidebar` (`#eff1f3`) | **5.66:1** | `... "graphite-light primary-600 on bg-sidebar" "#3f6a0c" "#eff1f3"` |
| graphite-light `primary-600` on `bg-subtle` (`#eef0f2`) | **5.61:1** | `... "graphite-light primary-600 on bg-subtle" "#3f6a0c" "#eef0f2"` |
| graphite-light `primary-600` on `bg-app` (`#f6f7f8`) | **5.97:1** | `... "graphite-light primary-600 on bg-app" "#3f6a0c" "#f6f7f8"` |
| graphite-light `accent-ink` (`#4d7c0f`, unchanged, still unused as text) on `bg-base` | 4.99:1 (unchanged from VI-C26) | `... "graphite-light accent-ink on bg-base" "#4d7c0f" "#ffffff"` |
| graphite-dark `primary-600` (`#a3e635`, unchanged) on `bg-base` (`#14171b`) | 11.92:1 (unchanged) | `... "graphite-dark primary-600 on bg-base" "#a3e635" "#14171b"` |
| graphite-dark `on-accent` (`#15240a`, unchanged) on `primary-600` (`#a3e635`, unchanged) | 10.81:1 — clears AA with headroom, confirms dark mode needed no change | `... "on-accent on primary-600 (graphite-dark)" "#15240a" "#a3e635"` |
| clay-light `primary-600` (`#c2410c`) on `bg-base` (`#fffdfb`) | 5.10:1 (unchanged, out of scope) | `... "clay-light primary-600 on bg-base" "#c2410c" "#fffdfb"` |
| clay-light `primary-600` on `bg-sidebar` (`#f3ebe1`) | **4.38:1 — still under AA, still unreached by any current component (VI-C26's finding, unchanged, out of this card's ownership)** | `... "clay-light primary-600 on bg-sidebar" "#c2410c" "#f3ebe1"` |
| clay-dark `primary-600` (`#fb923c`) on `bg-base` (`#1d160f`) | 7.90:1 (unchanged, out of scope) | `... "clay-dark primary-600 on bg-base" "#fb923c" "#1d160f"` |
| teal-dark `primary-600` (`#2dd4bf`) on `bg-base` (`#101e1a`) | 9.23:1 (unchanged, out of scope) | `... "teal-dark primary-600 on bg-base" "#2dd4bf" "#101e1a"` |
| new: white `on-accent` (`#ffffff`) on graphite-light `primary-600` (`#3f6a0c`) — the button/brand-mark background role | **6.41:1** (was 7.40:1 with the old bright-lime/dark-text pair; still clears AA with margin) | `... "on-accent (white) on primary-600 (graphite-light, new)" "#ffffff" "#3f6a0c"` |

## What a stranger still cannot do

A stranger arriving from outside this repository still cannot assume every token pairing
the design system *permits* is contrast-safe — only pairings a current component actually
renders are scanned (clay-light `primary-600`-on-`bg-sidebar` at `4.38:1` is the standing
example: real, under AA, and invisible to every test in this file because nothing renders
it today). That gap is VI-C26's, not this card's, and remains open.

## Surface-map delta

None — this card touches design tokens and test coverage only, no console/UI surface
migration.

## Context spent

- Tokens read before the first edit (cold start): read the card (~45 lines), VI-C26's
  handoff whole (337 lines, ~7k tokens as the dispatch instructions estimated),
  `a11y.spec.ts` lines 1–140 plus a targeted grep, `palette.ts` whole (short), `index.css`
  graphite blocks (light, dark, and the `prefers-color-scheme` fallback), `Button.tsx`'s
  variant switch, `Sidebar.tsx`'s swatch/footer section. Roughly in line with the ~25k
  cold-start ceiling the dispatch plan sets for a card this size.
- Context size at handoff: well under the 120k mid-card ceiling; this was a small,
  single-file-scope card.
- Files opened and not used: none.
- Read-list lines that were wrong: none — CLAUDE.md's pointers and the card's own file
  list matched what was actually needed.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*
