---
name: card
description: Pick up and deliver one card/task from this repo's active planning board (TODO.md) — token-lean cold start, correct branch, ownership rules, gates, handoff. Use when asked to "work card X", dispatch a wave, or continue whatever cycle is currently active.
---

# Work a board card

> **Report using `.claude/reporting-contract.md`.** Lead with the capability in plain language — what someone can or cannot do now — and keep file and function names in the technical-detail section at the end. Explain every blocker as: what is missing, what it was for, what it blocks.
> **If your card writes an ADR or anything asking the user to approve a decision, use `.claude/decision-contract.md`.** Lead with the ask in plain language — what you're deciding, why, and what stays blocked if nobody answers — before any citation, rejected-alternative, or file:line reference. Those go in a separate section below, for whoever implements or double-checks it. A decision doc that opens with a Status/Date block and dense cited prose, with the actual ask buried past line 20, fails this rule regardless of how accurate it is.
>
> **Budget your reading with `.claude/context-budget.md`** — `TODO.md` whole is ~199k tokens; one Part's board is ~15k and one card's cold start is ~7k. **If the cycle has a dispatch plan (`docs/agent-handoffs/<part>/README.md`), your card's block there is the read list — follow it instead of §1's generic recipe.**
> **Bound your scope with `.claude/scope-discipline.md`** — this tree's recurring defect is well-built mechanisms with no caller.

You are delivering ONE unit of planned work from `TODO.md`. Works for any cycle (Parts IV
and V today, whatever succeeds them tomorrow). The argument is the card/task id if given.

## 1. Discover the active cycle — never read TODO.md whole

`TODO.md` is ~12k lines. **Reading it whole costs ~199k tokens** — most of a context window,
for a file that is ~90% closed-cycle history. It is laid out so you never have to:

```bash
head -58 TODO.md            # header: which Parts are ACTIVE, and the archive rule. ~1k tokens.
grep -n "^# \|^## " TODO.md # the map. Do this before any sed.

# Your card's section (headings follow "### <ID> — <title>"):
n=$(grep -n "### <ID> " TODO.md | cut -d: -f1); sed -n "${n},$((n+75))p" TODO.md   # the longest card is 75 lines
```

**Current layout (since 2026-09-03):** active boards first — **Part VI** (agent onboarding
+ provider UX, §VI, ~940 lines), then **Part V** (adoption, §V), then **Part IV** (single-binary,
§IV, done) — in roughly the first 2000 lines. Below them, an `# Archive` divider, then Parts
I, II and III unchanged. The archive stays in this file because 234 Rust doc comments cite
its section numbers; do not propose moving it. **Extract one Part, never all three.**

**Two Parts are active at once and they share `README.md` and `docs/screenshots/**`.** Read
§VI.3 (which defers to §V.3 for the files Part V still owns) before branching a card in
either. They name which card goes first and which takes the merge.

### 1a. If the cycle has a dispatch plan, it replaces the recipe above

`docs/agent-handoffs/<part>/README.md` (Part VI has one) carries, per card: the exact read
list with measured sizes, what not to read, the gate command, the handoff extras, and the
stop conditions. **Read its header and your card's block, nothing else in it** (~2k
tokens), then read only what the block names. Its hard ceilings apply: cold start ≤ 25k
tokens, ≤ 120k at handoff, stop and hand off at ~150k. A file you opened and did not use
goes in the handoff's "Context spent" section — that is how the plan gets corrected for
the next agent. Never spawn a subagent from a card: it costs a full context, and every
block is sized for one agent.

From your Part's section, read (line-ranged, not whole-file): the **status board** row for
your wave, the **rules** section, the **shared-file ownership** table, and your card's
**Owns / Context / Tasks / Acceptance**.

Then read only the handoffs your card's `Context` (or its dispatch block) names. All
handoffs together are ~240k tokens; a single large one is ~15k. CLAUDE.md is **already in your context** — don't re-derive
architecture from source, and don't re-read it.

**Facts a card states as measured are not yours to re-derive.** The boards carry measured
evidence tables precisely so each agent does not re-measure them. If you think one is wrong,
check that one row and say so.

## 2. Durable rules (these outlive any cycle)

- **Branch** from the SHA the status board names as the current base, named
  `agent/<card-id-lowercase>-<slug>`. Integration branches are merged into by the wave's
  integrator card only.
- **Contract fixtures outrank code.** Wherever a frozen contract dir exists (e.g.
  `docs/contracts/`), fixtures are the authority over Rust/TS types. Disagreement is
  escalated in the handoff, not resolved by bending either side. A fixture edit and its
  pin-table update land in the same change.
- **Respect shared-file ownership.** If your change needs a file the ownership table
  assigns to another card, write the request into your handoff instead of editing it.
- **Generated files are never hand-edited** — find the regeneration command in CLAUDE.md
  or the gate skill and commit regenerated output together with its source change.
