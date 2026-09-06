# good first issue: in-UI diff review of agent artifacts

**Suggested labels:** `good first issue`, `enhancement`, `frontend`

## The gap, checked directly

When an agent finishes an attempt, the files it produced ("artifacts") are downloadable
one at a time as raw blobs — `frontend/src/shared/runWithAgent/ArtifactDownloadPanel.tsx`
renders a list with a **Download** button per artifact, nothing else. There is no
in-browser view of *what changed*. The content is already reachable over the API —
`GET /api/executions/{request_id}/attempts/{attempt_number}/artifacts/{artifact_id}/content`
(see `docs/openapi.json`, and `artifactsApi.download`/`artifactsApi.contentUrl` in
`frontend/src/shared/execution/artifacts.ts`) — the gap is entirely a missing UI
feature, not a missing endpoint.

## Why this is worth doing

Right now, seeing what an agent actually did means downloading a file and opening it in
something else. For text/code artifacts specifically, an inline diff view (even just
"show this file's content in a `<pre>` with syntax highlighting" as a first step, before
a real two-pane diff) closes the single biggest gap between "the agent ran" and "I can
tell what it did" without leaving the board.

## Where to start

1. Start narrow: render a text artifact's raw content inline (reusing
   `artifactsApi.download` — it's already wired for auth headers and the two failure
   states `ArtifactDownloadPanel.tsx` already distinguishes: `not_found` (404, no such
   artifact) vs. `not_verified` (409, manifest exists but content isn't committed yet —
   see that file's own doc comment for why these are kept semantically distinct, not
   collapsed into one generic error).
2. A real diff view needs a "before" to compare against, which most artifacts don't
   inherently have (a generated file isn't automatically a diff of anything) — decide
   scope explicitly: is this "view this file's content" or "diff this file against the
   pre-attempt version in the checked-out workspace"? The former is a much smaller,
   more mergeable first PR; open a discussion on the issue before building the second,
   larger version, since it may need a repo-side change (the runner would have to
   capture and report a pre-image) that's outside this issue's original scope.
3. For syntax highlighting / diff rendering, check what's already a frontend dependency
   before adding a new one (`frontend/package.json`) — a large diff-viewer library adds
   real weight to the bundle, and this repo enforces an entry-bundle-size ceiling
   (`npm run build` in CI checks it stays under 30 KB gzipped for the entry chunk;
   confirm any new dependency is lazy-loaded/code-split rather than added to that
   entry).

## What "done" looks like for a first PR

A working "view artifact content inline" panel for text-like artifacts (detected by
`kind`/mime type on the `ArtifactRecord`, not by guessing from the extension), with the
existing `not_found`/`not_verified` failure states preserved, and a Vitest test
(`ArtifactDownloadPanel.test.tsx` is the existing sibling test to extend or follow the
shape of) proving each state renders correctly. Binary/non-text artifacts should
continue to offer Download only rather than trying to render them.

## Before you start

Read `CONTRIBUTING.md`'s "Pull Request Process" section and the note above about the
bundle-size gate. This issue does not require reading this repository's internal
planning board to get started — the two files named above have everything needed.
