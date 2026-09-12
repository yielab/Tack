# ADR 0066: Docket becomes a third harness — the runner spawns it, and the model axis finally has a harness that proves it

**Decide:** approve a third harness kind, `docket`, implemented as a local subprocess the
runner spawns exactly like `codex` and `claude-code` — same frozen five-method adapter,
same probe, same journal. Approve that it speaks a **third wire**, OpenAI-compatible
`/v1/chat/completions`, which is what makes ADR 0063's "the model is a free axis" true in
practice rather than only in principle. Approve that Tack builds **nothing** until docket
publishes a versioned non-interactive harness contract, and that the one interface debt a third
adapter would turn into the normal path — a handle that cannot name its own harness — is closed
**before** the adapter, not after it.

Approve equally what this is not: it does not replace `claude-code` or `codex`, it does not
touch the ADR 0060 control-plane bridge, and it does not make Tack's runner reach a network
service.

**Why now:** three capabilities the runner-v1 protocol defines — `cancel`, `decisions`,
`artifacts` — have **no harness that implements them**. Both shipped adapters declare
`cancel: Advisory`, `decisions: Unsupported`, `resume: Unsupported`, each with an honest
reason, and none of those reasons is fixable from this side: they are properties of two
closed CLIs. That is this tree's named recurring defect — a well-built mechanism with no
caller — sitting in the *protocol*, not in a module. A harness whose process, process
group, tool loop and approval gate all belong to us is the only candidate that can turn any
of them into `Supported`.

**If you do nothing:** Tack's execution story stays entirely rented. Every attempt runs
inside a vendor binary that self-updates, whose output shape Tack parses, whose cancellation
Tack cannot guarantee, and whose permission enforcement happens somewhere Tack cannot
observe. `permission-policy` remains a field Tack passes to a program that decides on its
own what to do with it.

## The decisions, in short

