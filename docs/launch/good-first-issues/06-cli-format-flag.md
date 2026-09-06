# good first issue: unify the CLI's output flags into one `--format table|json|csv`

**Suggested labels:** `good first issue`, `enhancement`, `cli`

## The gap, checked directly

Every `tack` CLI subcommand that prints structured output has its **own** `--json: bool`
flag, repeated per-subcommand rather than declared once — `grep -c 'json: bool'
crates/tack-cli/src/main.rs` currently returns 89. There is no CSV output option
anywhere, and no single top-level `--format` flag; the top-level `Cli` struct
(`crates/tack-cli/src/main.rs`, near the top of the file) only has `--api-url` and
`--token` at that scope.

## Why this is worth doing

89 copies of the same boolean means adding a third output mode today would mean editing
89 call sites individually — exactly the kind of thing a `--format table|json|csv`
enum, declared once, would prevent. This is also a good task to learn the CLI's actual
shape: it's a thin HTTP client (`crates/tack-cli/src/client.rs` wraps `reqwest`) that
talks to the same API a browser would, never opening the database directly (`tack-cli`
must never depend on `tack-db` — that's an enforced layering rule, not just a
convention).

## Where to start

1. Add a `format: OutputFormat` field to the top-level `Cli` struct (next to `api_url`/
   `token`), backed by a `clap::ValueEnum` with `Table` (default, matches today's
   behavior), `Json` (matches today's `--json` flag), and `Csv` (new).
2. Don't remove the existing per-command `--json` flags in the same PR — that's a
   breaking change to a public CLI surface for existing scripts and users. Add the new
   global flag alongside them, prefer it when both are given, and note the old flags as
   an alias/deprecated path (or open a separate follow-up issue for actually removing
   them later, with a deprecation window).
3. Start with **one** command's actual table-vs-json-vs-csv rendering (`tack items
   list` is a reasonable one to prove the pattern on — see the existing table-printing
   code around it in `main.rs`), rather than converting every subcommand in one PR.
4. Write output formatting as small, testable functions (input: the deserialized
   response type; output: a `String`) rather than printing inline, so a test can assert
   on exact output instead of capturing stdout.

## What "done" looks like for a first PR

One command with all three formats working, each covered by a test asserting on the
literal formatted output (header row present for `csv`, valid JSON for `json`, etc.),
plus the new `--format` flag documented in `docs/book/src/user-guide/cli.md`. A
follow-up issue for converting the remaining commands is a completely reasonable thing
to open rather than trying to do all 89 call sites in one PR.

## Before you start

Read `CONTRIBUTING.md`'s "Pull Request Process" section. This issue does not require
reading this repository's internal planning board to get started — `main.rs` and
`client.rs` have everything needed.
