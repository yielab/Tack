# VI-C24 handoff

- Base SHA / branch / final SHA: worktree dispatched at `0ee8414` (`docs(board): VI-C20 is
  in, and VI-C24 is two tests, not one`) on branch `agent/vi-c24-contrast`, which is
  `develop`'s own tip — no rebase or merge needed. Final SHA is HEAD of this branch after
  the single commit that includes this file.
- Files changed:
  - `frontend/src/index.css` — the `tk-overlay` and `tk-drawer` `@keyframes` no longer
    animate `opacity`. `tk-overlay` now animates `background-color` (`transparent` →
    `var(--color-bg-overlay)`); `tk-drawer` keeps only its `transform: translateX(...)`
    slide.
  - No `--color-text-secondary` value changed, in any of the three light palettes. See
    "Behavior implemented" for why.
- Contract fixtures consumed: none.
- **The opacity hypothesis was refuted, not confirmed.** The card's own working theory —
  that `#5f736e` is `--color-text-secondary` composited at a fixed ~94% opacity over its
  surface — described the right *symptom* (an opacity-driven composite) but the wrong
  *source*. There is no persistent, always-on opacity applied to secondary text anywhere
  in the item detail drawer. Direct measurement (below) shows the token renders at its
  clean, declared value once the page settles. What actually happens: `Drawer.tsx`'s own
  mount transition (`animation: 'tk-overlay .15s ease'` on the full-screen backdrop,
  `animation: 'tk-drawer .22s cubic-bezier(.2,.7,.3,1)'` on the panel — both defined in
  `index.css`) ramps CSS `opacity` from 0 to 1 on **both** the backdrop and the panel
  nested inside it. `opacity` composites an entire subtree against whatever is behind it —
  so every descendant, including the chip/tab/heading/select elements axe flagged, briefly
  renders diluted toward the page behind the drawer while that ramp is in flight, even
  though each of those elements' own declared color and background never change. Axe-core
  reads `getComputedStyle` at whatever instant Playwright's own `toBeVisible()` resolves —
  which, per Playwright's own visibility semantics, does not wait for `opacity` to reach 1
  — so a scan can land, and often does land, mid-ramp.
- Behavior implemented:
  1. **Fix in "the thing that reduces its opacity", not the token.** An opacity ramp from
     0 dilutes contrast toward 1:1 at the low end of the ramp regardless of how dark the
     foreground token is — there is no fixed darkening of `--color-text-secondary` that
     can guarantee 4.5:1 against an *unbounded* opacity value between 0 and 1. The token
     was never the defect (see "Measured numbers": every light-palette pairing already
     clears AA by 0.44–2.48:1 at its true, settled value), so darkening it further would
     have bought a slightly bigger buffer against the same race without closing it, at the
     cost of a token color slightly duller than the redesign's intent — a worse trade than
     fixing the actual mechanism.
  2. **`tk-overlay`: `opacity` → `background-color`.** The backdrop's own visual fade (from
     invisible to its translucent dark tint) only needs to repaint the backdrop element's
     *own* fill; it does not need to affect how the panel nested inside it renders. Ani-
     mating `background-color: transparent → var(--color-bg-overlay)` keeps that same
     visual fade-in for the scrim while leaving the panel's own (already-opaque) background
     and every descendant's color untouched from frame 0.
  3. **`tk-drawer`: dropped the `opacity: 0 → 1` step, kept `transform: translateX(24px) →
     translateX(0)`.** The slide alone still communicates "this panel is entering"; drop-
     ping the opacity half removes the only remaining place descendant text could still be
     diluted (the panel itself, nested one level inside the backdrop). With both keyframes
     opacity-free, no ancestor of any drawer text ever reports an `opacity` below 1, at any
     point after mount — the composited color axe reads is always the declared token.
  4. `tk-overlay` is shared with `CommandPalette.tsx`'s backdrop, which used the identical
     `animation: 'tk-overlay ...'` pattern; that surface gets the same fix as a side effect
     of the shared keyframe, though it was not separately audited (out of this card's named
     ownership — the item detail drawer).
