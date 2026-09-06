# Part VI handoffs — Agent Onboarding & Provider UX (Phase 60)

**Read this file's header and your card's block. Nothing else in it.** The whole file is
~8k tokens (measured: `wc -c` ÷ 4); the header is ~1k and a card's block ~1k. It exists so that a card agent — a Sonnet with a 200k
window — spends its context on the card, not on discovering what to read.

One handoff per card, named `VI-<card>.md` (`VI-A1.md`, `VI-A2.md`, …), written from
[`TEMPLATE.md`](TEMPLATE.md). Each card writes **exactly one**; corrections are appended as
dated amendments and never rewritten. No card edits another card's handoff, and no card
edits the Part VI board in `TODO.md` — the wave integrator updates the board after
independent verification.

The board is `TODO.md` → **Part VI**, §VI.0–§VI.6. The decision of record is
`docs/adr/0061-provider-credentials-at-the-runner-boundary.md`, written by VI-A2 and
accepted by the user before Wave 15 opens. The story every doc reuses is §VI.0's statement.

## Waves, order, and the base to branch from

| Wave | Cards | Parallel? | Needs | Base SHA |
|---|---|---|---|---|
| 14 | VI-A1 · VI-A2 · VI-A3 | all three | nothing | `8152df7` — dispatched 2026-09-03 |
| 15 | VI-B1 → VI-B2 → (VI-B3 ∥ VI-B4) → VI-B5 | **partly** — B3 and B4 ran in parallel and integrated 2026-09-05 at `03df038`; B5 is sequential and next | ADR 0061 accepted (2026-09-03, `VI-A2.md` amendments) | **B2 branches from the 2026-09-04 integration tip — pin it with `git rev-parse --short develop`, do not reuse a SHA from this row.** B1 was dispatched 2026-09-03 from `2958e9e`; — the 2026-09-03 planning commit; branch from the `develop` tip. Decision 1 refined 2026-09-03 (keychain first, file fallback) — B1's block below already matches |
| 16 | VI-C3 · VI-C4 first; then VI-C1; then VI-C2 | C3 ∥ C4 (may start during Wave 15); C1 after B2+B3; C2 after C3 | see each block | **C2, C3 and C4 integrated 2026-09-04.** C1 is the only one left and still needs B2 + B3; it branches from the Wave 15 integration SHA |
| 17 | VI-D2 → VI-D1 | no | D2: C1, C2 and Part V's V-C2 landed; D1: everything | Wave 16 integration SHA |

**Integration line: `develop`.** Every card branches from it as `agent/vi-<card>-<slug>`
(`agent/vi-a1-docs-path`, `agent/vi-b2-vercel-gateway`, …) and never merges itself.

## Before dispatching a wave — the dispatcher's checklist

1. `git status --porcelain` on `develop` is empty and the planning edits (board, roadmap,
   skills, this directory) are committed. Cards branch from a clean line, never from a
   working tree with someone else's edits in it.
2. Record the base SHA in the table above and in the Part VI status table. **Pin the actual
   tip — `git rev-parse --short develop` — not the SHA of the planning commit.** On
   2026-09-03 the prompts named `2958e9e` while `develop` was already at `92a42cd`; every
   agent branched from `2958e9e` and ended up two docs-only commits behind. Harmless that
   time, a conflict the next time the gap contains code.
3. For Wave 15 only: the user has accepted ADR 0061 (the acceptance is dated in
   `VI-A2.md`). Without it, B1–B3 encode a guess. **Done 2026-09-03.**
5. Part VII (`docs/agent-handoffs/part-vii/README.md`) runs alongside this Part. VI-B1 and
   VII-A2 both add an arm to `crates/tack-cli/src/main.rs`: merge sequentially, build once
   with both. VII-B2 reads VI-B3's `GET /api/local-runner` when it exists; VII-C2 waits for
   VI-C1 and for VI-D2 on `README.md`.
4. Reap finished worktree targets — parallel builds fill `/home`
   (`du -sh /var/tmp/tack-agent-targets/*`).

### What the second 2026-09-05 wave added

**The merge built on the first try, and the difference was the plan, not luck.** The
morning's pair shared no file and still did not compile together, because one changed a
type the other called. The afternoon's three — the wire type and its fixtures, the frontend,
the desktop crate — were chosen so that no card could change a type another card called,
which is a stricter test than "disjoint ownership" and the one worth applying when picking
a wave.

**Two cards reported a blocker that was not there, in the same wave.** One said a live
credential was unavailable, when the integration machine has one; the other said an
endpoint's address could not be found without opening files it was told to avoid, when the
route sits in `handlers/runner_protocol.rs` and one grep for `\.route(` lists the whole
table. Both were corrected by the integrator and both amended their own handoffs. The
pattern to expect: **a card's sandbox is smaller than the repository, and it will report
the edge of its sandbox as a property of the code.** When a handoff says "could not", check
whether the integrator can, before that sentence becomes the next card's premise.

### What the 2026-09-05 parallel pair added

**Two cards can share no file and still not compile together.** VI-B3 and VI-B4 had
disjoint ownership, merged with no textual conflict, and each was green in its own
worktree. The merge did not build: B4 changed a type B3 called, in a file B3 owned. A
`git merge-tree` dry run says nothing about this — only building from the merge does.
So the rule is not "check for conflicts before merging", it is **build the merge before
trusting either report**, which is what the checklist below already says and what this
pair is the concrete case for.

**Ask a card for the grep it ran, not the number it got.** VI-B4's acceptance was a
vendor-name grep returning zero, and it did return zero — over the two dispatch
functions. Over the whole tree, splitting each file at its own `#[cfg(test)] mod tests`,
two production sites remained, and one of them would have recorded an unobserved model
as harness-reported the day a second gateway was added. A check that passes because it
looks in the wrong place is worse than no check, because it also stops anyone looking
again. When a card's acceptance is a command, run the command.

### Conflicts, after the 2026-09-04 integration

Two different problems, and only one of them is solvable by tooling.

**Generated files can no longer conflict.** `Cargo.lock`, `frontend/package-lock.json`,
`docs/openapi.json` and `frontend/src/shared/api/schema.gen.ts` are marked
`merge=tack-generated` in `.gitattributes`. The driver resolves them to our side without a
conflict and `.githooks/post-merge` regenerates them from the merged sources; `pre-push`
refuses a stale lockfile or schema. Run `./scripts/setup-git.sh` once per clone — the
driver is per-clone git config and cannot be committed. Never hand-merge one of these
files, and never trust one branch's copy: regenerate after all merges land.

