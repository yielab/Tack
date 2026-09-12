# Plan: four harnesses on one core

Implements ADR 0066 (docket as a third harness), ADR 0067 (opencode readmitted as a
fourth), and docket's own ADR 0001 / D-35 (harness mode, in `../rack-cli`). The evidence
behind the shape is `docs/plans/harness-maintainability-audit.md`. **All three ADRs are
proposed, not accepted; nothing below starts until they are** — except that the audit's
core-extraction card is already Part IX's IX-M5 and runs on Part IX's schedule regardless.

## The two things this plan is subordinate to

**Part IX is the live board and takes priority.** This plan touches no file under
`crates/tack-runner/` until IX-M5 (Wave 30) has landed: that card extracts the core every
harness here is written against, and IX-M4's `tack-runner` sub-cards own the crate's test
binaries until then. The only work that runs in parallel with Part IX is in **another
repository** — docket's harness mode.

**Part IX's rules are this plan's rules.** Every card here carries the per-card budget of
`docs/plans/human-maintainability.md` §2.3 — at most 15 new tests and 600 new test lines,
measured by `scripts/maintainability.py check --changed` and written into the handoff —
plus the audit's harness cap: at most 400 lines of production code for a harness's
descriptor and grammar together. A card that needs more has found a gap in the core, which
is its own card, not a reason to widen this one.

## The shape, in one paragraph

One core, four grammars, fixtures per version. `LocalProcessHarness<G>` owns everything a
local subprocess harness has in common — spawn with a cleared environment, prompt on stdin,
journal, redaction, cancel escalation, reconcile by pid, version parsing, usage with cost
unmeasured. Each harness contributes a `HarnessGrammar`: a descriptor (where it lives, how
it reports its version, which version range its fixtures were captured against), how its
stream becomes events, how events become an outcome, and what it honestly supports. Its
tests parse fixture files captured from the real binary with a provenance line; the
lifecycle is tested once, in the core, against the fake harness. The two open-wire harnesses
(docket, opencode) additionally run end to end in CI against a fake `/v1/chat/completions`
server: real binary, real workspace, real diff, zero spend.

## The dependency picture

```
docket repo (../rack-cli)               Tack — Part IX (live)          Tack — this plan
─────────────────────────               ─────────────────────          ────────────────
H1 harness run ─┬─ H2 served model ─┐
                ├─ H3 capabilities ─┼─ H4 contract ─────────────────────────┐
                └─ H5 graceful stop ┘                                       │
                                        IX-M4 ×28 → IX-M5 core ─► T1 handle kind ─► T2 third wire
                                                                                         │
                                                                     ┌───────────────────┴──────────────────┐
                                                                T3 docket grammar                   O1 opencode grammar
                                                                T4 docket proof                     O2 opencode proof
                                                                     └───────────────────┬──────────────────┘
                                                                                   T5 docs, once, for both
Phase 4: upgrades — each its own card, each gated on its own proof, none scheduled here
```

Two things run the day the ADRs are accepted: Phase 0 and the docket-repo cards H1–H5.
Everything in Tack waits for IX-M5. T3 and O1 run in parallel — disjoint files, one
grammar each — and T5 is one documentation card for both, not two.

## Phase 0 — measure before building (no code)

| Measure | Command | Decides |
|---|---|---|
| Whether opencode's first-run `node_modules` population reaches the network | run once in a fresh `OPENCODE_CONFIG_DIR` under `strace -f -e trace=network` or with outbound blocked, then again warm | ADR 0067's last risk: probe-time pre-population vs a typed `validate` rejection under `network: false` |
| Probe latency of `docket --version` cold | `for i in 1 2 3; do /usr/bin/time -f %e docket --version; done` | whether the probe runs on every capability refresh or is cached per runner session (opencode's 0.39 s and codex's 0.02 s are already measured) |
| What a running `docket` shows as in `ps` | `docket harness run … & ps -o pid,comm,args -p $!` | confirms ADR 0066 decision 10: `reconcile` matches the argv attempt id, not the program name |

Each number goes into the handoff of the card that uses it, with its command beside it.

## Phase 1 — the docket contract, in docket's repository

Five cards in `../rack-cli`, specified by docket ADR 0001 (D-35). **Nothing from that repo
is ever committed into this tree**; Tack cards read it as the contract and copy nothing.
Every docket card sets `DOCKET_HOME` to a directory it owns and never touches `~/.docket`.