- Tests added and exact commands/results: none added — the card explicitly forbids
  changing `a11y.spec.ts`'s assertions, and no new test was needed once the actual
  mechanism (not the token) was identified. All commands below ran with
  `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C24`, port 3399 confirmed free before
  each run (`ss -ltn | grep 3399`).
  - Acceptance #1, exact command:
    `nice -n 19 npx playwright test --project=chromium -g "item detail drawer" --workers=2`
    → `3 passed` (both target tests plus the third, unrelated "blocked dispatch outcome"
    scan the same `-g` pattern also catches). Repeated **8 times** in a row post-fix, from
    inside `frontend/`: 8/8 clean. Before the fix, the same command on the same code was
    flaky (see "Failure/adversarial case proved" — 3 of 5 solo runs of just the line-192
    test failed, and a grouped run failed either line 192 or line 442 nearly every time),
    which is exactly the "reported three times, carded zero times" history this card was
    opened to end: previous investigators most likely hit the pass side of the same race
    when they stashed their own changes and reproduced against a clean base.
  - Acceptance #2, full chromium suite: `nice -n 19 npx playwright test --project=chromium
    --workers=2` → `71 passed`, run twice in a row post-fix (`71 passed (32.1s)` /
    `71 passed (32.0s)`). No failure of any kind remains, `color-contrast` or otherwise.
  - Gates: `./scripts/check-comments.sh` → `✓ no board archaeology in crates/
    frontend/src`. `./scripts/check-test-hygiene.sh` → `✓ tests take their temporary paths
    from a guard`. `cd frontend && npm run type-check` → clean (`tsc -b`, no output).
    `cd frontend && npx vitest run` → `Test Files 91 passed (91)` / `Tests 848 passed
    (848)`. `nice -n 19 .githooks/pre-push` (from repo root, `CARGO_BUILD_JOBS=4`) →
    `✓ pre-push checks passed` (fmt, clippy `-D warnings`, lockfile/schema freshness all
    clean).
- Failure/adversarial case proved (doubles as the revert-once proof): `git stash` (removing
  the `index.css` edit, restoring the two `@keyframes` blocks to their pre-fix `opacity`
  ramps), then `nice -n 19 npx playwright test --project=chromium -g "item detail drawer"
  --workers=2` **5 times** in a row:
  - Run 1: line 442 failed, 2 passed.
  - Run 2: line 192 failed, 2 passed.
  - Run 3: line 442 failed, 2 passed.
  - Run 4: line 192 failed, 2 passed.
  - Run 5: **both** line 192 and line 442 failed, 1 passed.
  `git stash pop` restored the fix; the immediately following run was clean again (see
  "Tests added and exact commands/results"). The card's own two-tests-not-one framing is
  visible directly in run 5 — a slow-enough draw of the race fails both in the same
  invocation, which is exactly what the card's evidence (both lines failing with the same
  `#5f736e` foreground) described.