- Unsupported is typed, unknown is explicit, unmeasured is nullable. No `unimplemented!()`,
  no zero standing in for "unknown" — capability claims are load-bearing.
- Logs carry ids only (no credentials/prompts/env values), and tests assert the redaction.
- Never `git commit` without the user asking; never add Co-Authored-By/AI attribution.


### Branch discipline (before you write any code)

```bash
LINE=develop                                    # confirm against the board
git rev-list --count "$LINE..$(git branch --show-current)"   # must be 0 before you start
git switch -c agent/<card-id-lowercase>-<slug> "$LINE"
```

- **Branch from the integration line, never from "wherever I happen to be."** The board's
  recorded base SHA can be stale — if it does not match the line's tip, say so rather than
  silently using either.
- **A card branch holds that card's work and nothing else.** Tooling, skills, unrelated
  fixes and board edits belong on the integration line. If you notice you have mixed
  them, separate them into different commits before handing over — this has happened
  twice here and both times the tooling had to be moved afterwards.
- **Never merge the card branch yourself** unless you are the integrator, and never
  advance the integration line from a card branch.

## 3. Deliver, then gate

Implement against the card's **Acceptance** list — it is the specification, not a floor.
Test the claim itself: for "writes nothing / rejects before X" claims, assert the absence
directly (row counts, untouched checkpoint) and prove the test load-bearing by reverting the
fix once. Run `/gate <scope>` before declaring done, then `.githooks/pre-push` as the
last step — it is what actually blocks a push, and it catches what a scoped gate does
not: `cargo fmt` on both the workspace and `crates/tack-desktop`, and stale generated
files. A card that lands unformatted is invisible until someone tries to push.

**Before you write code, check yourself against `.claude/scope-discipline.md`.** The four
questions that catch most of the waste in this tree:

- Can you name the caller of the thing you are about to write? If not, it belongs to the card
  that has the caller.
- Is anything in your diff absent from the card's Acceptance? That is unreviewed scope the
  integrator did not agree to.
- Are you adding a trait with one implementor, or a flag whose off-state nothing exercises?
- Is the card ambiguous about whether X is included? Then it is not. Write the question into
  the handoff and stop — widening scope to resolve ambiguity is how a packaging card becomes
  a refactor.
- **Does any comment you wrote name this card, its wave, its phase, or `TODO.md`?** Delete
  it. Comments explain the code to a reader who has never seen the board; provenance belongs
  in the handoff and in `git log`. A 2026-08-30 sweep removed ~1,900 such lines — do not add
  the next one.

The counter-rule matters as much: doing *less* than the card asks is not discipline, it is an
unreported gap. No `unimplemented!()`, no silent fake success, no zero standing in for
unknown.

## 4. Handoff (mandatory)

Write `docs/agent-handoffs/<cycle-dir>/<ID>.md` from the cycle's `TEMPLATE.md` when one
exists (Part VI: `docs/agent-handoffs/part-vi/TEMPLATE.md`; Part VII: `part-vii/TEMPLATE.md`,
which points at Part VI's and adds three sections) — never open the Part III
archive to find the template; older cycles inline it in their rules section. Record what you believed, what you verified, what you escalated, and any
shared-file requests. Corrections are appended as amendments — the original claim stays;
the history of what was believed and later falsified is the point. Update the status
board row only if you are the integrator; otherwise propose the row text in the handoff.

## Final step — write the report

This is part of the card, not an afterthought. Full rules in
`.claude/reporting-contract.md`; the required shape is below and is not optional.

**Do not open with the branch, the commit, or "X is delivered".** That is housekeeping —
it goes at the very end. Open with what a person can now do.

```
## What this is about
One or two sentences. The feature in human terms. No file names, no ids, no modes.

## Where it stands
What works now that did not before. What still does not work. Plain sentences.
Evidence goes in Technical detail — not here.

## What is left
One short paragraph per item. Each answers, in order: what is missing, what it was
supposed to do, what it blocks — then who fixes it. Order: blocks the feature working
at all, then blocks the next piece of work, then can wait.
If you fixed something outside your ownership, say so here and name the integrator.

## Technical detail
Labelled items, ONE topic each. Never one dense paragraph listing five files.
  **Where the code lives** — files added or changed.
  **How <one mechanism> works** — one topic per item, repeat as needed.
  **What is blocking, technically** — exact type or file, and why.
  **Test results** — the numbers.
  **Not checked** — what was skipped, and why it is not covered.

## Next step
One sentence: what to do next, and the command if there is one.
Then, on its own line, the housekeeping: branch name and whether it is committed.
```

Two failure modes to check before sending: a plain-language section that contains an id,
a file path or a permission mode (move it down), and a technical section that is one
paragraph covering several topics (split it into labelled items).
