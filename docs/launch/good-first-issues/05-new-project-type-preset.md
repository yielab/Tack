# good first issue: add a new project-type preset

**Suggested labels:** `good first issue`, `enhancement`

## What exists today, checked directly

Tack ships 10 project-type presets today (`crates/tack-core/src/models.rs`, `pub enum
ProjectType`): `Software`, `Web`, `Mobile`, `Construction`, `Personal`, `Homework`,
`Maintenance`, `Legal`, `Research`, `Event`, plus `Custom`. Each maps to a workflow
(`crates/tack-core/src/workflow.rs::workflow_for_type`) and a vocabulary
(`crates/tack-core/src/vocabulary.rs::vocabulary_for_type`) — e.g. `Construction` gets a
`construction_workflow()` and renames "epic" → "Building", "sprint" → "Phase", etc. This
is one of the most self-contained, well-precedented extension points in the codebase —
adding an eleventh preset touches exactly two match arms plus one new function each.

## Where to start

1. Pick a domain not already covered (education, events-planning-adjacent-but-distinct,
   healthcare, whatever you actually know the workflow of — a preset written by someone
   who's actually run that kind of project reads authentically; one guessed from
   outside doesn't).
2. Add the variant to `ProjectType` in `crates/tack-core/src/models.rs`, including its
   `Display` impl arm.
3. Add a `your_domain_workflow()` function in `crates/tack-core/src/workflow.rs`
   (`construction_workflow()` right above the dispatch match at line 198 is a complete,
   readable example of the shape expected — a `WorkflowConfig` with `StatusDef` entries
   and WIP limits where they make sense) and wire it into `workflow_for_type()`.
4. Add a vocabulary map in `crates/tack-core/src/vocabulary.rs::vocabulary_for_type()`
   (the `Construction` arm is the existing example — a dozen or so term renames).
5. There's already a test asserting new-domain mappings resolve correctly
   (`workflow_for_type_maps_new_domains` in `crates/tack-core/src/workflow.rs`) — extend
   it for the new variant rather than writing a parallel one.

## What "done" looks like for a first PR

A new `ProjectType` variant with its own workflow and vocabulary, a unit test proving
`workflow_for_type`/`vocabulary_for_type` resolve it correctly, and (if you want the
preset to be selectable from a project-creation template rather than only via the API)
a matching entry wherever the frontend's project-type picker enumerates the existing
ten — `frontend/src/features/projects/Projects.tsx` and
`frontend/src/features/templates/TemplateCreator.tsx` are the two places that reference
`project_type` today (`grep -rln 'project_type' frontend/src/features/` to confirm
that's still current before you start — code moves).

## Before you start

Read `CONTRIBUTING.md`'s "Adding a New Workflow Preset" and "Adding a New Vocabulary
Pack" sections — they're already written for this exact task. This issue does not
require reading this repository's internal planning board to get started.
