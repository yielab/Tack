# Codex — vendor findings

What `crates/tack-runner/src/harness/codex.rs` (`CodexAdapter`) is built on, and what in it
is still a documented guess. No captured transcripts live here yet — this directory gains
`<version>/*.jsonl` fixtures when the harness core migration
(`docs/plans/harness-maintainability-audit.md` §4, card IX-M5) lands; until then the
adapter's tests drive the shared fake harness (`../fake_harness.sh`) except for three
opt-in live tests that resolve a real `codex` binary from `PATH` and cleanly skip when it or
an env var precondition is absent, each additionally `#[ignore]`d so a plain `cargo test`
never attempts them.

**Observed on:** `codex-cli 0.149.1`, one machine, one point in time.

## Measured

- Non-interactive execution is `codex exec --json --model <requested model id>` with the
  agent profile's instructions piped over **stdin**, never argv, for the same
  `ps`/`/proc` exposure reason `../process.rs` documents — confirmed against the real
  binary, including with a provider pointed at a different endpoint via per-invocation `-c`
  overrides (`model_provider=...`, `model_providers.<key>.*`): the request genuinely reached
  that endpoint rather than Codex's built-in OpenAI provider, and no `~/.codex/config.toml`
  was written (`CodexAdapter::start`).
- Version detection invokes `codex --version`, exit code 0; the real CLI prefixes the
  version with a program-name token rather than printing it bare, so detection scans for a
  strict `X.Y[.Z]` numeric token anywhere in the output rather than requiring the whole line
  to be one (`CodexAdapter::detect_version`/`find_strict_version_token`).

## Unverified — documented guesses, not facts

1. The installed binary is literally named `codex`, found via a `PATH` search, never a
   hardcoded path (`CodexLocator`).
2. Because non-interactive stdout/stderr shape is unverified, the adapter never attempts to
   parse it: `terminal_state` is derived solely from the child's exit code (`0` succeeded,
   nonzero/signalled failed, killed-by-timeout failed). It deliberately does not
   special-case the shared fixture's `malformed` mode into a different outcome
   (`classify_exit`) — special-casing it would itself be inventing a contract this file has
   not verified.
3. Whether/how Codex reports which model it actually used is unverified. Rather than
   fabricate an "observed" model, `ActualExecution`'s `model_provider`/`model_id` echo the
   **requested** selection with `model_observation_source = "requested_not_confirmed"` — a
   value not present in any frozen fixture, which only exemplifies `"harness_reported"`.
4. Session resume, a decision/approval protocol, and parseable usage (token/cost) output are
   all unverified. Each is reported honestly (`unsupported`/`advisory` with a reason) rather
   than assumed (`CodexAdapter::feature_capabilities`).
5. Codex's real model-discovery mechanism (if any) is unverified, so
   `HarnessCapability::model_combinations` is always empty — never a hardcoded list.

## Why `ActualExecution.model_provider`/`model_id` are non-nullable but this adapter cannot
   always fill them honestly

`ExecutionRequestSnapshot.requested_model_provider`/`requested_model_id` are `Option<...>` —
nullable when auto-selection is allowed. `ActualExecution.model_provider`/`model_id` are
**not** `Option`. Because this adapter has no verified way to observe which model an
auto-selected Codex run actually used (finding 3 above), it cannot honestly fill a
non-nullable field for that case without fabricating a value — exactly the kind of hidden
fake success this codebase forbids. Rather than guess, `validate` rejects a spec with no
explicit `requested_model_provider`/`requested_model_id` pre-spawn
(`CodexAdapter::check_selection`). This is a real, falsifying observation about the frozen
contract — non-nullable fields this adapter cannot always honestly fill — not something this
adapter resolves unilaterally.
