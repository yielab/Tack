# r/rust (draft, not posted)

**Format notes:** markdown tables supported. r/rust is the least tolerant of these four
audiences toward anything reading as a pitch — lead with the engineering problem, keep
the product framing minimal, expect the top comments to be about the architecture and
the testing approach, not the UI.

## Title

```text
Tack: a modular-monolith Rust workspace where a "kill -9 mid-task" is a tested,
recoverable state, not an incident
```

## Post body

Six-crate Cargo workspace (`tack-core`, `tack-db`, `tack-orch`, `tack-api`, `tack-runner`,
`tack-cli`), Axum + `sqlx`/SQLite on the server, a separate binary for the piece that
actually shells out to an agent CLI (Claude Code / Codex). The layering rule that's
actually enforced, not just documented: `tack-core` has zero I/O — no `tokio`, no
`sqlx`, nothing — and `tack-orch` (the reconciliation/execution-domain crate) is not
permitted to depend on `tack-api`, checked by grep in CI so it can't regress silently.

**The part I think is actually interesting for this sub:** the runner talks to the
server over a pull-based HTTP protocol (`/api/runner/v1`), and every attempt-scoped
write is guarded by a fencing token, not just a status flag. If a runner dies mid-attempt
and comes back (or a second runner claims the same work), a stale fencing token gets
`stale_lease` and writes nothing — proven in tests by asserting the actual row count is
unchanged after a rejected write, not just that the HTTP status was right. `BEGIN
IMMEDIATE` is mandatory on every read-then-write transaction against the SQLite pool —
deferred transactions here deadlock under real concurrency, and that's caught by
concurrency tests running against a file-backed DB rather than the shared in-memory
harness (the in-memory one won't reproduce it).

Numbers, measured today (`2026-09-06`), not aspirational:

| | |
| --- | --- |
| Rust | 127,815 lines across the six crates (`find crates -name '*.rs' \| xargs wc -l`) |
| Tests | 1,420 passed, 7 skipped, 0 failed, `cargo nextest run --workspace`, ~26s |
| Migrations | 62, one `ALTER` per migration file — the runner has no transaction wrapping a whole migration, so a multi-statement migration failing halfway would brick the install; the fix was a rule, not a retry loop |
| API surface | 97 documented paths, OpenAPI-generated, diffed against the handler set in CI (a drift gate, not just docs) |
| Release binary | 19.3 MiB static, `x86_64-unknown-linux-musl`, `lto = true, opt-level = "z"` |
| Idle RSS | ~13.6 MiB (`/proc/<pid>/status`, `VmRSS`) |

Frontend is SolidJS, types generated from the OpenAPI spec rather than hand-kept in
sync — `schema.gen.ts` is regenerated and diffed in CI so a Rust response-shape change
that doesn't update the frontend fails the build instead of shipping a silent mismatch.

Things I'd genuinely like eyes on:
- The `remote_string_enum!` macro pattern for round-tripping an external system's enum
  values including an `Unknown(String)` variant that never loses the original wire
  string — used for a third-party control-plane integration where the upstream project
  is beta and can add variants without warning.
- Whether the crate-boundary rule (`tack-core` zero-I/O, `tack-orch` never depends on
  `tack-api`) is enforced the right way — right now it's `grep -rn tack_api
  crates/tack-orch/` in review/CI rather than a workspace-level lint; happy to hear if
  there's a cleaner way to fail this at compile time.
- General Axum/`sqlx` review — this is a real, running system, not a toy, so actual
  criticism is more useful than encouragement.

MIT licensed, 0 stars so far (new post, first time anyone outside this repo has seen
it): <https://github.com/yielab/tack>.
