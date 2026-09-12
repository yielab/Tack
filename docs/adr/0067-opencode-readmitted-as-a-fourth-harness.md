# ADR 0067: opencode is readmitted as a fourth harness, on the shared core, with its one limitation typed

**Decide:** approve readmitting `opencode` as a harness kind — **not** by restoring the
2 555-line adapter ADR 0063 removed, but as a grammar on the shared harness core that
Part IX card IX-M5 extracts. Approve that it speaks the OpenAI Chat Completions wire
through an environment-injected provider, that every attempt runs in a config home the
runner owns, and that the one thing opencode still cannot do — say which model served a
request — is recorded as a typed `requested_not_confirmed` fact on every attempt rather
than treated as a veto.

**Why now:** ADR 0063 decision 8 removed opencode for three reasons. Re-measured on
2026-09-11 against opencode `1.18.30`, **two of the three no longer hold**: the whole
configuration now travels in one environment variable, and the environment sensitivity
is switched off by per-spawn variables the runner controls. The third — no served-model
field — is still true, and it is the same posture Tack already takes for the other two
harnesses whenever a gateway is in the path. Meanwhile the plan for four harnesses needs
a decision of record for the fourth, and the audit that shaped Part IX found that the cost
of a harness is set by the core, not by the vendor.

**If you do nothing:** Tack offers two harnesses that cost money to use and one that
requires Python. opencode is the only free, single-binary, open-wire harness a user can
install with one command, and it stays out of the tree for reasons that stopped being
true.

## The decisions, in short

| # | Decision | Why |
|---|---|---|
| 1 | `opencode` returns as a harness kind, implemented as a `HarnessGrammar` on the core IX-M5 extracts. **Supersedes ADR 0063 decision 8 only.** Nothing else in 0063 changes. | Restoring the old adapter would add a third copy of the lifecycle the audit found duplicated. A grammar is a few hundred lines; that is the only shape this ADR approves. |
| 2 | Its wire is **OpenAI Chat Completions**, spoken through the `@ai-sdk/openai-compatible` provider that the runner injects. **One wire in v1.** | opencode's bundled `openai` provider speaks the Responses wire and fails against a Chat Completions endpoint. The open wire is the reason to have this harness; a harness declaring several wires is a change to `wire_for_harness`'s shape and its own decision. |
| 3 | The served model is **always** recorded as `requested_not_confirmed`. Upgrading to confirmed needs an event or export field that carries the served id. | No event carries a model, and `opencode export` reports the id that was *requested*. Tack already records gateway-routed runs of the other two harnesses this way; the difference is that here it is unconditional, and the type says so. |
| 4 | Every spawn sets the full isolation set: `OPENCODE_CONFIG_CONTENT` with the provider block, `OPENCODE_CONFIG_DIR` and `HOME` inside the attempt workspace, `OPENCODE_DISABLE_PROJECT_CONFIG=1`, `OPENCODE_DISABLE_AUTOUPDATE=1`, `OPENCODE_DISABLE_MODELS_FETCH=1`, `OPENCODE_DISABLE_SHARE=1`, `--pure`, `--format json`, a fixed `--title`, and **stdin closed**. | Each item was measured to matter: an open stdin hangs the run after `init` with no log line; a missing `--title` triggers a second model call on a model nobody chose; project config is how an unrelated `opencode.json` changed behaviour before. |
| 5 | The outcome is read from the event stream, **never from the exit code**. | A denied tool and a rejected permission both exit 0. |
| 6 | Tack's `permission-policy` maps to opencode's `permission` block per spawn. `webfetch` and `task` are denied unless the policy grants network or sub-agents. | Denying a tool removes it from the list the model is offered, and `ask` in non-interactive mode rejects rather than hangs — so the mapping is enforceable. Both tools are in opencode's default set and neither is something a Tack attempt should get by omission. |
| 7 | `cost` from opencode maps to **unmeasured**; token counts are reported as measured. | For an unknown model the stream says `"cost": 0`. That is a placeholder, and `$0.00` is the literal this tree forbids. |
| 8 | `cancel: Advisory`. `SIGTERM` produces no terminal event and leaves the bash tool's child alive; the adapter synthesizes `Cancelled` from exit 143 and reports the evidence. | The same structure as the other two harnesses, measured the same way. Claiming more is what the probe registry rejects. |
| 9 | Fixtures are captured from `1.18.30` with provenance lines; the descriptor records that version as the tested range. Outside it the probe reports `Advisory` with the reason, never a refusal. | "Untested" is not "unsupported". The version range is what turns a vendor update from a production surprise into a probe warning. |
| 10 | **Out of scope, each its own decision:** multiple wires per harness, `--attach` to an opencode server, and opencode's own vendor logins. | Each changes a different invariant — the wire model, the trust boundary, the credential mode. |

If you accept this table, you have accepted the ADR — record the date at the bottom.
Everything past this point is supporting detail; nothing above depends on anything below.

---

