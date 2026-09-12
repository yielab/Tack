# Scope discipline — build the card, not the platform

This repository's recurring defect is not bugs. It is **mechanisms that were built well,
tested thoroughly, and then never called by anything.** Every rule here is derived from
something that actually happened in this tree, and each one names it.

## The evidence

- **`model_profiles`** (migration 043) is consulted by nothing. It has been a recorded
  standing finding since Phase 56 and is still listed as deferred in Phase 58's roadmap
  section. A table, a repository module and a UI surface, for zero callers.
- **The `decisions` path** — protocol routes, contract fixtures, a DecisionInbox, replay
  tables — is fully built, byte-pinned, and **has never been exercised**, because no harness
  in this tree ever asks a mid-run question. Correctly documented as a scope limit, but it
  was built before anything needed it.
- **Parts I and II built an entire control plane** — a `ControlPlane` trait, a reconciler, a
  health machine, 9 `orch_*`-prefixed tables plus `control_planes` (10 Docket-specific
  tables total) — against exactly one backend, then generalized it for backends that never
  arrived. Part III replaced the whole model. Both now coexist in the schema and the UI,
  and V-B2 exists to decide what to do about it.
- **234 Rust doc comments once cited `TODO.md` section numbers** — 0 today
  (`grep -rn "TODO\.md" crates --include='*.rs' | wc -l`). That is what abstraction costs
  here after the fact: for weeks the docket surface could not simply be deleted, because
  deleting it meant updating every citation. Every mechanism you add is a mechanism someone
  later has to pay to remove.

None of that came from carelessness. It came from building the general case before the
specific one existed.

## Rules

1. **No mechanism without a caller in the same change.** A column, a trait, a config flag, a
   route or a module that nothing invokes is not "ready for later" — it is unverified code
   with a maintenance cost and no test that can fail meaningfully. If the caller belongs to a
   different card, the mechanism belongs to that card too.
2. **`Acceptance` is the specification, not the floor.** A card is done when its Acceptance
   list is satisfied. Work beyond it is not generosity; it is unreviewed scope that the wave
   integrator did not agree to and cannot verify. If you believe the card is wrong, say so in
   the handoff and stop — that is rule 8 of §III.2 and it is there for this.
3. **Build for the second case, not the fifth.** One implementor needs no trait. Two need one
   only if they actually differ. Part III's neutral execution domain is the good example: it
   was written *after* three concrete harnesses existed and their differences were measured.
   The `ControlPlane` trait is the bad one: it was written for a second backend that never
   arrived.
4. **Prefer deleting the branch to adding a flag.** Every config flag is a permanent second
   code path that must be tested in both states forever. `TACK_*_ENABLE` gates exist for a
   specific reason — anything that deletes data or reaches the network — not as a general way
   to avoid deciding.
5. **Escalate ambiguity; never widen scope to resolve it.** If a card does not say whether X
   is included, the answer is that it is not. Write the question in the handoff. Widening is
   how a packaging card turns into a refactor.
6. **Do not opportunistically delete either.** Removal in this tree needs a decision record —
   see Part V §V.1 rule 3. Finding dead code is a handoff note, not a licence.
7. **Three files is a smell; six is an escalation.** A card whose diff touches many files
   usually contains two cards. Split it, or say in the handoff which second card you found.

## Signals you are overengineering, checkable before you write code

- You are adding a trait with one implementor, or a second one that exists only to prove the
  first is abstract.
- You are adding a schema column the card's Acceptance never mentions.
- You cannot name the caller of the thing you are about to write.
- You are writing a config flag whose off-state nothing will ever exercise.
- Your test asserts that a mechanism exists rather than that a behaviour changed.
- You are generalizing because "we'll need it for the next harness/backend/provider" — the
  exact sentence that produced two of the four items in The evidence above.

## Comments: explain the code, never the project's history

The same failure that produces callerless mechanisms produces callerless comments. A
2026-08-30 sweep found **~1,900 comment lines** across `crates/` that documented the
planning board rather than the code: card ids, wave numbers, who owned which file, what a
previous card's handoff had argued. `reconciler.rs` alone opened with a **220-line** module
doc, over half of which described the order in which cards A2, B1, B2 and B3 had landed.