- **H1 — `docket harness run` exists and blocks until done.** D-35 decisions 1, 2, 3, 7,
  8, 11 and the stdin half of 4: one Typer subcommand, one `core/` function driving the
  existing `agent_loop` with `DocketDriver` unchanged; prompt on stdin, `--workspace`,
  `--attempt`; endpoint and key from `DOCKET_LLM_BASE_URL` / `DOCKET_LLM_API_KEY` only;
  refuses the default `DOCKET_HOME`; no pod, worktree or clone; a gated tool call is a
  terminal `blocked` result. *Must not build:* a server, a thread, driver selection, a
  persisted format, an approval pause. *Acceptance:* against docket's own fake server one
  prompt runs to `done` and edits a file in the supplied workspace; with `DOCKET_HOME`
  unset it exits with the guard code and writes nothing.
- **H2 — the result states which model served.** D-35 decision 6: `model` from the retained
  `raw` body into `served_model`, `null` when absent. *Acceptance:* a fake that serves a
  different model than requested is reported as the served one.
- **H3 — `docket harness capabilities --json`.** Contract version, docket version, what this
  build supports (`cancel: advisory` until H5's upgrade path is proven), no model list.
  *Must not build:* a catalog fetch.
- **H4 — the contract is published, versioned and pinned.** `docs/contracts/harness-v1/`:
  one fixture per event type and per terminal result, a README stating the
  stdin/argv/env/stdout/exit-code contract, a byte-pinning test; ROADMAP §6 gains D-35
  **and corrects D-14**, which still names OpenClaw as the shipped driver. **This is the
  gate for T3.**
- **H5 — graceful stop, and the child group ids that make more than advisory possible.**
  D-35 decision 9: on `SIGTERM` terminate every child group, emit a terminal `cancelled`
  result, exit within a bounded grace; emit each child group id as an event when it
  starts. *Must not build:* any claim that cancellation is guaranteed.

## Phase 2 — the core and its two seams, in Tack

- **IX-M5 — the harness core.** Owned by Part IX, Wave 30, unchanged: extract
  `LocalProcessHarness` and `HarnessGrammar`, migrate `claude_code` and `codex` with no
  behaviour change, captured transcripts become fixture files, the live tests move under
  `tests/live/`, then prune to the audit's §5. This plan adds nothing to that card.
- **T1 — `LocalRunHandle` names its own harness kind.** ADR 0066 decision 5. Add
  `harness_kind`; fix the literal construction in `tests/crash_matrix.rs`; delete
  `encode_handle`/`decode_handle` and their tests. *Must not build:* the second documented
  debt (the duplicated kind types) — the ADR excludes it. If IX-M5's extraction has already
  removed the encoding, this card is closed by its integrator with the measurement, not
  re-done. *Acceptance:* `grep -rn encode_handle crates/` is empty; crash matrix green.
- **T2 — a third `Wire`, for both new kinds at once.** ADR 0066 decision 3 and ADR 0067
  decision 2. `Wire::OpenAiChatCompletions`; `vercel_ai_gateway.rs` answers
  `https://ai-gateway.vercel.sh/v1`; `anthropic.rs` answers `None` with a comment saying
  unmeasured, not absent; `wire_for_harness` and `CATALOG_ELIGIBLE_HARNESSES` gain
  `docket` **and** `opencode`. *Must not build:* a live catalog call for the new wire.
  *Acceptance:* `attach_catalog` records the Vercel catalog under both new kinds and
  nothing under them for the Anthropic provider, in one table-driven test.

## Phase 3 — two grammars, one proof each, one docs card

- **T3 — `harness/docket.rs`.** *Needs H4, T1, T2.* One `HarnessGrammar`: descriptor
  (`locate("docket")` plus `~/.local/bin`; version and capabilities from H3's command; zero
  `model_combinations`); `command` (own `DOCKET_HOME` inside the workspace; prompt on
  stdin; `--workspace`, `--attempt`; only the provider injection's environment plus
  `DOCKET_HOME`); `classify` over the contract's ndjson; `outcome` mapping
  `done/failed/blocked/cancelled` to `Succeeded/Failed/Failed+rule/Cancelled`,
  `served_model` into `actual_execution`, tokens measured, cost unmeasured;
  `capabilities`: `cancel: Advisory`, the rest `Unsupported` with the reason that would
  change each. `reconcile` is the core's, with the argv attempt id as the identity check.
  *Must not build:* a remote mode, a pod, an approval round-trip, any import from
  `tack_orch::adapters`. Budget: ≤ 400 production lines.
- **T4 — docket proof.** Tack's own fixtures under `tests/fixtures/docket/` with provenance
  (captured from H1's binary vs constructed); crash-matrix rows for the `docket` kind
  including pid-reused-by-another-Python; and one end-to-end test in the ordinary test
  binaries — **not** `tests/live/`, because it spends nothing — that runs the real binary in
  a `tempfile` workspace against a `wiremock` `/v1/chat/completions` answering a tool call
  and a final message, asserts the written file and the `served_model`, and is skipped
  with a named reason when `docket` is absent.
- **O1 — `harness/opencode.rs`.** *Needs T1, T2; ADR 0067 accepted.* One
  `HarnessGrammar`: descriptor (`locate("opencode")` plus Homebrew and npm global paths;
  `--version`; tested range `1.18.30`); `command` with ADR 0067 decision 4's full isolation
  set, the `@ai-sdk/openai-compatible` provider block built from the provider injection,
  `permission` built from `permission-policy` (decision 6), stdin closed, `--title` fixed;
  `classify` over `step_start/tool_use/text/step_finish/error`; `outcome` from events never
  exit code (decision 5), served model `requested_not_confirmed` always (decision 3),
  `cost` unmeasured, tokens measured; `capabilities`: `cancel: Advisory` with exit-143
  synthesis. *Must not build:* a second wire, `--attach`, vendor login. Budget: ≤ 400.
- **O2 — opencode proof.** Fixtures captured at `1.18.30` into `fixtures/opencode/1.18.30/`
  — the fourteen rows of ADR 0067's measured table become fixture files or table-driven
  test rows, each with provenance; crash-matrix rows for the kind; the same end-to-end
  fake-server test as T4, skipped by name when `opencode` is absent; and one test that
  Phase 0's network answer is enforced (pre-population at probe time, or a typed
  `validate` rejection under `network: false`).
- **T5 — docs, once, for both.** `docs/CONFIG.md` (no new variables — say so);
  `docs/book/src/user-guide/agent-runners.md` gains two rows in the harness table (wire
  *OpenAI Chat Completions*; endpoints *Vercel `/v1` · any OpenAI-compatible server · a
  local model*); the README's harness table; install guidance naming one blessed method
  per harness (`pipx install docket`; opencode's official installer). Regenerate nothing —
  no API type changed, and `openapi_contract` proves it. ADR 0063's decision-8 row gets a
  one-line "superseded by ADR 0067" note, in place.

## Phase 4 — upgrades, each its own card, none scheduled here

| Upgrade | Gate | Where |
|---|---|---|
| docket `cancel: Supported` | crash matrix shows that after `SIGKILL` to the harness the adapter terminates every group id H5 emitted and nothing survives | Tack only |
| docket `artifacts: Supported` | the result lists the paths its own tools wrote; the adapter reports exactly that list | one-field contract bump, then Tack |
| docket `decisions: Supported` | **a new ADR** — turns a subprocess into a session | both repos |
| opencode served model confirmed | an event or export field carries the served id | Tack, after re-measuring |
| Local-model quick start in the book | the T4/O2 end-to-end test has run against a real local server once, command recorded | docs |

## What this plan refuses, by name

- **No board Part before acceptance.** The integrator creates it from this plan when the
  ADRs are accepted, after Part IX Wave 30, not speculatively.
- **No tack-runner change before IX-M5.** Not even T1. The core is the seam every grammar
  is written against; writing against the old adapters would be writing it twice.
- **No copying docket source or fixtures into Tack.** Tack's fixtures are its own captures.
- **No UI card.** The harness picker renders whatever kinds the runner reports.
- **No new `TACK_*` variable.** Endpoint and key already flow through provider injection;
  per-attempt homes are set by the grammar, never by an operator.
- **No streaming beyond ndjson on stdout**, which `event_sink.rs` already parses.
- **No third abstraction.** `LocalProcessHarness` and `HarnessGrammar` are the two Part IX
  §IX.5 allows; a plugin seam, a registry of descriptors read from a file, or a DSL for
  grammars is out.
- **No second driver, plugin seam, async or server** on docket's side — D-35 decision 2.

## Definition of done for the whole plan

1. `tack serve --with-runner` on a machine with `docket` and `opencode` installed and a
   Vercel key reports four harnesses on the Agents page with the gateway's catalog under
   the two open-wire ones, and reports only what is installed elsewhere.
2. An item runs to completion with `--harness docket` and with `--harness opencode` against
   any catalog model, producing a real diff, measured tokens, `Not measured` cost, and — for
   docket — a `served_model`; for opencode, `requested_not_confirmed`, typed.
3. Both end-to-end fake-server tests pass in CI with zero spend and no vendor account.
4. Every harness is under the audit's §5 budgets and `scripts/maintainability.py check` is
   green on the integrated tree; `grep -rn encode_handle crates/` is empty.
5. ADRs 0066 and 0067 carry acceptance dates; docket's ROADMAP §6 carries D-35 and a
   corrected D-14; ADR 0063's decision-8 row points at 0067.
