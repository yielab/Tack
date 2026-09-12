---
name: integrate
description: Merge finished card branches into the integration line and review them as ONE combined change — conflict scan, unified diff summary, escalations collected from handoffs, gates run once. Use when several card branches are done and waiting, or when asked to integrate a wave, land branches, or do an integrator card (e.g. III-G5).
---

# Integrate finished branches (solo-friendly review)

> **Report using `.claude/reporting-contract.md`.** Lead with the capability in plain language — what someone can or cannot do now — and keep file and function names in the technical-detail section at the end. Explain every blocker as: what is missing, what it was for, what it blocks.
>
> **Budget your reading with `.claude/context-budget.md`** — `TODO.md` whole is ~199k tokens,
> the three active boards ~33k (one Part ~15k; one card's cold start ~7k), all handoffs together ~240k. Grep before you read; read a range
> before a file. CLAUDE.md is already in your context — don't re-derive or re-read it.
> **Bound your scope with `.claude/scope-discipline.md`** — this tree's recurring defect is
> well-built mechanisms with no caller.


**Do not review branch by branch.** Each card already justified itself in its handoff and
ran its own tests. Reviewing N branches separately re-does that work N times and still
misses the only thing that matters: whether they are correct *together*. Review once, on
the merged tree.


## 0. Merge INTO the integration line, and prove it afterwards

```bash
LINE=develop                                    # confirm against the board
git switch "$LINE"                              # merge target is the line, never a card branch
# ... merges happen here ...
# afterwards, nothing may be ahead of the line except unmerged cards you chose to leave:
git for-each-ref --format='%(refname:short)' refs/heads | while read b; do
  n=$(git cherry "$LINE" "$b" 2>/dev/null | grep -c '^+')   # content, not ancestry
  [ "${n:-0}" -gt 0 ] && printf 'still unlanded: %-42s %s\n' "$b" "$n"
done
git show-ref --verify --quiet "refs/remotes/origin/$LINE" \
  && git rev-list --count "origin/$LINE..$LINE" | xargs echo "unpushed:" \
  || echo "the line is not on the remote at all"
```

If a card branch is ahead of the line after integration and it was not deliberately left
out, the line did not actually advance — you merged into the wrong place. Fix it before
reporting, and say what happened.

Report the unpushed count in the summary. Pushing is the user's call, but silently
leaving a whole wave on one machine is not.

## 1. Establish the set

```bash
# Which branches actually hold unlanded work (patch-equivalence, not ancestry):
for b in $(git branch --no-merged HEAD | sed 's/^..//'); do
  n=$(git cherry HEAD "$b" | grep -c '^+'); [ "$n" -gt 0 ] && echo "$b  ($n commits)"
done
```
Confirm the set against the active wave's board row. A branch with unlanded work that the
board doesn't mention is a finding — surface it, don't silently merge it.

## 2. Predict conflicts BEFORE merging

```bash
# Files touched by more than one branch = the whole risk surface
for b in <branches>; do git diff --name-only HEAD...$b; done | sort | uniq -d
```
Any file in that list is either a legitimate shared-file handoff request or an ownership
violation. Read what each branch did to it before merging, and check the wave's
shared-file ownership table.

## 3. Read the escalations, not the diffs

Each branch carries its own handoff. Pull just the parts that need a decision:

```bash
for b in <branches>; do
  git show $b:docs/agent-handoffs/<cycle>/<CARD>.md \
    | grep -n -iE "escalat|blocked|cannot|could not|unverified|requested|finding|limitation"
done
```
Every escalation gets an explicit outcome: **accepted**, **routed to a card**, or
**rejected with a reason**. An escalation that is silently merged is how a known problem
becomes an unknown one.

## 4. Merge in dependency order, one at a time

Merge the branch that others depend on first (docs and CI usually last, since they
describe what the others did). After each merge, only check that the tree still builds —
save the full gate set for the end.

## 4a. Run the cycle's integrator checklist, if it has one

`docs/agent-handoffs/<part>/README.md` may carry a per-wave **Integrator checklist** (Part
VI and Part VII do: `sed -n '/^## Integrator checklist/,/^## The mistakes/p' docs/agent-handoffs/part-vi/README.md`, same for `part-vii`).
It names the adversarial checks the wave must survive — revert-the-fix proofs, absence
assertions with positive controls, the 404-on-non-loopback check, the stranger-read test.
Those are §III.2 rule 14's "verification by someone who did not author the code"; the
cards' own green tests do not substitute for them. Regenerate `docs/openapi.json` and
`frontend/src/shared/api/schema.gen.ts` after **each** merge that touched a handler, and
commit them with that merge — never hand-edit, never batch three merges' drift into one.

## 5. Gate once, on the integrated tree

Run `/gate full`. This is the only test result that counts: a card's own green tests are
not evidence that the integrated system works — that is why wave-gate tests exist and
import no per-card test infrastructure. Then `python3 scripts/maintainability.py check` on
the integrated tree: the workspace test : production ratio must not have grown, and each
handoff's *Budget check* must match what you measure. A baseline entry that went up in
`scripts/maintainability-baseline.json` is a finding, collected like an escalation.

## 6. Report as one review

Produce a single summary: what landed, what conflicted and how it was resolved, every
escalation with its outcome, gate results with numbers, and what remains open. That
summary — not N branch reviews — is the artifact worth keeping.

## 7. Close out the wave — say what is next and what must be reviewed

An integration is not finished when the merge is clean. End every run by answering these,
in the report:

1. **Is the wave complete?** Update the active wave's board row with: the integration SHA,
   gate numbers, every escalation's outcome, and any acceptance bar NOT met. A wave that
   merged cleanly but left a P0 open is *integrated*, not *complete* — say both words.
2. **What is the next wave?** Read the cycle's dependency graph and board for the next
   row. Name its cards, which may run in parallel, which must run last, and the exact base
   SHA (= the integration SHA you just produced).
3. **If there is no next wave**, check the cycle's *definition of done* line by line
   against the tree. Do not assume the last wave means the cycle is over — report which
   criteria hold and which do not, and what single card would close the gap.
4. **What needs human review, and how.** List only decisions a person must make, each with
   the command or file that shows the evidence:
   - accepted-but-unverified claims (say what was NOT proven, and why)
   - routed-open findings with owners and severity
   - anything that would ship broken if tagged today
   Rank by what blocks a release, then by what blocks the next wave, then by the rest.

Never close a wave by editing a gate's test to make it pass, and never let an escalation
merge without an outcome — those are the two ways a green board stops meaning anything.

## Then clean up

```bash
git worktree list           # remove worktrees for merged cards
git worktree prune
```
Deleting merged branches is the user's call — propose, don't execute. Verify a branch's
work is truly in HEAD by patch-equivalence (`git cherry`) before ever suggesting deletion.

## Final step — write the report

This is part of the integration, not an afterthought. Full rules in
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
  **Budget check** — `scripts/maintainability.py check` on the integrated tree.
  **Not checked** — what was skipped, and why it is not covered.

## Next step
One sentence: what to do next, and the command if there is one.
Then, on its own line, the housekeeping: branch name and whether it is committed.
```

Two failure modes to check before sending: a plain-language section that contains an id,
a file path or a permission mode (move it down), and a technical section that is one
paragraph covering several topics (split it into labelled items).