That text costs a reader's attention and an agent's context on every single read, and it
rots the moment the cycle ends — the worst of it actively misleads, because it describes a
plan rather than the code in front of you.

### Write

- **What the code does**, when the name does not already say it.
- **Why a non-obvious choice was made**, when the alternative looks better than it is.
- **What will break if you change it** — invariants, ordering constraints, "this must not
  hold a write across an await".
- **What is not true yet** — a mechanism with no caller, an unwired column, a capability
  that is declared but unproven. State it as current fact.

### Do not write

- **Card, wave, phase or board references.** `TODO.md §1.4`, `card B3`, `III-F6`, `Wave 2`,
  `this card's file ownership`. The reader has the code, not the board. If a decision needs
  provenance, the ADR or the handoff is where it lives — and `git log` already answers "who
  changed this and when" better than a comment can.
- **Narratives of how the code got here.** "A2's original recipe had…", "the deviation B1
  made…", "F5's own handoff shipped this default as `true`". Describe the code as it is. If
  a past mistake is worth warning about, write the warning ("do not reintroduce a
  client-side cursor reconstruction — it drifts silently"), not the story.
- **Instructions addressed to a cycle that has ended.** "Extending this for Wave 2 (read
  before adding `poll_traces`)" outlived Wave 2 by months and still told readers to do work
  that was already done.
- **Restatements of the code.** `// increment the counter` above `counter += 1`.
- **Dates and attributions.** They are wrong within a release and `git blame` is exact.
- **Commented-out code.** Delete it; the history has it.

### The test

Read the comment as somebody who has never seen this repository's planning board. If it
still tells them something true and useful about the code, keep it. If it only makes sense
to someone who knows what card III-F6 was, it is archaeology — delete it, or rewrite it as
the durable rule it was standing in for.

Before adding a comment, ask which of the four "write" categories it falls in. If none, the
comment is decoration and the code should carry the meaning instead — usually via a better
name.

### How much

The right kind of comment can still be too long. Budgets, measured by
`scripts/maintainability.py check` and listed by `comment-worklist`:

| Block | Budget | What belongs there |
|---|---|---|
| `//!` preamble, production file | ≤ 30 lines | what the module owns, its invariants, what breaks if you change them |
| `//!` preamble, test file | ≤ 10 lines | what it proves, how to run it |
| One `///` block | ≤ 15 lines | what the item does when the name does not say, the non-obvious choice, the hazard |
| Comment share of a production file | ≤ 35 % | — |

Vendor behaviour at a version goes in the fixture directory's README; design rationale in
an ADR or the book; a transitional parking place is `docs/dev-notes/`, which Part IX
empties. The plan and the numbers: `docs/plans/human-maintainability.md` §3.

## Tests: one claim, one place, one size

A test written to convince a verifier and a test written for a maintainer look different.
The second is what this tree keeps (`docs/plans/human-maintainability.md` §2.2, measured by
`scripts/maintainability.py check --changed`):

- **One claim per test; variants are rows** of one table-driven test.
- **The name states the claim in ≤ 60 characters** — no articles, no narrative, no board
  vocabulary. `env_canary_is_redacted`, not
  `a_planted_canary_in_the_environment_never_survives_into_the_returned_outcome`.
- **A body is ≤ 60 lines** (target 40); longer setup is a fixture in `tests/common`.
- **An invariant is pinned in at most two layers** — the repository and one router-level
  test; contract fixtures pin the shape, not the behaviour.
- **A test file's preamble is ≤ 10 lines.** Why it exists is an ADR's job.
- **A trailing `#[cfg(test)] mod tests` over 150 lines moves to `<module>/tests.rs`.**
- **No fixed waits; no live test hiding behind an env-var early return** — those live under
  `tests/live/`, `#[ignore]`d.
- **Throwaway proof goes in `crates/*/tests/scratch_*.rs`** (gitignored). A card adds at most
  15 tests and 600 test lines; over that is a finding for the integrator.

## The counter-rule, so this is not read as an excuse

Under-building has its own failure mode here, and it is worse: **`unimplemented!()`, silent
fake success, a zero standing in for an unknown, or a capability claimed but not supported.**
Unsupported is typed, unknown is explicit, unmeasured is nullable. Doing less than the card
asks is not scope discipline — it is an unreported gap. The discipline is about *breadth*, not
about *rigour*.
