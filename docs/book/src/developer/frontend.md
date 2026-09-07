# Frontend & Design System

The web UI is a [SolidJS](https://www.solidjs.com/) single-page app in `frontend/`,
built with Vite and Tailwind v4. It talks to the API over `fetch` and a WebSocket,
and is embedded into the `tack` binary at release time via the `embed-spa` feature.

This page covers how the frontend is organized and, in particular, the **design
token system** every component relies on.

## Layout

```
frontend/src/
├── app/          App shell — Router, Layout (sidebar + top bar), routes
├── features/     One folder per surface: board, list, table, calendar,
│                 timeline, sprints, item-detail, dashboard, projects,
│                 settings, templates, agents, approvals, economics, fleet,
│                 provisioning
├── shared/
│   ├── ui/       The component kit (Button, Badge, Modal, Drawer, Tabs,
│   │             CommandPalette, SearchBar, Sidebar, ToastContainer …) plus
│   │             redesign primitives: Avatar/AvatarStack, TypeBadge,
│   │             PriorityDot, WipChip, KbdHint, and the icon set (icons.tsx)
│   ├── state/    Context stores + signals (project, items, theme, palette,
│   │             commandPalette, optimistic updates, toasts)
│   ├── api/      Typed fetch client (api.*), one module per resource
│   ├── realtime/ Reconnecting board WebSocket
│   ├── vocab/    Per-project terminology resolution (useVocab)
│   └── types/    DTOs mirroring the backend
└── index.css     The design tokens (see below)
```

### Module boundary

Features are isolated: **a `features/*` file may import from `shared/*`, never from
another feature.** This is enforced by `frontend/src/architecture.test.ts` — a
failing import shows up as a unit-test failure. Anything two features need goes in
`shared/`.

## Design tokens

All colour, surface, and shadow values live as CSS custom properties in
[`frontend/src/index.css`](https://github.com/yielab/tack/blob/develop/frontend/src/index.css).
Components consume **only** these `--color-*` tokens (via inline `style`), never raw
hex or Tailwind colour literals. That single indirection is what lets one attribute
flip restyle the entire app.

The system has **two axes**:

- **Mode** — a `.dark` class on `<html>` (managed by `shared/state/theme.ts`).
  `:root` holds the light values; `.dark` overrides only what differs.
- **Palette** — a `data-palette="clay|graphite"` attribute on `<html>` (managed by
  `shared/state/palette.ts`). No attribute = the default **Teal** palette.

So the cascade is `:root` → `.dark` → `:root[data-palette="…"]` →
`.dark[data-palette="…"]`. Each block redefines only the **primitive** values
(backgrounds, text tiers, the accent, semantic solids); everything derived (the
primary ramp, hover/active surfaces, inverse text, focus ring) is expressed once as
`var()` aliases that re-resolve against whichever palette is active. Adding a fourth
palette means adding one primitives block — nothing else changes.

A Tailwind `@theme inline` block re-exposes the runtime tokens under utility names
(`bg-surface`, `text-content`, `border-line`, `bg-brand-*`, …) for the places that
use classes instead of inline styles.

### Accessibility constraint

Token values are tuned to **WCAG 2.1 AA** (4.5:1 text contrast) and verified by an
axe scan in the E2E suite (`frontend/e2e/a11y.spec.ts`). When changing a colour,
keep white-on-accent and faint-text-on-surface above 4.5:1 — the CI a11y job will
fail otherwise.

### Typography

Two self-hosted fonts (via `@fontsource`, so they work offline):

- **Hanken Grotesk** — the UI sans (`--font-sans`).
- **JetBrains Mono** — ids, estimates, and keycaps (`--font-mono`).

## Adding a UI component

1. Build it in `shared/ui/` as a small Solid component. Style it with inline token
   styles — `style={{ 'background-color': 'var(--color-bg-base)' }}` — so it
   re-themes for free. Reuse existing primitives (`Avatar`, `TypeBadge`,
   `PriorityDot`, `WipChip`, `KbdHint`) rather than re-inlining markup.
2. Export it from `shared/ui/index.ts`.
3. If it has pure logic (a colour map, a formatter), put that in a co-located
   `*.ts` and unit-test it (`shared/ui/primitives.test.ts` is the pattern).
4. Consume it from a feature — never reach into another feature.

## Design source

The current visual language was imported from a Claude Design project
(`Tack.dc.html`) and implemented onto the token system above. The design is a
reference; the source of truth is `index.css` plus the `shared/ui` kit.

## Running it

```sh
cd frontend
npm install
npm run dev          # http://localhost:5173, proxies /api to :3210
npm run type-check
npm test             # Vitest unit tests
npm run build
```

Start the API (`cargo run -p tack-cli -- serve`) before the dev server. See
[Testing](testing.md) for the Playwright E2E setup.
