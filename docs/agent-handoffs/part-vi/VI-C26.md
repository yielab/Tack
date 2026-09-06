# VI-C26 handoff

- Base SHA / branch / final SHA: worktree dispatched at `9f1096b` (`docs(board): record
  what VI-C24 actually found, and what it never weighed`) on branch
  `agent/vi-c26-palette-coverage`, which is `develop`'s own tip — no rebase or merge
  needed. Final SHA is HEAD of this branch after the single commit that includes this
  file.
- Files changed: `frontend/e2e/a11y.spec.ts` (new palette × mode coverage) and
  `frontend/e2e/helpers.ts` (one new helper, `setPaletteAndTheme`). `frontend/src/index.css`
  is **not** touched — see "The graphite fix: not made, and why" below. No other file.
- Contract fixtures consumed: none.

## The probe (task 1) — what each cell actually holds

`a11y.spec.ts` scanned exactly one of the six mode × palette cells before this card:
teal/light (Playwright's default color scheme, no stored palette). The other five were
scanned directly, on the board view (`/projects/:id/board`), using a temporary probe spec
(`e2e/probe-palette.spec.ts`, written and deleted this session — never part of the diff)
that pre-seeds `localStorage['tack_palette']`/`['tack_theme']` via `addInitScript` before
navigation:

```
nice -n 19 npx playwright test --project=chromium --workers=2 probe-palette.spec.ts
```

Result, one axe scan (`wcag2a`/`wcag2aa`/`wcag21a`/`wcag21aa`) per cell:

| Cell | Result |
|---|---|
| teal/dark | clean |
| clay/light | clean |
| clay/dark | clean |
| **graphite/light** | **1 violation** (`color-contrast`, 3 nodes, all the same pairing) |
| graphite/dark | clean |

The graphite/light violation's exact axe output (from the probe run, later reproduced
identically by the real, committed test — see "Fail-then-pass proof"):

```
color-contrast: fgColor #84cc16, bgColor #ffffff, contrastRatio 1.97, expected 4.5:1
  target: .active.gap-1\.5[aria-current="page"]        (WorkTabs.tsx active lens tab)
  target: .font-semibold.shrink-0                        (Breadcrumb.tsx current section)
  target: .gap-1.inline-flex.font-medium (nth-child(3))   (EmptyProjectGuide.tsx step link)
```

All three are components that set `color: var(--color-primary-600)` directly as body
text on a light surface (`--color-bg-base`/`--color-bg-elevated`, both `#ffffff` in
graphite/light) — the same pattern `Sprints.tsx`'s estimate span, `RunWithAgentModal.tsx`'s
two links, and `List.tsx`'s expand/collapse hover state also use (`grep -n
"color-primary-600" frontend/src -r` finds ~19 call sites; not all render on the board
view, so not all are in this specific 3-node list, but they share the identical token and
the identical bug).

Clay and every dark variant are genuinely clean — this isn't "unexamined," it's measured
and negative.

## Coverage shape chosen (task 2) — and what it deliberately does NOT cover

**Chosen:** one representative page (the board view) scanned under each of the five
previously-unscanned cells, added to the existing per-page scans (which stay teal/light
only, unchanged). Board view was picked because `WorkLayout.tsx` wraps every project route
in it, so its persistent chrome — `Sidebar.tsx` (including the "T" brand-mark badge),
`Breadcrumb.tsx`, `WorkTabs.tsx` — exercises most of the shared token surface (background,
text, border, primary, semantic ramps) that a page-specific scan would otherwise re-exercise
per page. This is also literally where the graphite defect showed up, unprompted, on the
very first cell probed.

**Cost of that choice, measured** (see "Suite wall-clock" below): +5 tests for
+~1–2s of wall-clock against a ~33–35s baseline — the "one representative page, not every
page in every palette" bet the card asked me to make paid off; scanning all ~30 existing
page-states in the other five cells would have been 5× this file's current cost, not a
rounding error.

**What this leaves deliberately unscanned** (a stranger should not assume otherwise):

1. **Every feature-specific surface, in every non-default palette/mode.** The economics
   dashboard's warning-band progress bar, the fleet health chips, the approvals inbox, the
   provisioning wizard, every modal — all of it is scanned only under teal/light. A
   palette-specific bug confined to one of those surfaces (not reachable from the board
   view's own chrome) has no test that would catch it. `Avatar.tsx`'s per-name `hsl()`
   chip (the exact class of bug VI-C24's neighbor card, "A12," found — a hue-dependent
   contrast failure invisible until the right name was fixtured) is the clearest example of
   the shape of bug this leaves open: nothing here re-checks its whole hue range under
   clay or graphite, light or dark.
2. **graphite/light beyond the one known, tracked rule.** The new graphite/light test
   disables `color-contrast` for that one scan only (see next section) — any *other* class
   of violation (missing labels, ARIA misuse, non-focusable controls) on the board view
   under graphite/light is still caught; a second, different color-contrast bug on that
   same page/palette would not be, until the tracked one is resolved and the disable is
   removed.
3. **Latent token pairings nothing currently renders.** `clay`'s own `--color-primary-600`
   (`#c2410c`) measures `4.38:1` against `--color-bg-sidebar` (`#f3ebe1`) — under AA — but
   no component today pairs `primary-600`-as-text with `bg-sidebar` in clay, so no scan
   (mine or the probe) surfaces it; it would only fire if some future component made that
   pairing. See "Measured numbers" for the calculation. This is exactly the kind of gap a
   token-level audit (checking every token pair a design system *permits*, not just the
   ones a current page *uses*) would catch and a page-scan approach structurally cannot.