**Hand-written files still conflict, and dispatch is what prevents it.** VI-B1 and VII-A2
each added a `mod` line and a match arm to `crates/tack-cli/src/main.rs`, in the same two
regions. The merge cost thirty seconds, but the rule is cheaper than the merge: **do not
put two cards that edit the same hand-written file in the same wave.** If the board's
ownership table already gives that file to one card, dispatch the other one a wave later.

## When a card agent dies mid-work

Measured 2026-09-03: five Sonnet cards dispatched at once exhausted the session rate limit
after roughly 25 minutes; four of the five died between implementation and handoff, having
written no handoff at all. **Resume them, do not re-dispatch.** The worktree keeps every
edit and a message to the agent restores its context, so a resume costs one message where a
re-dispatch costs a whole cold start and discards finished work. Tell the resumed agent
what it still owes — the remaining gate, the live proofs, and the handoff it never wrote —
because the interruption usually lands before that last step.

Two things to size before a wave: **the session limit** (four to five parallel Sonnet cards
is the observed ceiling for a wave of this shape) and **disk** — the five per-card target
directories reached 53 GB, the two frontend-heavy cards 19 GB each. Reap them after
integration.

## How to dispatch a card to a Sonnet agent

One `Agent` call per card, `model: "sonnet"`, `isolation: "worktree"`,
`run_in_background: true` for every card in a parallel set. The prompt is the generic one
below with the id substituted; the block-specific line is added where the card's block says
so. The dispatcher — Opus, acting as integrator — does **not** read the cards' handoffs
whole afterwards; it runs the escalation grep in `/integrate` §3 and this file's per-wave
checklist.

```text
You are working card VI-<ID> of Tack's Part VI board. Run /card VI-<ID>.
Before reading anything else, read the header and the VI-<ID> block of
docs/agent-handoffs/part-vi/README.md and follow its read list exactly — it is
sized for your context. Read nothing it does not name without recording why in
your handoff. Do not read TODO.md whole, any harness adapter file whole,
docs/openapi.json, or any handoff the block does not name. Do not spawn
subagents. Deliver the card's Acceptance list, run the gate the block names,
write docs/agent-handoffs/part-vi/VI-<ID>.md from TEMPLATE.md, and finish with
the report shape in .claude/reporting-contract.md. Do not commit. If your
context passes ~150k tokens, stop, write the handoff with what you have, and say
so in "Context spent".
```

**Context rules for every card** (they are in the prompt because Sonnet will not infer them):

- **Cold start ≤ 25k tokens** before the first edit, **≤ 120k** at handoff. The numbers in
  each block are measured (`wc -l` × ~16 tokens/line for `TODO.md`, ~12 for code and docs);
  re-measure only if you are about to exceed them.
- `TODO.md` is read by anchor, never by line number and never whole:

  ```bash
  head -58 TODO.md                                             # header, ~1k
  a=$(grep -n "^## §VI.0" TODO.md|cut -d: -f1); b=$(grep -n "^## §VI.4" TODO.md|cut -d: -f1)
  sed -n "${a},$((b-1))p" TODO.md                              # capsule + rules + ownership + graph, ~5k
  n=$(grep -n "### VI-<ID> " TODO.md|cut -d: -f1); sed -n "${n},$((n+75))p" TODO.md   # your card, ~1k
  ```

  That is the whole board any card needs: ~7k tokens. `CLAUDE.md` is already in your
  context; `.claude/scope-discipline.md` is ~2.5k and is worth it once.
- **The four traps in this Part, by size:** `crates/tack-runner/src/harness/{codex,claude_code,opencode}.rs`
  are 1,761 / 2,300 / 2,521 lines (~80k tokens together) — read the ranges the block
  names, never a whole file. `docs/openapi.json` is ~88k — query it with `python3 -c`.
  `TODO.md` whole is ~199k. All handoffs together are ~240k — a block names at most two.
- **A file you opened and did not use is a finding.** List it under "Context spent" in
  the handoff; the integrator uses that to fix this file for the next agent.
- **No subagents from a card.** A subagent costs a full context; every block below is
  sized so one agent can do the card alone.
- Every gate command sets `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-<ID>` — the
  shared target has filled `/home` before.

---

## Wave 14 — Truth first

### VI-A1 — Write the path from a board item to a completed attempt, and delete what is false

**Branch:** `agent/vi-a1-docs-path`. **Owns:** see the card. Pure docs; no crate.

**Read (≈ 22k):**
- The board prelude above (~7k).
- `docs/book/src/user-guide/agent-runners.md` whole (433 l, ~6k) — you are restructuring it.
- `docs/book/src/user-guide/cli.md` whole (238 l, ~3k); `configuration.md` whole (96 l);
  `quick-start.md` whole (122 l).
- `docs/CONFIG.md` whole (151 l, ~2k); `docs/API-REFERENCE.md` `sed -n '1265,1295p'`;
  `docs/MCP.md` `sed -n '95,102p'`.
- The precedence, from its source: `sed -n '1,45p' crates/tack-orch/src/model_policy/wiring.rs`
  and `sed -n '1,30p' crates/tack-orch/src/model_policy/mod.rs` (~1k).
- The CLI's own help, **run, not read**: `cargo run -q -p tack-cli -- execution create --help`
  and the same for `runner`, `fleet`, `agent-profile`, `model-profile`. Never open
  `crates/tack-cli/src/main.rs`.
- The request's required fields: `python3 -c "import json;d=json.load(open('docs/openapi.json'));print(d['components']['schemas']['CreateExecution']['required'])"`.

**Do not read:** any handoff; any adapter; `docs/openapi.json` whole; the roadmap.

**Gate:** `/gate docs` — `mdbook build docs/book` clean, `grep -rn claude_code docs/book/`
empty, and every example in your pages **executed** against a live
`tack serve --with-runner` with its output pasted (the fake-harness shim in
`scripts/smoke.sh` is fine for the attempt itself).

**Handoff extras:** claim → evidence (every example → its transcript); surface-map delta
(none expected — say so); vocabulary check is not required for this card.

**Stop if:** an example cannot be made to run against the current product. Record the
exact failure — that is a finding for VI-D1 or a runtime card, not something to paper over
with a constructed transcript.

