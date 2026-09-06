# Hacker News — Show HN (draft, not posted)

**Format notes:** HN's submission form is title + URL + optional text; the text field
takes no markdown (no tables, no bold) — paragraphs and blank lines only, URLs
auto-link. Submit the URL as the GitHub repo (`https://github.com/yielab/tack`) and put
this text in the optional comment box, or submit as a text post with this as the body —
either is fine for a Show HN. Title is capped at 80 characters; the one below is 67
(`printf '...' | wc -c`).

## Title (67 chars)

```text
Show HN: Tack, a self-hosted PM that recovers a killed coding agent
```

## Post text

I built Tack because two tools in this space both had the ground shift under them this
year: Vibe Kanban's company (Bloop) shut down in April, and Crystal was deprecated in
February in favor of a different project (Nimbalyst). Neither is "dead" exactly — Vibe
Kanban went community-maintained, Crystal still runs — but both userbases got a "the
thing you built your workflow around is not what it was" moment, and I kept coming back
to the same question: where's a self-hosted project manager that also runs Claude Code
or Codex against your own repo, without asking you to trust anyone else's server with
your code or your model credentials.

So: Tack is a project manager (Kanban/Scrum/phase workflows, dependencies, timelines,
per-project vocabulary) and an agent execution fleet, in one Rust binary with an
embedded SQLite file. No accounts, no cloud, no Docker required (though there's a
Docker image if you want one). `curl | sh` then `tack serve --with-runner` and you have
a board that can dispatch a card to Claude Code or Codex running on your own machine,
with its own credentials, in an isolated workspace.

The part I actually think is interesting, and the reason I'm posting now instead of
waiting for it to feel "done": I couldn't find another tool in this space whose own
docs claim durable recovery from a killed agent process — most don't say either way.
Tack's runner protocol gives every attempt a fencing token and a lease. Kill the runner
mid-attempt and the board doesn't retry blindly — it shows `needs_operator`, with no
duplicate execution, and an operator has to make an explicit decision to requeue it.
There's a 35-second recording of the whole sequence — kill, no duplicate, explicit
requeue, success — made from an actual downloaded release binary running in Docker, not
a staged dev build. It's in the README's "Durable by design" section:
<https://github.com/yielab/tack>.

What it's not, so nobody has to find out the hard way: single shared bearer token, no
per-user accounts; no outbound notifications (no email, nothing — you have to look at
the board); English-only UI; one contributor (me); this has never had an outside user.
Full list is in the README's "Known limitations" section, and I'd rather list it there
than have someone hit it first.

10.3–19.3 MiB binary depending on build profile, ~13.6 MiB idle RSS, 1,420 tests, MIT
licensed. Would genuinely like the "what's missing" feedback more than the "cool"
feedback — I have a `good first issue` label seeded with real gaps (i18n scaffolding,
SMTP notifications, time tracking, in-UI diff review of what an agent changed) if
anyone wants to poke at the code instead of just the pitch.