| # | Decision | Why |
|---|---|---|
| 1 | A third harness kind, `docket`, is added as a **local subprocess** adapter under `crates/tack-runner/src/harness/`. It implements the same `HarnessAdapter` and `HarnessProbe` as the other two, unchanged. | The seam already exists and is frozen. A harness that needed a new seam would be a different, much larger decision. |
| 2 | It is **not** the ADR 0060 bridge. No `orch_*` table, no `ControlPlane` method, no `TACK_ORCH_*` gate, no route. Both may be installed; they never interact. | ADR 0060 is *board → control plane*. This is *runner → subprocess*. Reusing anything from that path would create a second meaning for the word "docket" inside one product. |
| 3 | Its wire is **OpenAI-compatible `/v1/chat/completions`**, a third `Wire` variant alongside `AnthropicMessages` and `OpenAiResponses`. Every existing provider must answer for it explicitly: Vercel AI Gateway serves it at `/v1`; Anthropic answers `None` until someone measures otherwise. `wire_for_harness` and `CATALOG_ELIGIBLE_HARNESSES` gain the `docket` kind. | ADR 0063 decision 3 says a harness declares a wire, never a vendor. This is the wire that the largest number of vendors actually serve, so it is where that principle stops being theoretical. A provider that silently reports an endpoint for a wire nobody measured is exactly how the catalog would list a model no harness can reach. |
| 4 | Tack depends on a **published, versioned docket harness contract**, never on docket internals. If that contract does not exist, Tack writes no adapter. | The other two adapters are ~5k lines of reverse-engineering precisely because no contract was on offer. Repeating that against a repo we control would be a choice, not a constraint. |
| 5 | **One** interface debt is closed as a precondition: `LocalRunHandle` gains a `harness_kind` field, and the literal construction in `tests/crash_matrix.rs` that blocks it is fixed. The second documented debt — the kind key typed twice — is **not** required by a third harness and is not part of this ADR. | A third adapter makes the encode-the-kind-into-`process_id` workaround the normal path, and that is worth ending first because the fix is small. Unifying the two kind types is a refactor with no functional trigger here; `Other(String)` already carries a third kind. Making it a precondition would gate delivery on cleanup. |
| 6 | Capabilities are declared from what is **proven for this adapter**, never from what docket can do in general. v1 declares `cancel: Advisory`, `resume: Unsupported`, `decisions: Unsupported`, `artifacts: Unsupported`, and `usage` with real token counts and cost explicitly unmeasured. Each upgrade to `Supported` is its own card with its own proof. | docket's bash tool starts its command in a **new session** (`start_new_session=True` in `toolbox.py`), which is the same structure that makes Claude Code's cancel Advisory: a `SIGKILL` escalation to the harness cannot reach that session. docket being ours means the fix is buildable — it does not mean it is built. The probe registry rejects an overclaimed `cancel` at registration, and that guard exists for exactly this temptation. |
| 7 | **Workspace ownership stays with Tack's engine.** The adapter passes a prepared workspace in and docket provisions none — no pod, no worktree, no clone. | `HarnessOutcome::normalize_workspace_facts` already asserts one source for workspace facts. Two systems each believing they own the checkout is a correctness bug, not a configuration choice. |
| 8 | A zero cost from docket maps to **unmeasured**, never to `$0.00`. Token counts, which docket reads off the response body, are reported as measured. | docket's own driver capability flag says it does not report USD. Rendering that as zero would violate this tree's standing rule and would understate real spend on an operator's screen. |
| 9 | Each attempt gets its **own `DOCKET_HOME`, created inside the attempt workspace** so `WorkspaceManager::cleanup` disposes of both together. The adapter refuses to run against a default or user home. | `~/.docket` holds a person's real approvals and audit log. An execution runner must never write into it, and "must never" is enforced by refusing, not by documenting. Placing the home inside the workspace means no second lifecycle to get wrong. |
| 10 | The prompt travels on **stdin**, never argv; endpoint and credential travel in the **environment**; the attempt id travels on **argv** as `--attempt <id>`. `reconcile` recovers by pid **plus** that argv token, not by program name. | `ProcessSpec` already prefers stdin so prompts stay out of `/proc/<pid>/cmdline`. docket runs as `python -m docket`, so the program-name match the other adapters use would match any Python process; the attempt id on the command line is a non-secret identity a reused pid cannot forge. |
| 11 | Every event docket emits passes through `harness/redact.rs` before it is journaled or forwarded, exactly as the other two harnesses' output does. | Logs carry ids only. The contract says docket never prints the key; Tack does not rely on that and redacts anyway. |
| 12 | Tack keeps **its own captured fixtures** of the harness contract under `crates/tack-runner/tests/fixtures/docket/`, each with a provenance line (live capture vs constructed), plus one end-to-end test that runs the real `docket` binary against a **fake `/v1/chat/completions` server** and is skipped, not failed, when the binary is absent. | This is the pattern `tack-orch/tests/fixtures/` already uses for docket's HTTP wire. And unlike the other two harnesses, this one can be exercised end to end in CI with **zero spend and no vendor account** — the wire is open, so the test server can be ours. |
| 13 | **Out of scope here, each needing its own ADR:** a remote docket harness, docket's multi-agent pod model, and routing a mid-run approval to a Tack operator. | Each is a genuine feature and each changes a different invariant. Bundling them would turn one reviewable decision into four unreviewable ones. |

If you accept this table, you have accepted the ADR — record the date at the bottom.
Everything past this point is supporting detail for whoever implements or later audits one
of these calls; nothing above depends on anything below it.

---

- **Status:** proposed 2026-09-08.
- **Date:** 2026-09-08
- **Plan:** `docs/plans/harnesses.md`, which sequences this after Part IX's IX-M5 (the
  harness core) and alongside ADR 0067 (opencode) on the same wire.
- **Counterpart:** docket decision D-35, `../rack-cli/docs/adr/0001-harness-mode.md`. That
  document specifies the contract decision 4 requires. **Neither is implementable alone**,
  and docket's is the one that must land first.
