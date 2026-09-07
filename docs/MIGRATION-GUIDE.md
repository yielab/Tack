# Migration Guide

This guide covers two things: how Tack's automatic schema migrations behave (safe by
default, with an automatic pre-upgrade snapshot before the one genuinely risky class
of migration), and how to safely enable the runner-fleet execution features added in
Part III on an existing deployment.

If you are only running Tack as a project manager and have never set
`TACK_ORCH_ENABLE` or created an execution request, upgrading is exactly one step:
**back up, then start the new binary.** Everything below the first section is opt-in.

---

## Upgrading the binary (routine)

1. **Back up first**, always — see [Backup and Restore](book/src/user-guide/backup-restore.md).
   Migrations run automatically on the next startup; there is no separate "run
   migrations" command to skip.

   ```sh
   tack backup --path /backups/tack-pre-upgrade.db
   ```

2. **Replace the binary** and start it. `tack_api::serve()` runs
   `migrations::run_all()` before accepting any request
   (`crates/tack-api/src/server.rs`).

3. **What happens automatically:**
   - Every already-applied migration is checked against the binary's own ordered
     list by **name and by a deterministic checksum** — if your database's history
     isn't an exact prefix of what this binary expects, or a previously-applied
     migration's definition changed, startup refuses to proceed rather than guess
     (`crates/tack-db/src/migrations.rs::verify_applied_migration_invariant`). This
     is what prevents silently running an edited or reordered migration history.
   - Every ordinary migration runs inside its own transaction; a failing statement
     rolls the whole migration back rather than leaving a half-applied schema.
   - A small number of migrations (037, 038 — a pre-Part-III `COPY`/verify/swap
     table rebuild) are a different, higher-risk kind: **before the first attempt at
     one of these**, Tack automatically takes a `VACUUM INTO` snapshot named
     `<your-db-file>.before-<migration-name>.sqlite`, next to your database file. It
     is never overwritten by a retry — the first pre-upgrade image is the recovery
     artifact. See `docs/adr/0008-transactional-migration-rebuild-recovery.md` for
     the full design rationale. If you're already past 038 (any Tack released after
     Part III began), this snapshot step is inert for future upgrades — it only
     fires again if a future rebuild-class migration is added.
   - The rebuild migrations themselves run with `PRAGMA defer_foreign_keys=ON` inside
     their transaction, compare row counts and an explicit bidirectional column
     projection between the old and new table before dropping the original, and run
     `PRAGMA foreign_key_check` before committing — a lossy or FK-violating copy
     never reaches a committed state.

4. **If startup refuses to proceed:** don't force it. The error names the mismatch
   (unexpected entry, out-of-order name, or changed checksum) and tells you to
   restore a database with an intact `_migrations` history. Restore your pre-upgrade
   backup, confirm you're running the intended binary version, and try again.

`GET /api/health` reports `migrations_applied` — read that field rather than a
hand-written count, which this document does not track. Migrations 039–048 added the
ten neutral runner-v1 execution tables; later ones refined execution replay, recovery
and attempt-start facts. None of them are destructive to data written by an earlier
version: every one of them is additive.

---

## Enabling the runner-fleet execution features

Nothing described on the [Agent Runners](book/src/user-guide/agent-runners.md) page
activates by touching a migration. All of it is env-var gated, off by default, and the
gates are independent of each other — enabling one does not enable the others.

| You want... | Set | Off-by-default because |
|---|---|---|
| The execution/runner/fleet API surface to exist at all (`POST /api/executions`, `/api/runners/*`, etc.) | Nothing — this surface has no `_ENABLE` gate; it's always mounted | Creating an execution request or enrolling a runner has no side effect on its own until something acts on it |
| Docket control-plane polling and dispatch (`/api/control-planes`, `/api/fleet`) | `TACK_ORCH_ENABLE=true` | Spawns a background reconciler task and adds a real, network-reaching agent-fleet backend to the picture |
| Resolving a scoped execution decision | `TACK_EXECUTION_DECISION_TOKEN=<secret>` | Fail-closed by design — with it unset, `POST .../decisions/{id}/resolve` rejects every request rather than falling back to the ordinary operator token |
| Granting/denying a Docket approval | `TACK_ORCH_APPROVAL_TOKEN=<secret>` | Same fail-closed posture, mirrored exactly for the Docket approval surface |
| Automatic deletion of old execution history, replay bookkeeping, and artifact blobs | `TACK_EXECUTION_RETENTION_ENABLE=true` | **Deletes rows and on-disk blobs.** Off by default deliberately: data deletion is an explicit operator opt-in, matching `TACK_ORCH_ENABLE`'s posture |
| The read-only execution health watch (logs a `warn!` on stale-lease/`needs_operator` onset) | Nothing to do — `TACK_EXECUTION_HEALTH_ENABLE=true` is the default | Reads and logs only; no data is deleted or sent anywhere |

**Recommended enablement order for a first rollout:**

1. Upgrade the binary with everything above still off. Confirm `/api/health` and your
   existing board still work exactly as before — the new tables exist but nothing
   reads or writes them yet.
2. If you plan to resolve execution decisions from the UI, set
   `TACK_EXECUTION_DECISION_TOKEN` before anyone needs to use that feature — it's
   fail-closed, so forgetting it just means decisions can't be resolved yet, not an
   open door.
3. Only enable `TACK_EXECUTION_RETENTION_ENABLE` once you've decided how long you
   want execution history and artifact blobs to live
   (`TACK_EXECUTION_RETENTION_DAYS`, default 90) — this is the one setting on this
   page that deletes data.
4. Enable `TACK_ORCH_ENABLE` only if you're actually running Docket. It's entirely
   optional; runner-fleet execution (Codex/Claude Code via `tack-runner`)
   does not need it and vice versa — see
   [Docket compatibility](book/src/user-guide/agent-runners.md#docket-compatibility).

Full variable reference, defaults, and cross-references: `docs/CONFIG.md`.

---

## Downgrading

Migrations are forward-only. There is no `tack migrate down`. If you need to run an
older Tack binary against a database that has already been migrated forward, restore
the pre-upgrade backup you took in step 1 above — that is the only supported path
back. Running an old binary directly against a newer schema is not supported and will
fail the same checksum/order invariant described above (the old binary's migration
list won't be a prefix match for the newer database's history).

---

## See also

- [Backup and Restore](book/src/user-guide/backup-restore.md) — routine snapshot and
  staged-restore mechanics, remote/cloud backup, and what's included in each.
- [Agent Runners & Fleet Execution](book/src/user-guide/agent-runners.md) — what each
  gate above actually turns on, and the capability/security posture of the runner
  fleet.
- [Recovery Runbook](book/src/user-guide/recovery-runbook.md) — attempt-level
  recovery (a crashed runner or harness process), a different concern from schema
  migration recovery.
- `docs/CONFIG.md` — the authoritative table of every `TACK_*` variable.
- `docs/adr/0008-transactional-migration-rebuild-recovery.md` — the design rationale
  for the rebuild-migration snapshot/verify/swap mechanism.
