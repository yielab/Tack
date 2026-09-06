# r/selfhosted (draft, not posted)

**Format notes:** r/selfhosted's markdown editor supports tables, headers, and code
blocks. Check the sub's current self-promotion/"my project" flair rules before posting
(they change); this community is also fast to downvote anything that reads like a
press release, so the honest-limitations section matters here as much as on HN.

## Title

```text
Tack — self-hosted project manager that also runs Claude Code/Codex against your own
repo, with crash-safe agent execution (single binary, SQLite, MIT)
```

## Post body

**What it is:** a project manager (Kanban/Scrum/phase workflows, dependencies,
timelines, calendar, per-project vocabulary) that can also dispatch a board item to an
AI coding agent — Claude Code or Codex — running on your own machine with your own
credentials. One `tack` binary, one SQLite file. No accounts system, no telemetry, no
required external service.

**Why post here specifically:** this sub cares about footprint and data ownership more
than feature lists, so here are the actual numbers, not marketing copy:

| | Measured value | How |
| --- | --- | --- |
| Release binary (static, UI embedded) | 19.3 MiB | downloaded the real GitHub Release archive, `ls -l` |
| Idle memory (RSS) | ~13.6 MiB | `awk '/VmRSS/{...}' /proc/<pid>/status`, 1s after ready |
| Cold start | ~113 ms | wall-clock to first `/api/health` 200 |
| Dependencies at runtime | none | no Postgres, no Redis, no Docker required — SQLite and the web UI are embedded in the one binary |
| Data location | `tack.db` + `storage/` in the working directory | back up both, that's the whole database |

Install:

```bash
curl -fsSL https://raw.githubusercontent.com/yielab/tack/main/install.sh | sh
tack serve --with-runner
```

`--with-runner` embeds an agent runner in the same process (off by default, loopback
only). Plain `tack serve` is just the board with no agent execution at all, if that's
all you want. A Docker image and `cargo-binstall` support also exist if you'd rather not
run a shell script off the internet — see the repo's `packaging/` directory.

**The actual reason I think this is worth your time over the other options in this
space:** I couldn't find another tool here whose own docs claim durable recovery from a
killed agent process — most just don't say. Tack's runner protocol gives every attempt
a fencing token and a lease — kill it mid-run and the board shows `needs_operator`, no
duplicate, and waits for an explicit operator decision before it retries. There's a
recording of the full sequence (kill → needs_operator → no duplicate → operator requeue
→ success) made from a real downloaded release binary in Docker, linked in the README.

**What's not here, stated up front instead of found the hard way:**

- No accounts — one optional shared bearer token, no per-user permissions
- No outbound notifications of any kind (no email, nothing) — you check the board
- English-only UI
- One contributor, zero outside users so far — this is a genuinely new project
- Backup is snapshot replication (S3-compatible), not live multi-writer sync — one
  SQLite writer at a time
- Not code-signed yet (right-click-Open on macOS, SmartScreen click-through on Windows)
- No native mobile app

MIT licensed. Repo: <https://github.com/yielab/tack>. Genuinely interested in
self-hosting-specific feedback — backup/restore behavior, reverse-proxy setups,
anything that breaks on a NAS or a low-power box.
