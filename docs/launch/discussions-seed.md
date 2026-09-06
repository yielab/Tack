# GitHub Discussions — seed topics (draft, not posted)

Two starter topics, category-tagged for GitHub Discussions. Neither has been created.
Discussions is already enabled on the repository, and its real category set was checked
directly rather than guessed:

```bash
$ gh api repos/yielab/tack --jq .has_discussions
true
$ gh api graphql -f query='{ repository(owner:"yielab", name:"tack") {
    discussionCategories(first:20) { nodes { name slug } } } }'
# → Announcements, General, Ideas, Polls, Q&A, Show and tell
```

## Topic 1 — category: **Q&A**

**Title:** `What would make you actually switch to this?`

**Body:**

> Tack is genuinely new — zero outside users as of this post. Rather than guess what
> matters, I'd rather ask directly: if you're currently using Vibe Kanban, Crystal,
> Conductor, Sculptor, Emdash, or just plain Claude Code/Codex with no board at all —
> what's the one thing that would have to be true for you to try this instead?
>
> To save you re-deriving it: the honest gap list is in the README's "Known
> limitations" section — no accounts, no notifications, English-only, one contributor.
> If one of those is your blocker, say so; that's exactly the kind of thing that becomes
> a `good first issue` instead of staying a silent reason not to try it.

## Topic 2 — category: **Ideas**

**Title:** `Durable agent recovery — what would you do with fencing tokens and leases?`

**Body:**

> The recovery demo (`docs/screenshots/recovery-demo.gif`) shows the core mechanic:
> kill a runner mid-attempt, the board shows `needs_operator` instead of silently losing
> the run or duplicating it, and an operator makes an explicit call before it retries.
> That mechanic — a fencing token per attempt, a lease that expires, a replay table — is
> generic underneath the one workflow it's demoed on.
>
> Genuinely curious what else people would build on it: multi-runner failover policies?
> Budget-aware auto-pause? A different notification path when an attempt needs a
> decision (there isn't one today — see the SMTP `good first issue`)? This is the one
> piece of the architecture I'd most like outside pressure-testing on.

## Notes for whoever posts these

- Post Topic 1 first; it's the one every launch-post draft in `docs/launch/posts/`
  implicitly points readers toward (each draft says "what's missing" feedback is wanted
  more than reactions).
- Neither topic references this repository's internal planning board — consistent with
  the rest of this card's material.
