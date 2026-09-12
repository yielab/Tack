# Claude Code — vendor findings

What `crates/tack-runner/src/harness/claude_code.rs` (`ClaudeCodeAdapter`) is built on.
No captured transcripts live here yet — this directory gains `<version>/*.jsonl` fixtures
when the harness core migration (`docs/plans/harness-maintainability-audit.md` §4, card
IX-M5) lands; until then the adapter's tests drive the shared fake harness
(`../fake_harness.sh`), and every claim below was checked once, by hand, against a real
`claude` binary.

**Observed on:** `claude` version `2.1.223`, one machine, one point in time. A finding here
is "this is what that one installed copy actually did", never "this is how Claude Code
behaves on every machine or every version" — where a claim rests on something not
independently invoked (the Bedrock/Vertex/Foundry provider families, confirmed only by
`strings` against the binary, never an actual provider switch), that limit is called out
below rather than left implicit.

## Findings

- `claude --version` prints `"<version> (Claude Code)"` to stdout, exit 0, empty stderr, and
  needs neither `HOME` nor `PATH` — a fast, side-effect-free probe (`detect_version`).
- `claude -p` reads the prompt from **stdin** when no positional argument is given. The
  adapter always uses stdin, never argv, keeping the prompt out of `/proc/<pid>/cmdline`
  (matches `../process.rs`'s own documented preference).
- `--output-format json` (non-streaming) has **no reliable single "model used" field** —
  only an aggregate `modelUsage` map that, even for a single trivial prompt, included a
  second, unrequested internal model (`claude-haiku-4-5-...`) alongside the one actually
  requested. The adapter uses `--output-format stream-json --verbose` instead and reads the
  authoritative model from the `{"type":"system","subtype":"init"}` event's `model` field,
  cross-checked against `assistant` messages.
- `is_error` (boolean) is the only reliable success/failure signal. `subtype` is **not**: an
  invalid-model 404 was observed with `"subtype":"success"` alongside `"is_error":true`. The
  adapter keys exclusively off `is_error`.
- A persisted per-user settings file (`~/.claude/settings.json`, `"effortLevel"`) silently
  changed default behavior in a way that broke an otherwise-valid invocation (`effort
  'xhigh' is not supported when thinking is disabled`) even with the process environment
  fully cleared. The adapter always passes an explicit `--effort high` (verified compatible
  across every model exercised) and `--setting-sources ""` to reduce ambient configuration
  influence over a supposedly deterministic run.
- Claude Code's own Bash tool runs its command in a **new session** (distinct `pgid`/`sid`
  from the top-level `claude` process, confirmed twice via `ps`), unlike the shared fake
  harness's `spawn_child` mode. A graceful `SIGTERM` appeared to let Claude Code clean up
  that detached session itself, but a `SIGKILL` escalation cannot give it that chance, and
  `kill(-pgid, SIGKILL)` does not reach a different session's group — reflected as
  `cancel: Advisory`, never `Supported`, in `feature_capabilities`.

## What the adapter does not attempt

- Resolving `secret_reference`-only environment entries: no secret-store client exists in
  this crate yet. Such entries are skipped with a `tracing::warn!` (name only).
- Enumerating installed/available models ahead of a real invocation: the CLI has no
  `list-models`-style command, so the probe reports zero `model_combinations` rather than an
  unverified static alias list.
- Actually exercising the Bedrock/Vertex/Foundry provider paths: doing so needs real cloud
  credentials this adapter does not fabricate or request.