### VI-A2 — ADR 0061: provider credentials and model catalogs at the runner boundary

**Branch:** `agent/vi-a2-adr-0061`. **Owns:** the ADR, one bullet of `docs/CONFIG.md`.
No code. **Prepares and stops** — the user accepts the ADR.

**Read (≈ 12k):**
- The board prelude (~7k) — the surface map and the statement are this card's inputs.
- `docs/adr/0050-runner-control-plane.md` whole (99 l); `docs/adr/0058-standalone-single-binary-runner.md`
  whole (103 l); `head -40 docs/adr/0060-docket-control-plane-disposition.md` for the house
  format (~3k together).
- `docs/CONFIG.md` `sed -n '85,110p'` — the bullet you replace.
- What exists on the runner side, by range: `sed -n '1,45p' crates/tack-orch/src/model_policy/wiring.rs`;
  `sed -n '55,70p;925,945p' crates/tack-runner/src/harness/claude_code.rs`;
  `sed -n '765,780p' crates/tack-runner/src/harness/codex.rs`;
  `rg -n "state_dir|RunnerCredential|REDACTED" crates/tack-runner/src/config.rs`;
  `rg -n "with_runner_enabled|loopback|fn " crates/tack-cli/src/local_runner.rs | head -30`;
  `rg -n "app_meta|secret" crates/tack-api/src/handlers/settings.rs | head -15` (the one
  precedent for a UI-written secret, which the ADR must distinguish itself from).
- `grep -n -i "credential\|proxy\|invisible" docs/agent-handoffs/part-iv/IV-A5.md | head`
  — the earlier card that saw the confusion and fixed only the visibility.

**Do not read:** any adapter beyond the ranges above; the frontend; other handoffs.

**Gate:** none to run. The deliverable is reviewed, not tested: every decision names its
rejected options and their cost; 0050 and 0058 are cited by line for what is reaffirmed
and what is bounded; the `docs/CONFIG.md` sentence is replaced; the handoff lists every
sentence in `docs/` and `README.md` the ADR makes imprecise
(`grep -rn "never becomes a model proxy\|no TACK_\* variable for a model provider\|never reads, stores, or forwards" docs/ README.md`).

**Handoff extras:** the six decisions as a table (decision · chosen · rejected · cost);
the user's acceptance line is added by the dispatcher as an amendment with the date.

**Stop if:** a decision needs a fact you cannot measure from the ranges above (for
example whether `local_runner.rs` can start the runner after `serve` is up). State the
question with the smallest experiment that answers it; do not run the experiment.

### VI-A3 — Two components, one product: the README, the introduction and the diagram

**Branch:** `agent/vi-a3-two-components`. **Owns:** `README.md`, `introduction.md`, the
developer overview's opening, `docs/diagrams/**`, one sentence of `CLAUDE.md`. Prepares the
GitHub description; applies nothing outward-facing.

**Read (≈ 11k):**
- The board prelude (~7k) — the statement is applied verbatim; copy it from §VI.0, do
  not retype it.
- `README.md` whole (263 l, ~3.5k); `docs/book/src/introduction.md` whole (60 l);
  `head -40 docs/book/src/developer/README.md`.
- `grep -n -i "claim\|evidence" docs/agent-handoffs/part-v/V-A4.md | head -20`, then only
  the claim → evidence table's range — every claim you keep must keep its proof.
- The recovery-demo slot's source: `n=$(grep -n "### V-C2 " TODO.md|cut -d: -f1); sed -n "${n},$((n+26))p" TODO.md`.
- `ls docs/screenshots/`; `cat docs/book/book.toml` (whether the book has a mermaid
  preprocessor — decides SVG vs mermaid).

**Do not read:** `CLAUDE.md` again (it is in your context; you change one sentence);
the rest of `developer/README.md`; any crate; other handoffs.

**Gate:** `/gate docs` — `mdbook build docs/book` clean. Render proof: three PNGs of the
diagram (GitHub light, GitHub dark, the built book) — use `npx playwright screenshot` or
the `frontend/e2e/screenshots.spec.ts` pattern; attach paths in the handoff. The
stranger-read test: the dispatcher runs it (below); you state which line range constitutes
"the first screen".

**Handoff extras:** `diff` proving the statement is byte-identical across the four places;
vocabulary check (the README may say "runner" — it is telling the story); the drafted
GitHub description; claim → evidence for every first-screen claim.

**Dispatcher's adversarial check:** a fresh Sonnet agent, given **only** the README's first
screen (the line range the handoff names), is asked what the product is. It must answer:
two parts, what each holds, one board / many runners. If it answers "a project manager",
the card is not done.

**Stop if:** a claim on the first screen has no proof in V-A2/V-A3/V-A4's handoffs or the
smoke. Delete the claim; do not soften it.

---

## Wave 15 — Provider at the runner boundary (sequential: B1 → B2 → B3)

### VI-B1 — A runner-local secret store, and `secret_reference` finally resolves

**Branch:** `agent/vi-b1-secret-store` from the Wave 14 integration SHA.