- **Status:** proposed 2026-09-11.
- **Date:** 2026-09-11
- **Plan:** `docs/plans/harnesses.md`. Nothing is built until IX-M5 has landed and this
  ADR is accepted.
- **Relationship to earlier ADRs:** **supersedes ADR 0063 decision 8** and only that row;
  decisions 3, 5 and 6 of 0063 — wire not vendor, served-model honesty, capability claims
  that are load-bearing — are exactly what this ADR is measured against and are unchanged.
  Extends ADR 0066 (docket) by sharing its wire, its `wire_for_harness` entry and its plan.
  `docs/contracts/runner-v1/` is unchanged: a harness kind is an opaque string in an
  existing field.

## What is true today, measured

Measured 2026-09-11 against `opencode 1.18.30` (the installed binary) and a fake
`/v1/chat/completions` server on loopback; zero spend, no outbound network to a vendor, no
file written into any repository. The scripts are the session's, not the tree's; the
fixture card in the plan re-captures every claim into `fixtures/opencode/1.18.30/` with
provenance.

| Fact | How it was seen |
|---|---|
| The whole config loads from `OPENCODE_CONFIG_CONTENT`; `opencode models` lists the injected provider | debug log `loaded custom config from OPENCODE_CONFIG_CONTENT`; `opencode models` output |
| A run writes nothing into the workspace | `git status --porcelain --ignored` empty after a completed run that wrote `HELLO.txt` via the `write` tool |
| Project-local config is switched off by `OPENCODE_DISABLE_PROJECT_CONFIG=1`; user state goes wherever `HOME` and `OPENCODE_CONFIG_DIR` point | config dir populated under the redirected home; nothing under the real one |
| An **open stdin hangs the run** after `init`, silently | three runs of 90–150 s with no log line after `message=init`; the same command with `</dev/null` completes in 1.4 s |
| Without `--title`, a **second model request** is made for the title, on a model nobody selected | fake server log: two requests, the first for `gpt-5.4-nano` under the bundled `openai` provider; one request with `--title` fixed |
| The bundled `openai` provider speaks the Responses wire and **fails** against Chat Completions | `APIError: Received a Chat Completions stream while using the OpenAI Responses API`, request path `/v1/responses` |
| `@ai-sdk/openai-compatible` speaks Chat Completions and completes a tool-calling turn | request path `/v1/chat/completions`, `tool_use` event with `status: completed`, file written |
| **No event carries a model**; `opencode export` reports the requested id, never the served one | fake server answered `model: SERVED-MODEL-XYZ`; export shows `fake-model` and never contains the served string |
| `step_finish` carries real token counts and `"cost": 0` for an unknown model | `tokens: {input: 11, output: 5, …}, cost: 0` |
| A denied tool exits **0**; a rejected permission exits **0** | `tool: invalid … unavailable tool 'write'`, exit 0; `The user rejected permission`, exit 0 |
| `permission.edit: deny` removes `write` and `edit` from the offered tools; `ask` auto-rejects in non-interactive mode without hanging; `webfetch: deny` removes `webfetch` | tools list in the fake server's request log, per run, 1.5 s each |
| The default tool set includes `webfetch` and `task` | tools list: `bash, edit, glob, grep, read, skill, task, todowrite, webfetch, write` |
| The bash tool's child has its **own process group and session** | `ps -o pid,pgid,sid`: `sleep 25` with pgid = sid = its own pid |
| `SIGTERM` to opencode: exit 143, **no terminal event**, the child survives | one `step_start` event on stdout; `sleep` alive after 3 s |
| `opencode --version` takes 0.39 s | `/usr/bin/time`, three runs (`claude` 0.00 s, `codex` 0.02 s) |
| On first use of a fresh config dir, opencode populated `node_modules` under it (a `package.json` naming `@opencode-ai/plugin`) | directory listing; **whether that required network was not isolated** and is a Phase 0 measurement in the plan |

## Why the removal criteria read differently now

ADR 0063 was right on the day it was written and measured what it said. The written
config file is gone because upstream added an environment override; the environment
sensitivity is gone because upstream added switches for each of its sources. Neither
happened because Tack asked, and both could reverse in a release — which is what decision
9's version range and captured fixtures exist for. The served-model gap is not gone, and
this ADR does not pretend otherwise: it types it, on every attempt, and names what would
change it.

## The risk this accepts

**opencode changes weekly and Tack pins nothing about it.** Decision 9 turns that into a
probe warning rather than a production failure, and the fixtures turn a change into a
diff. What remains is that a user on an untested version gets an `Advisory` harness and
may not read why; that is accepted and made visible, not hidden.

**The first-run `node_modules` population may reach the network** from inside an attempt
that was promised none. If Phase 0 finds it does, the adapter pre-populates the config dir
at probe time — outside any attempt — and the attempt's own spawn stays offline; if that
is not possible, `network: false` policies reject `opencode` at `validate`, typed. Either
way the promise holds; what changes is the cost of keeping it.