- **Relationship to earlier ADRs:** supersedes nothing. **Extends ADR 0063** — its decision
  3 ("a harness declares which wire it can be pointed at, never which vendors it supports")
  gains its first harness where the wire is not a vendor's own. Its decision 8 (the criteria
  that removed `opencode`) is the bar this harness is measured against, and decision 5 below
  is how it stays measured against it. **ADR 0060 is untouched in substance** — decision 2
  keeps this out of the control-plane bridge entirely. Wire contract
  `docs/contracts/runner-v1/` is **unchanged**: a new harness kind is an opaque string in an
  existing field.

## What is true today, measured

Every claim below was read from the tree on 2026-09-08.

| Fact | Where |
|---|---|
| The adapter interface is five methods and is described as frozen | `crates/tack-runner/src/engine.rs:139-148` |
| Capability reporting is a second, separate trait, because the first five all require a claimed attempt | `crates/tack-runner/src/harness/mod.rs:187-225`, and its "Why `HarnessProbe` is not a sixth method" section |
| `wait` must return terminal state, terminal reason, checkpoint, actual execution and usage | `HarnessOutcome`, `crates/tack-runner/src/engine.rs:109-115` |
| Workspace facts are normalized by the engine, overwriting whatever the adapter said | `HarnessOutcome::normalize_workspace_facts`, same file |
| Both shipped harnesses declare `cancel: Advisory`, `resume: Unsupported`, `decisions: Unsupported` | `feature_capabilities` in `harness/claude_code.rs` and `harness/codex.rs` |
| Claude Code's cancel is Advisory because its Bash tool runs in a **new session**, which `kill(-pgid, …)` cannot reach | `harness/claude_code.rs` module doc, confirmed twice via `ps` |
| Registering a probe that overclaims `cancel` is rejected before any attempt exists | `harness/mod.rs`, `registering_a_probe_that_overclaims_cancel_support_is_rejected_before_any_attempt_exists` |
| `LocalRunHandle` carries no harness kind; the registry encodes it into `process_id` as a workaround, and the obvious fix is blocked by a literal construction in a test | `harness/mod.rs` module doc, gap 1, naming `crates/tack-runner/tests/crash_matrix.rs:277` |
| The kind key is typed twice: `tack_orch::execution::HarnessKind` and `registry.rs`'s own enum | `harness/mod.rs` module doc, gap 2 |
| Two adapters cost 5,274 lines, plus 1,195 for the registry and probe | `wc -l crates/tack-runner/src/harness/*.rs` |
| docket's bash tool starts its command with `start_new_session=True` and kills by group on timeout — the same session structure that makes Claude Code's cancel Advisory | `run_bash`, `_kill_group` in `../rack-cli/src/docket/edges/adapters/toolbox.py` |
| The other adapters' `reconcile` recovers by pid plus a program-name match against the located binary | `reconcile` and `process_program_matches` in `harness/claude_code.rs` |
| Harness binaries are found on `PATH` and then a fixed per-user install list, because desktop and `systemd --user` launches inherit a minimal `PATH` | `harness/locate.rs` module doc |
| The workspace manager owns cleanup of the attempt directory | `WorkspaceManager::cleanup`, `crates/tack-runner/src/workspace.rs:178` |
| docket ships file read/write/edit, glob, grep and bash tools, with path jailing, sandbox modes, a jailed environment and process-group kill | `../rack-cli/src/docket/edges/adapters/toolbox.py` |
| docket evaluates a policy verdict per tool call and audits the decision | `evaluate_tool_call`, `_audit_tool_decision` in `../rack-cli/src/docket/core/tools.py` |
| docket retains the full parsed response body, so the served model is already in memory and merely unread | `ChatResponse.raw`, `../rack-cli/src/docket/core/llm.py` |
| docket reads real token counts off the response rather than estimating them, and says so | `TokenUsage`, same file |
| docket's driver reports no USD cost and declares that fact rather than returning a made-up number | `DriverCapabilities.reports_cost_usd`, `../rack-cli/src/docket/core/runtime_driver.py:204-215` |
| A process-wide endpoint override already exists at the highest precedence, so no config file is needed to point docket at a gateway | `resolve_endpoint`, `../rack-cli/src/docket/edges/adapters/llm.py` |