**Read (≈ 19k):**
- The board prelude (~7k) and `docs/adr/0061-*.md` (~2k, decisions 1 and 4 — decision 1
  is keychain-first; the card's Design paragraph is the authority on the two backends).
- The `keyring` crate, 4.x: the platform stores are separate crates
  (`apple-native-keyring-store`, `windows-native-keyring-store`,
  `dbus-secret-service-keyring-store` or `zbus-secret-service-keyring-store`,
  `linux-keyutils-keyring-store`). Confirm names and feature flags with `cargo info keyring`
  and `cargo doc -p keyring` after adding it — do not assume from memory, do not fetch the web.
- The contract's shape: `grep -n -B2 -A6 secret_reference docs/contracts/runner-v1/claim.response.json`;
  `rg -n "struct EnvironmentValue" -A 12 crates/tack-orch/src/execution/types.rs`.
- Each adapter's environment builder, **by range only**:
  `sed -n '920,975p' crates/tack-runner/src/harness/claude_code.rs`;
  `sed -n '740,790p' crates/tack-runner/src/harness/codex.rs`;
  `sed -n '1125,1170p' crates/tack-runner/src/harness/opencode.rs` (~2k together).
- Where validation happens before spawn: `rg -n "fn validate|trait HarnessAdapter|trait HarnessProbe" -A 8 crates/tack-runner/src/harness/mod.rs | head -60`.
- `crates/tack-runner/src/config.rs` whole (208 l, ~2.5k) — the state-dir handling and the
  `RunnerCredential` redaction you copy exactly.
- The CLI arm you extend: `n=$(grep -n "Commands::Runner" crates/tack-cli/src/main.rs|cut -d: -f1); sed -n "${n},$((n+40))p" crates/tack-cli/src/main.rs`;
  `head -60 crates/tack-cli/src/doctor.rs` as the small-module precedent.
- The fake-harness shim you prove with: `rg -n "shim|fake" scripts/smoke.sh | head -12` and
  `sed -n '850,885p' crates/tack-runner/src/harness/mod.rs`.

**Do not read:** any adapter whole (the ranges above are the only parts that change);
`bootstrap.rs`; the frontend; `docs/openapi.json`.

**Gate:** `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-B1 cargo nextest run --workspace -E 'package(tack-runner)'`
and `-p tack-orch --test runner_contract` (must be byte-identical — you changed no
fixture), then `/gate runner`. The reverted-fix proof: remove the resolver once, watch the
"variable is set" assertion fail, restore, record both runs. The keychain proof is live
and manual: `secret-tool lookup service tack-runner account <name>` (Linux) or
`security find-generic-password -s tack-runner -a <name>` (macOS) — record the command and
exit status, never the output. The fallback proof: run with the platform store unreachable
(`DBUS_SESSION_BUS_ADDRESS=/dev/null` on Linux) and assert `tack runner doctor` reports
`backend: file`. Unit tests use the file backend (and `keyring`'s in-crate mock if 4.x has
one — check, don't assume) so CI needs no Secret Service.

**Handoff extras:** secret-path proof (the `stat -c '%a'`, the log capture with the name
present and the value absent, the keychain and fallback commands with exit statuses);
which backend the dev machine ended up on; surface-map delta (none — say so).

**Stop if:** resolution cannot happen at `validate` without touching the journal
ordering. State the ordering constraint you found; do not move the journal write. Also
stop if no platform store can be reached on this machine at all: prove the file backend
fully, record the failure, and leave the keychain proof to the integrator — never fake it.

### VI-B2 — Vercel AI Gateway as a runner provider

**Branch:** `agent/vi-b2-vercel-gateway` from the current `develop` tip. **The heaviest card here.**

**The gateway key is already stored**, in the runner's keychain backend under the name
`vercel-ai-gateway/default`. Reference it by that name; never print, echo or log its value, and
never read it out of the store to inspect it. `tack runner secret list` shows names only.

**Read (≈ 30k):**
- The board prelude (~7k); `VI-B1.md` (how the store is read, ~3k); the ADR (~2k).
- **Vendor facts, re-fetched, not trusted from the card:** `WebFetch` the three pages
  with a narrow prompt each (Claude Code, Codex, OpenCode under
  `vercel.com/docs/ai-gateway/coding-agents/`), ~2k each. Never `curl` a whole page into
  context.
- Each adapter, three ranges each and nothing more — module doc, probe, spawn/args:
  `sed -n '1,90p;680,800p' crates/tack-runner/src/harness/codex.rs`;
  `sed -n '1,90p;800,1000p' crates/tack-runner/src/harness/claude_code.rs`;
  `sed -n '1,90p;1000,1180p' crates/tack-runner/src/harness/opencode.rs` (~9k together).
- `rg -n "pub async fn probe|DiscoveryReport|fn build_adapter_registry" -A 15 crates/tack-runner/src/bootstrap.rs | head -80`.
- `sed -n '40,110p' crates/tack-orch/src/execution/capabilities.rs` (`model_combinations`,
  `model_passthrough`, `additional`).
- `rg -n "model_combinations|model_passthrough|ModelSelector" crates/tack-orch/src/scheduler/select.rs | head -20`,
  then the one function that intersects — you do not change it; you must know what it
  reads.
- `crates/tack-cli/src/doctor.rs` whole (463 l, ~5k) — it gains a provider block.
- `docs/CONFIG.md` `sed -n '85,110p'` — the rows you add sit here.

**Do not read:** any adapter outside the ranges; `select.rs` whole (705 l); `mod.rs` of
harness (1,171 l) beyond a grep; the frontend; other handoffs.

**Gate:** `cargo nextest run --workspace -E 'package(tack-runner) | package(tack-orch)'` (`runner_contract`
byte-identical or an escalation naming the missing field), `-E 'binary(wave2_gate)'`; then
`/gate runner`. **Live proof is billed** — run the real `claude-code` attempt once,
deliberately, and record the request id; the second harness likewise.

**Handoff extras:** secret-path proof (the `sqlite3 tack.db .dump | grep -c` with its
positive control); the measured vendor table (what each harness actually accepted — this
replaces the card's table for VI-C1 and VI-D1); the actual-model observation per harness
with its `model_observation_source`; surface-map delta (the provider row — reached for
which harnesses).

**Stop if:** a harness cannot receive the key non-interactively (OpenCode is the likely
one). Record it as a measured fact with the command that proved it, mark that harness's
gateway path as a console step, and continue with the others — do not write to a user's
`~/.codex/config.toml` or `auth.json` to get past it.

### VI-B3 — The embedded runner, controlled from the UI: turn it on, hand it a key

**Branch:** `agent/vi-b3-embedded-control` from B2's merge.

**Read (≈ 20k):**
- The board prelude (~7k); `VI-B1.md` (~3k); the ADR — decisions 2 and 6 (~2k).
- `crates/tack-cli/src/local_runner.rs` whole (451 l, ~5.5k) — the file you restructure.
- `rg -n "pub async fn serve" -A 20 crates/tack-api/src/lib.rs`; `rg -n "pub struct AppState" -A 30 crates/tack-api/src/*.rs | head -50`.
- The gated-mount pattern you copy: `sed -n '60,140p' crates/tack-api/src/router.rs` and
  `rg -n "require_orch_enabled|orch_routes" crates/tack-api/src/router.rs`.
- The write-only-secret and `app_meta` precedents: `rg -n "app_meta|secret_key|pub async fn" crates/tack-api/src/handlers/settings.rs | head -30`.
- Frontend precedents: `frontend/src/features/fleet/runnerFleet/ModelProfilesPanel.tsx`
  whole (167 l, ~2k) for a panel; `head -60 frontend/src/shared/execution/api.ts` for the
  client shape; `rg -ln "backup" frontend/src/features/settings | head -3` then the
  secret-field component only.
- How E2E starts a server: `head -60 frontend/e2e/helpers.ts`; `head -40 frontend/e2e/smoke.spec.ts`.

**Do not read:** `router.rs` whole (528 l) beyond the ranges; any adapter; `bootstrap.rs`
beyond a grep for the re-probe entry point.

**Gate:** `cargo nextest run --workspace -E 'package(tack-api)'` (includes `openapi_contract` — **regenerate**, do not
hand-edit: `UPDATE_OPENAPI=1 cargo nextest run --workspace -E 'binary(openapi_contract)'` then
`cd frontend && npm run gen:api`), `cargo nextest run --workspace -E 'package(tack-cli)'`, `/gate api`, `/gate frontend`,
and the two Playwright specs you add (`make e2e` scoped with `-g`).

**Handoff extras:** secret-path proof (`app_meta` row count before/after; the dump grep;
the `stat`); the non-loopback 404 proof (the test name); the persisted-flag round trip
(restart without the flag → runner returns).

**Stop if:** the runner cannot be started after `serve` is up without a second code path
into the runtime. Escalate with the exact seam; do not call runner handlers directly (the
Part IV mistake this directory's sibling README warns about applies here unchanged).

---

### VI-B4 — A provider is a module, and a second module proves it (parallel with B3)

**Branch:** `agent/vi-b4-provider-trait` from the `develop` tip you are given.

**Read (≈ 16k):**
- The board prelude (~7k) and the VI-B4 card.
- ADR 0063 — the decision table (`sed -n '23,58p' docs/adr/0063-harness-credential-modes.md`)
  and the amendment of 2026-09-05 at the bottom (`sed -n '/^## Amendment — 2026-09-05: accepted/,$p'`).
  Decisions 4 and 5 are the card; read no other ADR.
- `crates/tack-runner/src/provider.rs` whole (399 l, ~5k) — the file you restructure. It is
  the only file you must read whole.
- Its callers, by grep, not by opening the files: `rg -n "attach_catalog|resolve_endpoint|CatalogStatus|Wire::" crates/ --include='*.rs'`.
  Then read only the ranges that grep names.
- `rg -n "VERCEL_AI_GATEWAY|ProviderConfig" -B2 -A8 crates/tack-runner/src/config.rs`.
- The catalog half of doctor: `rg -n "provider|catalog" -B2 -A10 crates/tack-cli/src/doctor.rs | head -60`.
- Anthropic's current model-list documentation, fetched — not the card's paragraph about it.

**Do not read:** any harness adapter whole (`codex.rs` and `claude_code.rs` are 1,761 and
2,300 lines); `bootstrap.rs` beyond the ranges the grep names; `docs/openapi.json`; any
handoff except `VI-B2.md`'s measured vendor table
(`grep -n -A 12 -i "measured vendor" docs/agent-handoffs/part-vi/VI-B2.md`).

**Gate:** `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-B4 cargo nextest run --workspace`
— the whole workspace, because this card's core claim is that nothing outside the provider
modules changed. Plus `cargo clippy --workspace --all-targets -- -D warnings`,
`./scripts/check-comments.sh`, `./scripts/check-test-hygiene.sh`, and the vendor-string grep
your own gate adds.

**Handoff extras:** the `git diff --stat` for the Anthropic module alone (the card's central
proof); the vendor-string grep with its command and its zero; the byte-identical
`runner_contract` result; the live gateway transcript; the two catalog bodies you parsed,
with any field one publishes and the other does not.

**Stop if:** the common shape needs a field one provider cannot express — record both bodies
and hand the shape to VI-B5, do not invent a union type. Do not touch
`docs/contracts/runner-v1/**` or `crates/tack-orch/src/execution.rs` for any reason: your own
acceptance requires them byte-identical.

---

### VI-B5 — The catalog reaches the board: price, context window and modality on the wire

**Branch:** `agent/vi-b5-catalog-on-the-wire` from the `develop` tip you are given.
**Needs VI-B4, which is merged.** The only card in this Part allowed to change the frozen
contract.

**Read (≈ 14k):**
- The board prelude and the VI-B5 card; ADR 0063 decision 5 only
  (`sed -n '23,58p' docs/adr/0063-harness-credential-modes.md`).
- `crates/tack-orch/src/execution/capabilities.rs` whole (~130 l) — `ModelCombination` and
  `HarnessCapability` are here, not in an `execution.rs`.
- `crates/tack-orch/tests/runner_contract/domain.rs` and `fixtures.rs` — how a fixture is
  loaded and byte-pinned; you are changing what these assert.
- `docs/contracts/runner-v1/capabilities.json` — the fixture that gains the field.
- The shape you are carrying: `rg -n "struct CatalogEntry" -A 20 crates/tack-runner/src/provider/mod.rs`,
  and the two parsers (`anthropic.rs`, `vercel_ai_gateway.rs`) by grep for what each fills.
- Where the runner builds the combination: `rg -n "ModelCombination" -B4 -A12 crates/tack-runner/src/provider/mod.rs`.

**Do not read:** any harness adapter; `docs/openapi.json`; the frontend; `TODO.md` whole.

**The two rules this card exists to respect.** The `additional` map is `#[serde(flatten)]`
and must not carry this — smuggling a field through it is a contract change with no review,
which §VI.2 forbids by name. And a runner that reports no metadata must still round-trip
against a newer board: an older runner is not a break, so the new field is defaulted and
absent-means-absent, never zero. Never render an unmeasured price as `$0.00`; the literal
this tree uses is `Not measured`.

**Gate:** `cargo nextest run --workspace` (the whole workspace — the contract test is the
point of this card), `cargo nextest run --workspace -E 'binary(runner_contract)'` shown
separately in the handoff, `cargo clippy --workspace --all-targets -- -D warnings`,
`./scripts/check-comments.sh`, `./scripts/check-test-hygiene.sh`. Regenerate the spec rather
than hand-editing it: `UPDATE_OPENAPI=1 cargo nextest run --workspace -E 'binary(openapi_contract)'`
then `cd frontend && npm run gen:api`. Set `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-B5`.

**Handoff extras:** the fixture diff and the matching pin-table change, side by side; the
old-runner round trip as a named test; which of the two providers publishes which field,
measured from their real bodies rather than from their docs.

**Stop if:** the two providers' catalogs cannot fill one shape without a union type — that
was VI-B4's own stop condition and it did not fire, so if it fires now, say what changed.

---

## Wave 16 — UI-first flow

### VI-C3 — Project-level agent settings (start early; may branch from the Wave 14 SHA)

**Branch:** `agent/vi-c3-project-agent-settings`.

**Read (≈ 28k):**
- The board prelude (~7k); the ADR's vocabulary decision (~1k).
- The migration list and the last migration's shape: `sed -n '95,135p' crates/tack-db/src/migrations.rs`;
  `n=$(grep -n "MIGRATION_061" crates/tack-db/src/migrations.rs|head -1|cut -d: -f1); sed -n "${n},$((n+25))p" crates/tack-db/src/migrations.rs`.
  **One `ALTER`, one migration name, number 062.**
- `rg -n "pub struct Project\b" -A 30 crates/tack-core/src/models.rs`;
  `rg -n "pub async fn|vocabulary|workflow" crates/tack-db/src/repo/projects.rs | head -30`;
  `rg -n "UpdateProject|pub async fn update" -A 25 crates/tack-api/src/handlers/projects.rs | head -80`;
  `rg -n "UpdateProject" crates/ --type rust -l` — every constructor must gain the field.
- `crates/tack-orch/src/model_policy/wiring.rs` whole (207 l, ~2.5k) — you replace the
  always-`None` tier; `rg -n "^async fn|^fn|#\[tokio::test\]" -A1 crates/tack-orch/tests/model_policy_test.rs | head -40`
  then one test body as the pattern.
- `frontend/src/features/settings/ProjectSettings.tsx` whole (165 l) and one existing tab
  (`rg -ln "general" frontend/src/features/settings | head -2`);
  `AgentProfilesPanel.tsx` whole (170 l); `FleetsPanel.tsx` whole (173 l);
  `rg -n "fleet|member" frontend/src/shared/execution/api.ts | head -20`.

**Do not read:** `migrations.rs` whole (1,614 l); `select.rs`; any adapter; `RunWithAgentModal.tsx`
(C2's).

**Gate:** `cargo nextest run --workspace -E 'package(tack-db)'` (fresh DB **and** an upgraded copy of a beta.7 `tack.db` —
say where you got it), `-p tack-orch` (the new project-tier test), `-p tack-api`
(regenerate `openapi.json`/`schema.gen.ts`), `/gate frontend`, the settings E2E. The
fleet-member proof goes through the route, not the database.

**Handoff extras:** surface-map delta (the "choose a default model" and "add a member"
rows); the migration's `run_all` transcripts; the list of `UpdateProject` constructors
touched.

**Stop if:** the JSON blob cannot be validated against a typed struct at write time
without a second validation path at read time. State it; do not tolerate a malformed
blob at read.

### VI-C4 — List what an attempt produced (independent; may branch from the Wave 14 SHA)

**Branch:** `agent/vi-c4-attempt-lists`.

**Read (≈ 18k):**
- The board prelude (~7k).
- The concrete request from the card that found the gap: `sed -n '335,375p' docs/agent-handoffs/part-iii/III-F4.md`
  (~0.7k). Do not read the rest of that 472-line file.
- `rg -ln "execution_artifacts|execution_decisions" crates/tack-api/src/handlers crates/tack-db/src/repo`,
  then `rg -n "pub async fn" <those files> | head -40`; `rg -n "/executions" crates/tack-api/src/router.rs`.
- `frontend/src/shared/runWithAgent/DecisionInbox.tsx` whole (337 l, ~4k);
  `ArtifactDownloadPanel.tsx` whole (124 l); `frontend/src/features/item-detail/tabs/AgentActivityTab.tsx`
  whole (225 l, ~2.7k); `rg -n "artifact|decision" frontend/src/shared/execution/api.ts`.
- `frontend/e2e/execution-attempt-detail.spec.ts` whole — you extend it.

**Do not read:** runner-protocol handlers (`handlers/runner_protocol*.rs` — you add
operator routes only); any adapter; the modal.

**Gate:** `cargo nextest run --workspace -E 'package(tack-api)'` (regenerate the spec), `-E 'binary(runner_contract)'`
byte-identical, `/gate frontend`, the extended E2E with its byte-equality download proof
intact.

**Handoff extras:** surface-map delta (the last row); the route shapes as shipped versus
III-F4's request, with any difference justified.

### VI-C1 — The Agents page (after B2 and B3)

**Branch:** `agent/vi-c1-agents-page`.

**Read (≈ 18k):**
- The board prelude (~7k); `VI-B3.md` (the routes and panels you compose, ~3k);
  from `VI-B2.md` only its measured vendor table (`grep -n -A 12 -i "measured vendor" docs/agent-handoffs/part-vi/VI-B2.md`).
- `sed -n '1,130p;225,260p' frontend/src/shared/runWithAgent/shared.ts` (capability
  helpers and the gate — reuse, do not fork);
  `rg -n "RunnerCapabilities|HarnessCapability|interface RunnerSummary" -A 15 frontend/src/shared/execution/types.ts | head -70`.
- `frontend/src/features/fleet/runnerFleet/RunnerFleetSection.tsx` whole (51 l);
  `rg -n "RunnerFleetSection" frontend/src/features/fleet/FleetPage.tsx` — the one line you
  move; `head -40` of `EnrollmentPanel.tsx` and `RunnerHealthCard.tsx` (props only).
- `frontend/src/app/routes.tsx` whole (55 l); `rg -n -i "fleet" -B2 -A6 frontend/src/shared/ui/Sidebar.tsx`.
- `rg -n "RunWithAgentButton|capabilities" frontend/src/features/board/Board.tsx | head`
  — where the first-run banner mounts.
- `head -60 frontend/e2e/run-with-agent.spec.ts`; `head -60 frontend/e2e/helpers.ts`.
- The command strings the page must match: `sed -n '85,110p' docs/CONFIG.md`.

**Do not read:** `FleetPage.tsx` beyond the grep (the rest is docket); `RunWithAgentModal.tsx`
(C2's); any crate.

**Gate:** `/gate frontend` + the Playwright journeys the card names (`make e2e` scoped).
The vocabulary test is proven load-bearing by putting "runner" on the default screen once.

**Handoff extras:** vocabulary check (the grep, its hits, and why each is under
*Advanced*); surface-map delta (every row the page renders, reached or not); the exported
constant's path for VI-D1.

**Stop if:** a status cannot be derived from an API observation. Render "unknown" with the
reason — never a check mark from a file's existence, never a hard-coded string.

### VI-C2 — Run with agent: zero hand-typed identifiers (after C3)

**Branch:** `agent/vi-c2-modal-defaults`.

**Read (≈ 20k):**
- The board prelude (~7k); `VI-C3.md` (the settings shape and its client, ~3k).
- `frontend/src/shared/runWithAgent/RunWithAgentModal.tsx` whole (470 l, ~5.5k) and
  `shared.ts` whole (334 l, ~4k) — you rework the first and must not change the gate
  helper in the second; `head -80 RunWithAgentModal.test.tsx`.
- `rg -n "export const|fleets|runners|agentProfiles" frontend/src/shared/execution/api.ts | head -30`;
  `rg -n "export" frontend/src/shared/execution/store.ts | head` (the chip's source).
- `rg -n "RunWithAgentButton" -B5 -A10 frontend/src/features/board/Board.tsx` — the chip's
  placement.
- `head -120 frontend/e2e/run-with-agent.spec.ts`.

**Do not read:** any crate; the Agents page (C1's); other handoffs.

**Gate:** `/gate frontend`, then the E2E that intercepts `POST /api/executions` and asserts
the body equals the project defaults field for field. `shared.ts`'s gate helper must be
byte-identical (`git diff` it in the handoff).

**Handoff extras:** vocabulary check; surface-map delta (the "run an item" row); whether
`base_revision` accepts a ref — measured against the runner's checkout, with the command.

---

## Wave 17 — Proof

### VI-C5 — Handler tests for VI-C4's two attempt-list routes (needs nothing)

**Branch:** `agent/vi-c5-attempt-list-tests` from the `develop` tip you are given.

**Read (≈ 10k):**
- The board prelude's surface map only (`§VI.0`) plus your card.
- The amendment that created this card:
  `sed -n '/^## Amendments/,$p' docs/agent-handoffs/part-vi/VI-C4.md`.
- The two handlers themselves — find them first
  (`grep -rn "attempts/{attempt\|artifacts\|decisions" crates/tack-api/src/router.rs`), then
  read only those two functions. **The order they guarantee comes from the handler's own
  query, never from what the frontend renders.**
- One neighbouring test file as the pattern to follow, and its module wiring
  (`ls crates/tack-api/tests/`, then the `mod` list of the integration test that owns them).

**Do not read:** the frontend, `docs/openapi.json`, any runner-protocol file, any other
handoff.

**Gate:** `cargo nextest run --workspace` green; the same run with each new assertion's
behaviour reverted once, showing exactly that test fail;
`cargo clippy --workspace --all-targets -- -D warnings`; `./scripts/check-comments.sh`;
`./scripts/check-test-hygiene.sh` (your temp paths come from a `tempfile` guard, never
`env::temp_dir().join(...)`). `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C5`.

**Handoff extras:** the revert-once proof per test, pasted; the ordering the handler
actually guarantees, cited by line.

**Stop if:** a route cannot be tested without changing production code — record what you
needed and hand it over rather than changing it.

### VI-C6 — The unknown-attempt cases VI-C5 declared (needs nothing)

**Branch:** `agent/vi-c6-unknown-attempt` from the `develop` tip you are given.

**Read (≈ 9k):**
- The board prelude's surface map only (`§VI.0`) plus your card.
- `crates/tack-api/tests/handlers/attempt_lists.rs` whole — VI-C5's six tests are the
  pattern, the harness and the file you extend.
- The gap section of `docs/agent-handoffs/part-vi/VI-C5.md`
  (`grep -n "404\|unknown" docs/agent-handoffs/part-vi/VI-C5.md`, then that range).
- The two handlers themselves, found with one grep over `crates/tack-api/src/router.rs`.

**Do not read:** `docs/openapi.json`, the frontend, any runner-protocol file, any other
handoff. **The spec records intent; you are recording behaviour, and where they disagree
the handler wins and the disagreement is the finding.**

**Gate:** `cargo nextest run --workspace` green; the revert-once proof per new test;
`cargo clippy --workspace --all-targets -- -D warnings`; `./scripts/check-comments.sh`;
`./scripts/check-test-hygiene.sh`. `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C6`.

**Handoff extras:** `git diff --stat` showing nothing outside your test file and the
handoff; what a caller would wrongly conclude, if either route answers `200` for an
attempt that never existed.

**Stop if:** the honest answer needs a production change. Record it and hand it over.

### VI-C7 — Re-measure the Codex dispatch stall (needs nothing; changes no file)

**Branch:** `agent/vi-c7-codex-dispatch` from the `develop` tip you are given.

**Read (≈ 10k):**
- The board prelude's surface map only (`§VI.0`) plus your card.
- The escalation *and its amendment* in `docs/agent-handoffs/part-v/V-C2.md` — the
  amendment is the point: it says what that escalation was measured against and what had
  already stopped being true.
- `frontend/src/shared/runWithAgent/RunWithAgentModal.tsx` — what the dialog sends.
- The claim route and its filter, found with one grep for `\.route(` in
  `crates/tack-api/src/handlers/runner_protocol.rs`.

**Do not read:** the whole runner crate, the whole orch crate, any other card's handoff.
Follow one request through the hops; do not audit the system.

**Live rules — another card is recording video against this same tree while you work:**
your own port (`TACK_PORT=3312`), your own database under your worktree
(**never** the repository's `tack.db`, which is real data), your own runner state dir. Kill
what you start. `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C7`.

**Gate:** none in code — you change no file but your handoff. The gate is evidence: ids,
timestamps, and the command that shows each one.

**Handoff extras:** the one-line verdict, first. If it reproduces, the component that
stops and the smallest fix **as a diff in the handoff, not applied**. If it does not, the
exact conditions under which it does not.

**Stop if:** it needs a credential you do not have, or a runner change. Both are findings.

### VI-D2 — Assets that show the execution plane (after C1, C2 and Part V's V-C2)

**Branch:** `agent/vi-d2-assets`.

**Read (≈ 8k):**
- The board prelude's statement and surface map only (`§VI.0`, ~2.5k) plus your card.
- `docs/agent-handoffs/part-v/V-C2.md` — the recording tooling section only (grep for
  "record" and read that range); `frontend/e2e/hero-gif.spec.ts` and `screenshots.spec.ts`
  whole (the existing recording harness).
- `grep -n "screenshots/\|diagrams/" README.md` — the markup you replace.
- The *Next step* lines of `VI-C1.md` and `VI-C2.md`.

**Do not read:** anything else. This card records; it does not investigate.

**Gate:** none in code. Every asset from a release build (`cargo build --release` once, in
your own target dir — it is slow), a real agent, a machine named in the handoff; frame
count and file size measured; the request id of the completed run recorded.

**Handoff extras:** vocabulary check on alt texts; the measured sizes; V-C2's slot
untouched (`git diff` shows no change to its lines).

**Unblocked 2026-09-06.** V-C1, V-C2, VI-C1 and VI-C2 are all on `develop`;
`docs/screenshots/recovery-demo.gif` is V-C2's and is not yours. **Live rules — other cards
are running servers on this same machine:** your own port (`TACK_PORT=3311`), your own
database under your worktree (**never** the repository's `tack.db`, which is real data),
your own runner state dir; kill what you start.
`CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-D2`. **If a dispatch of yours sits in
`queued` and never runs, that is VI-C7's card, in flight beside you** — record what you saw,
with ids and timestamps, switch harness or model mode, and keep recording. Do not debug it,
and do not let it stop the assets.

### VI-D1 — Prove the stranger's path, and make the docs match what shipped (last)

**Branch:** `agent/vi-d1-stranger-proof`.

**Read (≈ 22k):**
- The board prelude (~7k) — §VI.5's table is your checklist.
- From every `VI-*.md` in this directory, **only three sections**, extracted, not read:

  ```bash
  for f in docs/agent-handoffs/part-vi/VI-*.md; do echo "== $f"; \
    awk '/^## (What a stranger|Surface-map delta|Next step)/{p=1;print;next} /^## /{p=0} p' "$f"; done
  ```
- `scripts/smoke.sh` whole (538 l, ~7k) — you add steps 13–14 in its own idiom.
- `docs/book/src/user-guide/quick-start.md` whole; the two sections VI-A1 added to
  `agent-runners.md` (grep their headings, read those ranges); the README's "Run it"
  section (`grep -n "^## Run it" README.md`, then that range).
- `docs/CONFIG.md` whole (~2k).

**Do not read:** any handoff whole; any crate; the frontend beyond the exported constant
VI-C1 named.

**Gate:** `./scripts/smoke.sh` green with steps 13–14 proven by injected failure;
`/gate docs`; `/gate full` once, as the integrator will run it. The stranger transcript is
produced on a clean container with the release binary.

**Handoff extras:** the surface map with every Target marked reached / not reached and
why; the re-measured §VI.0 evidence table; the stranger transcript; vocabulary check on
the docs you amended.

---

## Integrator checklist, per wave

The integrator is the dispatcher (Opus). It reads escalations, not handoffs — `/integrate`
§3's grep — plus the checks below, which are the adversarial verification §III.2 rule 14
requires. Merge order is the dependency order in the table at the top.

**Wave 14.** Run VI-A1's `tack execution create` example yourself against a fresh
`tack serve --with-runner`; `grep -rn claude_code docs/book/` is empty; `mdbook build` is
clean. VI-A2: record the user's acceptance as a dated amendment in `VI-A2.md`; confirm the
`docs/CONFIG.md` sentence is gone. VI-A3: run the stranger-read test with a fresh Sonnet
given only the named line range; `diff` the statement across the four files; open the
three render PNGs. Update the status row with the integration SHA. Nothing to regenerate.

**Wave 15.** Merge B1, build `tack-runner`; merge B2, build; merge B3, regenerate
`openapi.json` and `schema.gen.ts`, commit them with the merge. Gate:
`cargo nextest run --workspace`, `runner_contract`
byte-identical (or the accepted revision, with its pin-table update), `wave2_gate`, then
`./scripts/smoke.sh` (existing steps must stay green). Adversarial: revert B1's resolver →
its test fails; after a gateway run, `sqlite3 tack.db .dump | grep -c <key>` = 0 and the
store file is `600`; start on `0.0.0.0` → every `local-runner` route is 404; restart
without the flag → the runner comes back. Record the live request ids B2 produced.

**Wave 16.** Merge C3, regenerate; merge C4, regenerate; merge C1; merge C2. Gate: the
Rust crates C3/C4 touched, `runner_contract` byte-identical, `/gate frontend`, `make e2e`.
Adversarial: put "runner" on the Agents default screen → C1's test fails; submit the modal
untouched → the intercepted body equals the project defaults; migration 062 on an
upgraded beta.7 database copy; `shared.ts`'s gate helper unchanged (`git diff`).

**Wave 17.** D2 after V-C2 has landed — check `git log -- docs/screenshots/`. D1 last.
Reproduce the stranger transcript yourself on a clean container; run smoke with the
injected failure for steps 13–14; walk §VI.5's table row by row and mark each; if a row
does not hold, name the single card that would close it rather than closing the Part.
Then the `/integrate` §7 close-out.

## The mistakes this Part is most likely to make

1. **Putting a provider key anywhere but the runner's store** — `app_meta` because backup
   secrets live there, a request's `environment` value because it is convenient, a log line
   because the name and the value were formatted together. Every card that touches a
   secret proves the *absence* with a positive control.
2. **A green status derived from a file existing.** A session file is "present,
   unverified". Only a completed run or a successful catalog call earns a check mark.
3. **Architecture words on a default screen.** "Runner" in a modal label, "fleet" in a
   dropdown, "harness" in a heading. C1's grep test exists because reviewers miss these.
4. **Splitting a feature into a mechanism card and a caller card.** C3 is wide on purpose.
   A column with no reader, a route with no panel, a store with no resolver — the
   scope-discipline failure this tree keeps repeating.
5. **Reading an adapter whole.** Three files, ~80k tokens, and the card needed sixty lines
   of one of them. The read lists above name the ranges; trust them, and record it if one
   is wrong.
6. **A README claim without a proof.** V-A4's claim → evidence rule did not end with Part V.
   A3 and D1 keep the table; a row without evidence is a claim to delete.
7. **A smoke step that cannot fail.** Steps 13–14 must be proven by breaking the feature
   once and watching `FAIL` — this repository has shipped a false green before.
