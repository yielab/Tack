## What

<!-- One-sentence description of the change. -->

## Why

<!-- Motivation: bug fix, new feature, refactor? -->

## Related issue

<!-- "Closes #123", or "N/A" for a small self-contained fix (docs, tests, typo) that
     CONTRIBUTING.md's PR process says can skip the discuss-first step. -->

## How to test

<!-- Steps to verify this works — curl commands, UI flow, or test name. A new test you
     added counts; this section shouldn't just restate the checklist below. -->

## Checklist

- [ ] `cargo fmt --all --check` clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo nextest run --workspace` passes (never `cargo test --workspace` — see
      `CONTRIBUTING.md`)
- [ ] Frontend changed? `cd frontend && npm run type-check && npm test` passes, and
      `npm run gen:api` produces no diff in `schema.gen.ts` if the API response shape
      changed
- [ ] Docs updated if this changes behavior, config, or the API (the changelog is generated from the commit messages — use `feat:`/`fix:`/… types)
- [ ] Commit messages have no AI-attribution trailers (see `CONTRIBUTING.md`)