## Why the wire, not the vendor, is the whole point

ADR 0063 decided that a harness constrains exactly one thing about the model: the wire it
speaks. Both existing harnesses speak a wire owned by the same company that ships the
harness, so the distinction has never been load-bearing — it reads as a technicality. A
third harness whose wire is an *interface* rather than a product is what makes the axis
real: the same adapter reaches a hosted gateway, a vendor's own endpoint, a self-hosted
vLLM, or a model on the operator's own machine, and Tack's code does not know or care which.

This is also the honest answer to "can Tack run Kimi or Qwen." Through a gateway's
Anthropic-Messages shim it depends on that gateway's translation layer and is not measured
here. Through this wire it is a base URL.

## The three capabilities this could make real, and the discipline that applies to them

None of these is claimed by decision 6. They are the named triggers.

**`cancel: Supported`.** Both current adapters are `Advisory` because a tool call escapes the process group they can signal, and docket's `run_bash` does the same thing for the same good reason. The difference is that docket's side is buildable: harness mode can record every child process group it starts, and the adapter can terminate those groups itself after the harness is gone. If — and only if — that mechanism exists and the crash matrix demonstrates termination, this adapter may declare `Supported`; the registry guard makes an unproven claim fail at registration.

**`decisions: Supported`.** docket has an approval state and an approval store; Tack has
`AttemptState::WaitingDecision`, a decisions transport and a fail-closed decision token, and
no harness that ever enters that state. Connecting them would put a mid-run tool approval on
an operator's board. It is genuinely valuable and it is genuinely a separate ADR, because it
turns a spawn-and-wait subprocess into something that must survive a pause.

**`artifacts: Supported`.** docket's own toolbox performs every write, so it knows what
changed without inferring it from a diff. That is strictly better information than either
current adapter has.

## What this deliberately does not do

- **No remote docket.** The adapter spawns a local process. A docket reachable over the
  network is a different trust boundary, a different credential story and a different
  recovery model.
- **No pods, no multi-agent runs.** Tack's unit of execution is one attempt with one
  outcome. docket's team model is one of its best features and does not fit that unit; a
  future ADR may add a shape that does.
- **No replacement.** `claude-code` and `codex` stay first-class. The product claim is a
  choice of harness, not a migration away from one.
- **No commitment to Python in Tack's own distribution.** Tack stays a single binary. This
  harness, like the other two, is discovered if installed and simply absent if not — a
  missing binary leaves no entry in `capabilities.harnesses` at all.

## The risk this accepts

**It couples two products that are independent today.** If `docket` becomes the harness that
carries Tack's differentiation, a defect in docket's turn loop becomes a Tack incident and
the two release cadences bind together. Right now either product can be abandoned without
touching the other, and that is a real asset being spent.

Three things bound it. The harness is optional and never the only one, so a broken docket
degrades Tack to exactly today's behaviour. The dependency is a versioned published
contract rather than an import, so it can be served by a frozen docket release. And the
adapter is one file behind a frozen trait, which is the same reversibility argument docket's
own architecture principles make for their adapters.

**Environment sensitivity is real and only partly mitigable.** ADR 0063 decision 8 removed `opencode` in part for being environment-sensitive, and a Python program is sensitive by construction: which interpreter, which site-packages, which `pipx` venv. The mitigation is that the probe records the resolved executable and its version, harness mode reads nothing but its environment and its arguments, and the install guidance names one blessed method. What remains — a machine whose `docket` resolves to a different interpreter tomorrow — is accepted and made visible, not hidden.

The residual risk is **effort**: 5,274 lines bought two adapters, and although a cooperative
upstream should cost far less, "far less" is not measured and must not be quoted as though
it were. The precondition in decision 5 and the contract in decision 4 exist to make the
number knowable before most of it is spent.
