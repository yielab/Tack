# IX-<card> handoff

Use **`docs/agent-handoffs/part-vi/TEMPLATE.md`** as the body — *Claim → evidence*,
*Measured numbers*, *What a stranger still cannot do*, *Context spent*, *Amendments* all
apply. Skip Part VI's *Surface-map delta*, *Secret-path proof* and *Vocabulary check*: this
Part changes no surface and no secret path.

Add these three sections, in this order, before *Context spent*:

## Budget check

The verbatim output of `python3 scripts/maintainability.py check --changed` on the final
tree, and of `python3 scripts/maintainability.py measure --totals` before and after the
card. For an IX-M4 sub-card, also `measure crates/<crate>/tests/<binary>*` before and after.

## What was removed

One line per removed test, merged test family or moved comment block: what it was, and the
reason class from the plan — §2.2 rule number for a test (`2: name`, `1: variants → rows`,
`5: third layer`, `8: fixed wait`), §3 row for a comment (`preamble`, `doc block`,
`share`, `vendor → fixture README`, `design → ADR`). Nothing removed without a class.

## Re-baselined?

`no`, or `yes` with the list of files whose baseline entry went down and the command that
shows it (`git diff scripts/maintainability-baseline.json | grep '^[-+] '`). A baseline
entry that went **up** is a defect; say so and fix it before handing off.