- Root-cause verification method: `page.evaluate` probes (not committed — written and
  deleted during this session, never part of the diff) confirmed the mechanism directly
  rather than by inference:
  - At the exact moment `await expect(page.getByRole('button', { name: 'Dispatch to
    agents' })).toBeVisible()` resolves (i.e., the point the real test scans from),
    `getComputedStyle(dialogPanel).opacity` read `"0.476049"` and the backdrop's read
    `"0.346775"`, with each element's own Web Animation `currentTime` at ~33ms into a
    220ms/150ms effect — genuinely mid-ramp, not settled.
  - The same probe re-run 2000ms after mount (animations long finished) read the chip's own
    `color` as `rgb(86, 107, 102)` (`#566b66`, the declared `--color-text-secondary` value,
    byte for byte) and `background-color` as `rgb(238, 242, 241)` (`#eef2f1`, the declared
    `--color-chip`/`--color-bg-subtle` value) — confirming the settled render is exactly
    the clean token pair, never a persistently-dimmed one.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields:
  - **Dark palettes: already compliant, not quietly relying on the same trick — but this
    is inferred from token margin, not directly measured, since `a11y.spec.ts` never scans
    in dark mode (`grep -n "dark\|theme" e2e/a11y.spec.ts` finds nothing that toggles it).**
    `Drawer.tsx`'s animations are theme-agnostic, so the same transient dilution mechanism
    applies equally in dark mode; it was simply never caught there because nothing tests
    that surface in dark mode. Computed dark-palette `--color-text-secondary` margins
    against every drawer-relevant surface (script and figures under "Measured numbers")
    are 6.08–7.67:1 — 1.1–1.6 points of headroom above the light palettes' own tightest
    margin (4.94:1), a byproduct of the dark block's own prior, unrelated fix (documented
    in `index.css`: lightened from `#67807a`/`#81705c`/`#6a7480` because each measured
    ~3.5:1 against `--color-bg-subtle`, a static-surface problem with nothing to do with
    opacity). That larger buffer makes it less exposed to the same race, but "less
    exposed" is not "proven immune" — an opacity value sampled early enough in the ramp
    (near 0) would still dilute any foreground toward 1:1 regardless of starting margin.
    Whether dark mode's buffer is large enough to survive the specific range of opacity
    values Playwright's own timing produces in practice is `not_measured`.
  - **A separate, pre-existing, out-of-scope defect in the graphite palette.** Probing the
    drawer with `data-palette="graphite"` (via `localStorage.setItem('tack_palette',
    'graphite')`, matching `frontend/src/shared/state/palette.ts`'s own storage key)
    surfaced a `color-contrast` violation with foreground `#84cc16` (graphite's
    `--color-primary-600`) on background `#ffffff`, ratio `1.97:1` — the sidebar's "Board"
    wordmark and the active nav link's brand-green text on the white sidebar. This is
    unrelated to `--color-text-secondary`, unrelated to opacity/animation (reproduced
    identically across 4/4 runs — a static failure, not a race), and outside this card's
    ownership (`--color-text-secondary` in the light palettes). It does not affect
    acceptance #2: no test in this repo's suite scans any page with a non-default palette
    active, so it cannot be "the full chromium suite's only remaining failures" — there are
    none, because nothing here exercises the path that would surface it. Flagging it here
    rather than fixing it, per this card's scope.
- Secrets/logging review: n/a — no log line, secret, or config value touched.
- Safe merge order and likely conflicts: `frontend/src/index.css`'s `@keyframes` block is a
  narrow, self-contained edit (13 lines); the only plausible conflict is another card also
  touching `tk-overlay`/`tk-drawer`, which no other named Part VI/VII card owns.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| `#5f736e`/`#e9edec`/`#eef1f0` are not real, static rendered values — they are `--color-text-secondary`/`--color-chip` mid an opacity ramp | `page.evaluate` probe: `opacity: "0.476049"` on the panel at the exact instant the real test scans, vs. `color: "rgb(86,107,102)"` (`#566b66`, the clean token) 2000ms after settle |
| The declared token already clears AA on every light-palette surface the drawer renders it against | `python3` WCAG-luminance script (below) — 12 pairings, lowest 4.94:1, all ≥ 4.5:1 |
| Removing `opacity` from both entrance animations eliminates the violation, not just moves it | 8/8 clean runs of `-g "item detail drawer"` post-fix, vs. 3/5 solo and repeated grouped failures pre-fix on identical test code |
| The fix is load-bearing, not incidental | revert-once proof: `git stash` → 5/5 runs each show at least one of the two named tests failing (one run shows both); `git stash pop` → clean again |
| Full chromium suite has no remaining failure of any kind | `71 passed` × 2 consecutive full runs |
| Dark palettes were not touched and are not proven exempt from the same race | `grep` shows no dark-mode scan exists in `a11y.spec.ts`; dark-palette margins computed but not exercised live |
| A separate graphite-only defect exists, unrelated to this card | `data-palette="graphite"` probe: `color-contrast`, fg `#84cc16` / bg `#ffffff`, `1.97:1`, reproduced 4/4 |

A row with no evidence is a claim to delete, not a row to leave blank.

## Measured numbers

- Declared-token contrast, every light palette against every surface the item detail
  drawer actually renders `--color-text-secondary` on (`--color-bg-subtle`/`--color-chip`,
  `--color-bg-app`, `--color-bg-sidebar`, `--color-bg-base`), computed with the standard
  WCAG relative-luminance formula:
  ```
  python3 - <<'EOF'
  def lin(c):
      c = c/255
      return c/12.92 if c <= 0.03928 else ((c+0.055)/1.055)**2.4
  def lum(hex_color):
      hex_color = hex_color.lstrip('#')
      r,g,b = (int(hex_color[i:i+2],16) for i in (0,2,4))
      return 0.2126*lin(r) + 0.7152*lin(g) + 0.0722*lin(b)
  def ratio(c1, c2):
      l1, l2 = lum(c1), lum(c2)
      l1, l2 = max(l1,l2), min(l1,l2)
      return (l1+0.05)/(l2+0.05)
  # (fg, bg) pairs per palette follow the same call: ratio(fg, bg)
  EOF
  ```
  - Teal (default): bg-subtle/chip `5.04:1`, bg-app `5.23:1`, bg-sidebar `4.94:1`,
    bg-base `5.69:1`.
  - Clay: bg-subtle `5.44:1`, bg-app `5.92:1`, bg-sidebar `5.40:1`, bg-base `6.28:1`.
  - Graphite: bg-subtle `6.11:1`, bg-app `6.50:1`, bg-sidebar `6.16:1`, bg-base `6.98:1`.
  - All 12 pairings clear 4.5:1; none of these values changed in this card — no light
    palette's token needed darkening.
- Dark palettes, same surfaces, same script, for context (not exercised by any test):
  teal-dark `6.08–7.67:1`, clay-dark `6.36–7.37:1`, graphite-dark `6.26–7.62:1` across the
  same four surfaces each.
- Mid-animation opacity, captured at the exact instant the real test scans (`page.evaluate`
  reading `getComputedStyle(...).opacity` plus `element.getAnimations()[0].currentTime`
  right after the same `toBeVisible()` await the real test uses): panel `0.476049` at
  `33.4ms` of a `220ms` effect; backdrop `0.346775` at the same `33.4ms` of its own `150ms`
  effect. A second capture at `t=2000ms` (long settled): both back to declared, undiluted
  values.
- Pre-fix flake rate: solo runs of the line-192 test only
  (`-g "item detail drawer has no accessibility violations"`), 5 consecutive invocations:
  2 passed / 3 failed. Grouped runs of both target tests together, 2 consecutive
  invocations: both showed exactly one of the two lines failing.
- Post-fix pass rate: grouped runs of both target tests, 8 consecutive invocations: 8/8
  clean. Full chromium suite, 2 consecutive invocations: 71/71 clean each time.
- Revert-once proof: 5 consecutive invocations against the stashed (pre-fix) code, all 5
  showed a failure (4 with exactly one of the two named tests, 1 with both).

## What a stranger still cannot do

A stranger who scans the item detail drawer in dark mode, or with the graphite palette
active, has no test in this suite backing them up — dark mode because nothing here scans
it at all, graphite because of the separate, pre-existing `--color-primary-600`-on-white
defect this card found but did not own. A stranger relying only on axe's `impact: serious`
label to judge urgency would also have been misled the same way three prior investigators
were: the failure was real (an actual sub-AA frame really is painted, briefly, on every
mount) but its *reported* frequency depended entirely on how much other work happened to
run between mount and scan on whatever machine ran it, which is why "stashed everything
and it still failed" and "stashed everything and it passed" were both true statements
about the same unmodified base, made by different people on different days.

## Surface-map delta

None — this card touches only two `@keyframes` blocks (an implementation detail of two
existing entrance transitions); no route, console command, or token consumed by other
components changed shape.

## Context spent

- Tokens read before the first edit: `frontend/src/index.css` read in full (all six
  mode/palette primitive blocks plus the shared alias section and the animation block);
  `frontend/e2e/a11y.spec.ts` read around both target tests (lines ~160–465) plus its
  header/`scan()` helper; `frontend/src/features/item-detail/ItemDetailDrawer.tsx`,
  `ItemHeader.tsx` and `frontend/src/shared/ui/TypeBadge.tsx` read to trace where
  `--color-text-secondary` is actually consumed in the drawer; `frontend/src/shared/ui/
  Drawer.tsx` read in full, which is where the actual mechanism was found;
  `frontend/src/shared/ui/CommandPalette.tsx` read around its own backdrop to confirm the
  shared-keyframe side effect; `frontend/src/shared/state/palette.ts` read in full to drive
  the graphite/clay verification probes.
- Files opened and not used: none — every file read fed directly into either the
  root-cause trace or a verification probe.
- Read-list lines that were wrong: the card's own working hypothesis (a fixed ~94%
  opacity applied to the token) was not confirmed by any file read — it was refuted by
  direct browser measurement instead. No specific file/line citation in the card was
  itself incorrect; the *mechanism* it inferred from the two reported ratios was.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*
