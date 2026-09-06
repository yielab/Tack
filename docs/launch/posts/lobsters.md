# Lobsters (draft, not posted)

**Format notes:** Lobsters submissions are a title + URL + tags, with an optional short
comment as the first reply — there's no long post body on the story itself. Suggested
tags below are a guess; check current tag list before submitting (invented tags are
rejected). This community is small, technical, and has close to zero tolerance for
promotional tone — the comment should read like an engineer describing their own
system, not pitching it.

## Story

- **Title:** `Tack: a self-hosted project manager that durably recovers a killed AI coding-agent run`
- **URL:** `https://github.com/yielab/tack`
- **Suggested tags:** `rust`, `show`, `practices` (whichever subset actually exists on
  the instance at submit time)

## First-comment text (posted immediately after submitting, as the author)

Built this because I wanted a project manager that could dispatch a board item to
Claude Code or Codex against my own repo, without running anyone else's server or
handing over model credentials. The board and the runner are separate concerns on
purpose: the board (Rust/Axum/SQLite, one binary) never executes code or holds a model
credential; a small runner process does that, pulling work over an HTTP protocol and
reporting back.

The one piece I think is worth a technical look rather than a glance: every attempt gets
a fencing token and a lease. Kill the runner mid-attempt and the board doesn't silently
lose the record or blindly re-run it — the attempt goes to `needs_operator`, and an
explicit operator decision is required before a retry happens. `docs/screenshots/recovery-demo.gif`
in the repo is a recording of that exact sequence, made from a downloaded release
binary running in two Docker containers, not a staged dev build.

Current state, honestly: 0 users outside my own testing, one contributor (me), no
accounts system (one shared bearer token), no notifications, English-only. MIT
licensed, 1,420 tests, `cargo nextest run --workspace` green. Feedback on the fencing/
lease design or the crate-boundary rules (`tack-core` has zero I/O; the orchestration
crate is structurally barred from depending on the HTTP layer) is more useful to me
right now than general reactions to the pitch.