## The graphite fix (task 3): not made — stop-and-report, per the card's own escape hatch

The card's ownership line scoped this to `--color-primary-600`'s *value* in `index.css`.
I found that a value-only edit cannot satisfy AA in both roles that token plays
simultaneously, and the card explicitly names this situation as a reason to stop rather
than decide:

**The two roles, and why one token can't serve both once you require AA in each:**

1. **Foreground text on a light surface** (the actual violation: WorkTabs, Breadcrumb,
   EmptyProjectGuide, and the ~15 other call sites like them) — needs `primary-600` **dark**
   enough that `contrast(primary-600, white) >= 4.5:1`.
2. **Background under `--color-on-accent` text** (`Button.tsx`'s `primary` variant,
   `Sidebar.tsx`'s "T" brand-mark badge, `Layout.tsx`, `Timeline.tsx`, `ItemHeader.tsx`) —
   graphite's `--color-on-accent` is **dark** (`#1a2e05` — `Button.tsx`'s own comment:
   "palette-aware so bright accents like the Graphite lime get dark text instead of
   unreadable white"), so this role needs `primary-600` **light** enough that
   `contrast(#1a2e05, primary-600) >= 4.5:1`.

Darkening `primary-600` to satisfy (1) directly un-satisfies (2), because on-accent is
already very dark — two dark colors don't contrast with each other regardless of exactly
how dark the darker one is. Proven two ways:

- **Arithmetic** (relative luminance, `L`, from the standard WCAG formula; `on-accent`
  `#1a2e05` has `L = 0.02185`): satisfying (1) requires `L(primary-600) <= 0.1833`;
  satisfying (2) with the *current* on-accent requires `L(primary-600) >= 0.2733`. These
  ranges don't overlap — no value of `primary-600` alone satisfies both.
- **Empirical**, by actually trying it: edited `index.css` line 313 to
  `--color-primary-600: #4d7c0f` (a shade matching graphite's own already-dark
  `--color-accent-ink`, chosen because it's the *least* aggressive darkening that clears
  role (1) — `4.99:1` on `--color-bg-base`) and reran the probe. Role (1)'s violation
  disappeared, but a **new** one appeared in its place:

  ```
  color-contrast: fgColor #1a2e05, bgColor #4d7c0f, contrastRatio 2.92, expected 4.5:1
    target: button[title="New item"]   (Button.tsx primary variant)
    target: .gap-2                      (Sidebar.tsx "T" brand-mark badge)
  ```

  (2.92 measured vs. 2.93 computed analytically — the small difference is axe's own
  rounding, not a different mechanism.) The edit was reverted immediately after this
  measurement; `git diff --stat` and `git status --short` below confirm `index.css` carries
  no diff in the committed change.

The only value shape that clears **both** roles is `--color-on-accent` becoming light
(matching how teal and clay already pair a dark `primary-600` with a **white**
`on-accent`) — which is not "correcting a value," it's swapping graphite's
bright-lime-with-dark-text pattern for a dark-olive-with-white-text one, i.e. the same
pattern the other two palettes already use. That would make graphite read distinctly less
like its current, stated identity (`Button.tsx`'s comment is explicit that the dark-text
choice was deliberate, made *for* this bright value) and touches a file (`Button.tsx`,
plus `Sidebar.tsx`'s hardcoded `#84cc16` swatch preview, which would then show a color the
app no longer actually renders) outside this card's named ownership. Per the card's own
instruction, I'm stopping here rather than making that call.

**What I did instead, to keep the suite green and the gap tracked rather than hidden:**
the new graphite/light test disables only the `color-contrast` rule, scoped to that one
test (`GRAPHITE_LIGHT_KNOWN_ISSUES`, passed as an extra argument to `scan()` — every other
new test, and every pre-existing one, still runs the full, unsuppressed rule set). This
mirrors the file's own pre-existing `KNOWN_ISSUES` convention ("triage existing debt
without blocking... remove it once the underlying issue is fixed") at the narrowest scope
that convention supports, rather than adding `color-contrast` to the file-wide
`KNOWN_ISSUES` list, which would have blinded every other scan in the suite to future
color-contrast regressions everywhere, not just this one known case.

**Recommendation left for whoever makes this call** (not decided here):
1. Accept the identity shift: darken `primary-600` (e.g. toward `~#3f6a0c`–`#4d7c0f`),
   flip `on-accent` to white, update `Sidebar.tsx`'s hardcoded swatch preview to match.
   Fixes every role everywhere; graphite becomes visually closer to teal/clay's pattern.
2. Leave `primary-600` exactly as-is (preserve the bright-lime identity for its background
   role), and instead redirect the ~19 *text*-role call sites to the token that was already
   built for this — `--color-primary-700`/`--color-accent-ink` (`#4d7c0f`, already
   `4.99:1` on `--color-bg-base`) — which is a `.tsx` change across several component
   files, not a token-value change, so it wasn't made under this card's CSS-only
   ownership either.
3. Leave it exactly as tracked here: one narrowly-scoped, documented exception, until (1)
   or (2) is chosen.

## Fail-then-pass proof (in that order)

Both runs below are the **real, committed** test (`e2e/a11y.spec.ts`), not the deleted
probe — the graphite/light test literally toggled between the two states by commenting out
the extra-disable argument, to prove the assertion itself (not just the probe) can fail.

**Fail** — line 126 temporarily read `await scan(page)` (no suppression):
```
nice -n 19 npx playwright test --project=chromium --workers=2 a11y.spec.ts -g "board view \("
```
```
✓ board view (clay/light) ...
✓ board view (teal/dark) ...
✓ board view (graphite/dark) ...
✓ board view (clay/dark) ...
✘ board view (graphite/light) has no accessibility violations other than the tracked, known one
    Error: insufficient color contrast of 1.97 (foreground color: #84cc16, background
    color: #ffffff, ...). Expected contrast ratio of 4.5:1
  1 failed, 4 passed
```

**Pass** — line 126 restored to `await scan(page, GRAPHITE_LIGHT_KNOWN_ISSUES)`:
```
nice -n 19 npx playwright test --project=chromium --workers=2 a11y.spec.ts -g "board view \("
```
```
✓ board view (clay/light) ...
✓ board view (teal/dark) ...
✓ board view (graphite/dark) ...
✓ board view (clay/dark) ...
✓ board view (graphite/light) has no accessibility violations other than the tracked, known one
  5 passed (4.1s)
```

This proves what the coverage claims to prove: the new graphite/light scan genuinely
detects the known violation (fail, first) rather than passing vacuously, and only reports
clean once that one specific, tracked rule is set aside (pass, second) — not because the
underlying color was fixed. **This is not the "fail before the colour fix, pass after"
proof the acceptance criteria describe for an actual repair; no repair was made, per the
stop-and-report section above.** The other four cells (clay/light, clay/dark, teal/dark,
graphite/dark) never needed suppression at any point — they were clean on the very first
probe run and stayed clean here.

## Measured numbers, every pair, with the command that produced it

Contrast tool used throughout (WCAG relative-luminance formula, standalone, no dependency
on axe): `/tmp/claude-1000/.../scratchpad/contrast.js` (session-local, not committed) —
`node contrast.js "<label>" "<fg-hex>" "<bg-hex>" ...`. Cross-checked against axe-core's
own `contrastRatio` for the two pairings axe itself measured live (graphite/light's
violation, and the empirical darkened-value experiment) — both matched to two decimal
places.

| Pairing | Ratio | Command (label as passed) |
|---|---|---|
| graphite-light `primary-600` (`#84cc16`) on `bg-base` (`#ffffff`) | **1.98:1** (axe measured 1.97 live, same pairing) | `node contrast.js "graphite-light primary-600 on bg-base" "#84cc16" "#ffffff"` |
| graphite-light `primary-600` on `bg-sidebar` (`#eff1f3`) | 1.74:1 | `... "graphite-light primary-600 on bg-sidebar" "#84cc16" "#eff1f3"` |
| graphite-light `primary-600` on `bg-subtle` (`#eef0f2`) | 1.73:1 | `... "graphite-light primary-600 on bg-subtle" "#84cc16" "#eef0f2"` |
| graphite-light `primary-600` on `bg-app` (`#f6f7f8`) | 1.84:1 | `... "graphite-light primary-600 on bg-app" "#84cc16" "#f6f7f8"` |
| graphite-light `accent-ink` (`#4d7c0f`, the already-safe, unused-as-text token) on `bg-base` | 4.99:1 | `... "graphite-light accent-ink on bg-base" "#4d7c0f" "#ffffff"` |
| graphite-dark `primary-600` (`#a3e635`) on `bg-base` (`#14171b`) | 11.92:1 | `... "graphite-dark primary-600 on bg-base" "#a3e635" "#14171b"` |
| clay-light `primary-600` (`#c2410c`) on `bg-base` (`#fffdfb`) | 5.10:1 | `... "clay-light primary-600 on bg-base" "#c2410c" "#fffdfb"` |
| clay-light `primary-600` on `bg-sidebar` (`#f3ebe1`) | **4.38:1 — under AA, but unreached by any current component** (see "What this leaves uncovered," item 3) | `... "clay-light primary-600 on bg-sidebar" "#c2410c" "#f3ebe1"` |
| clay-dark `primary-600` (`#fb923c`) on `bg-base` (`#1d160f`) | 7.90:1 | `... "clay-dark primary-600 on bg-base" "#fb923c" "#1d160f"` |
| teal-dark `primary-600` (`#2dd4bf`) on `bg-base` (`#101e1a`) | 9.23:1 | `... "teal-dark primary-600 on bg-base" "#2dd4bf" "#101e1a"` |
| current: `on-accent` (`#1a2e05`) on `primary-600` (`#84cc16`) — the button/brand-mark background role, as shipped | 7.40:1 | `... "on-accent on primary-600" "#1a2e05" "#84cc16"` |
| experiment: darkened `primary-600` (`#4d7c0f`) as text on `bg-base` | 4.99:1 (fixes role 1) | `... "darkened primary-600 on bg-base" "#4d7c0f" "#ffffff"` |
| experiment: `on-accent` (`#1a2e05`, unchanged) on darkened `primary-600` (`#4d7c0f`) | **2.93:1 (breaks role 2)** — axe measured 2.92 on the live page | `... "on-accent on darkened primary-600" "#1a2e05" "#4d7c0f"` |

## Suite wall-clock

Baseline (before any change in this card, same base commit, same machine):
```
nice -n 19 npx playwright test --project=chromium --workers=2
71 passed (33.3s)
```
(a second baseline run: `71 passed (33.9s)` — noise band established as ~33–34s)

After (5 new tests added):
```
nice -n 19 npx playwright test --project=chromium --workers=2
76 passed (34.2s)
```
(a second run after: `76 passed (35.3s)`)

**Cost: +5 tests for roughly +1–2s**, within the pre-existing run-to-run noise band — the
"one representative page per palette, not every page" bet holds up measured, not just
assumed.

## Gates

- `./scripts/check-comments.sh` → `✓ no board archaeology in crates/ frontend/src`
  (unaffected either way — the new comments live in `frontend/e2e/`, which this script
  does not scan, but they were still written to the same standard: no card/wave/phase
  citations, explain the code and the finding, not the board).
- `./scripts/check-test-hygiene.sh` → `✓ tests take their temporary paths from a guard`.
- `cd frontend && npm run type-check` → clean (`tsc -b`, no output).
- `cd frontend && npx vitest run` → `Test Files 91 passed (91)` / `Tests 848 passed (848)`.
- `nice -n 19 .githooks/pre-push` (repo root, `CARGO_BUILD_JOBS=4`) → `✓ pre-push checks
  passed` (fmt, clippy `-D warnings`, lockfile/schema freshness all clean).

## Schema/API/contract change requested from another owner

None.

## Known limitations / `not_measured` fields

- See "Coverage shape chosen" above for the three deliberate gaps (non-board surfaces in
  non-default palettes, the one tracked graphite/light rule, and latent token pairings no
  live component reaches).
- The identity-vs-compliance decision for graphite's primary is `not_measured` in the
  sense that it isn't a number to measure — it's a design call, deliberately left open
  per "The graphite fix" above.

## Secrets/logging review

n/a — no log line, secret, or config value touched.

## Safe merge order and likely conflicts

`frontend/e2e/a11y.spec.ts` and `frontend/e2e/helpers.ts` are both append-only changes
(new tests, one new exported helper) — the only plausible conflict is another card also
appending to either file's end; no other named Part VI/VII card owns `a11y.spec.ts` or
`helpers.ts`.

## Checklist

No unowned files touched (`index.css` was edited only as a reverted experiment, never
committed — confirmed clean in the diff below). No live secret. No panic stub. No blind
retry.

## Claim → evidence

| Claim | Evidence |
|---|---|
| Only teal/light was ever scanned before this card | `a11y.spec.ts`'s pre-existing `scan()` never set a palette/theme; every existing test's `page.goto` runs under Playwright's default color scheme with no stored palette |
| clay/light, clay/dark, teal/dark, graphite/dark are genuinely clean, not just unexamined | Direct probe scan of each, `0` violations returned by axe, all four |
| graphite/light has a real, reproducible violation | Same probe: `color-contrast`, `1.97:1`, 3 nodes; reproduced identically by the committed test (`1.97` again, byte-for-byte) |
| A value-only fix to `--color-primary-600` cannot satisfy AA in both roles it plays | Arithmetic (non-overlapping luminance ranges) + empirical (live axe scan of an actual edit, showing the violation move from 3 nodes to 2 different nodes, `2.92:1`) |
| The new coverage actually detects the known violation, not just asserts around it | Fail-then-pass proof above: same test, same page, toggled between unsuppressed (fails, `1.97:1`) and scoped-suppressed (passes) |
| Adding 5 scans costs roughly nothing against this suite's existing runtime | Two baseline runs (`33.3s`/`33.9s`) vs. two after-runs (`34.2s`/`35.3s`) |
| The full chromium suite is still fully green | `76 passed` both after-runs; gates section above |

## Context spent

- Files read in full: `frontend/e2e/a11y.spec.ts` (all ~1190 lines, pre-edit), `frontend/
  src/shared/state/palette.ts`, `frontend/src/shared/state/theme.ts`, `frontend/src/
  index.css` (the token block, lines 1–420), `frontend/e2e/helpers.ts` (relevant section),
  `frontend/e2e/playwright.config.ts`.
- Files read in part, to trace where `--color-primary-600` is consumed as text vs.
  background: `frontend/src/shared/ui/WorkTabs.tsx`, `Breadcrumb.tsx`, `Sidebar.tsx`,
  `Button.tsx` (all in full); `EmptyProjectGuide.tsx`, `RunWithAgentModal.tsx`, `Sprints.tsx`,
  `List.tsx` (excerpts around each `primary-600` call site, via `grep -n` then targeted
  reads).
- Files opened and not used: none.
- Prior handoff consulted: `docs/agent-handoffs/part-vi/VI-C24.md`, which had already
  found this exact graphite defect (identical `1.97:1`, identical pairing) as an
  out-of-scope side note and flagged it for a later card — this one.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*
