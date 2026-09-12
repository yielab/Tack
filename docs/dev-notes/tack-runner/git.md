# `crates/tack-runner/src/git.rs`

Moved out of the module preamble; trim or delete freely.

The real [`WorktreeProvisioner`]: a private, attempt-scoped git checkout.

 # Why a private clone and not `git worktree add`

 The trait is named `WorktreeProvisioner` because the *product* requirement
 is an isolated working tree per attempt, not because git's `worktree`
 subcommand must be the mechanism. Two facts rule that subcommand out here:

 1. `git worktree add` keeps administrative state (`worktrees/<name>`, a
    lock file and a `gitdir` pointer) inside one shared repository. Two
    attempts provisioning at once contend on that repository's index lock,
    and a runner killed mid-add leaves a registered-but-absent worktree that
    a later attempt inherits — exactly the two failure modes a private
    clone avoids. Recovering from it needs `git worktree prune` against shared
    mutable state that another live attempt may be using at that moment.
 2. `git worktree add` refuses a target directory that already exists and is
    not empty. Every attempt directory already carries the `.tack-attempt`
    marker that [`super::WorkspaceManager`] writes *before* provisioning, so
    the target is never empty by construction.

 `git init` in place has neither problem: it accepts a non-empty directory,
 every attempt owns 100% of its own repository state, and cleanup is a plain
 recursive delete of a directory this runner stamped — which
 [`super::WorkspaceManager::cleanup`] already implements and guards.

 # Restart safety

 Provisioning is not atomic — a checkout is thousands of files. A runner
 killed halfway leaves a directory that *looks* like a checkout but is not
 one. The completion sentinel [`CHECKOUT_MARKER`] is written (and fsynced)
 only after `checkout` returns, and it records the exact resolved commit. On
 a restart the provisioner either finds a sentinel that agrees with the live
 repository — and reuses the checkout — or it discards everything under the
 attempt directory and provisions again. A half-made checkout is therefore
 never inherited, by this attempt or any later one.

 # What never reaches a log

 A remote URL can embed credentials (`https://user:token@host/repo.git`) and
 a query string. Git echoes the remote back in most of its error messages,
 so raw git output is treated as tainted: it is scrubbed through
 [`SecretMaterial`] (seeded with the remote, its userinfo and its password)
 and [`redact_query`] before it can reach a tracing field, and the typed
 errors this module returns carry no remote, path or git text at all.
