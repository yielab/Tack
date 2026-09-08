# Part VII handoff template

Use **`docs/agent-handoffs/part-vi/TEMPLATE.md`** as the body — every section there
applies (Claim → evidence, Measured numbers, What a stranger still cannot do, Context
spent, Amendments). Skip Part VI's *Surface-map delta*, *Secret-path proof* and
*Vocabulary check* unless your card touches those things (C2 does the vocabulary check).

Add these three sections, in this order, before *Context spent*:

## Platform measured

OS and version, desktop environment, `echo $XDG_SESSION_TYPE` (Wayland or X11), whether
an appindicator host is running, systemd version. One line each. Anything you did not
run on is `not_measured`, not assumed.

## Daemon proof

The exact sequence of §VII.1 rule 3 with timestamps: attempt started → window closed →
observation from a second shell (`curl /api/health`, the attempt's state) → window
reopened → the same state rendered. Commands and outputs verbatim.

## Process proof

`pgrep -af tack` before and after each lifecycle step you touched (launch, close, reopen,
quit; install, uninstall). No orphan after quit or uninstall; a hand-started server still
present after the app's quit in attach mode.
