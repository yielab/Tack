# Project Governance

Tack is a **single-maintainer, BDFL-model** open-source project. It is maintained
by **Santiago Yie** (<info@yielab.com>), who acts as the Benevolent Dictator For
Life (BDFL) and has final say on the project's direction, scope, and releases.

This document describes how decisions are made and how that can change over time.
It is intentionally lightweight — the project is small and values shipping over
process.

## Roles

- **Maintainer (BDFL):** Santiago Yie. Owns the roadmap, reviews and merges pull
  requests, cuts releases, and makes the final call on any disputed decision.
- **Contributors:** anyone who opens an issue or a pull request. Contributions of
  code, docs, tests, triage, and design are all welcome and valued.

## Decision-Making

- **Everyday changes** (bug fixes, docs, tests, self-contained features) are
  decided through normal pull-request review. The maintainer reviews, requests
  changes, and merges.
- **Significant changes** (new subsystems, dependencies that affect the binary
  size budget, breaking API changes, changes to the project's scope or licensing)
  should start as a GitHub issue or discussion so the direction can be agreed
  before code is written. See the roadmap in
  [`docs/book/src/roadmap.md`](docs/book/src/roadmap.md) for the current
  direction and open questions.
- **Disagreements** are resolved by discussion in the open. If consensus is not
  reached, the maintainer makes the final decision. This is the "BD" part of
  BDFL — biased toward keeping the project small, focused, and coherent.

## Guiding Principles

Decisions are weighed against the project's core philosophy:

- **Single binary, single file.** One binary, one SQLite file, no runtime to install.
  Binary size is a budget, not a slogan: it is measured in
  [Benchmarks](docs/BENCHMARKS.md), and a change that moves it is expected to say so
  and to justify the move.
- **Universal work tracking with domain-specific vocabulary.** Features should
  serve the solo-developer / small-team use case across diverse domains.
- **Local-first and honest.** Prefer local, offline-capable behavior; keep docs
  and claims truthful to what the code actually does.

## Releases

The maintainer cuts releases by tagging `vX.Y.Z` (see `.github/workflows/release.yml`).
Release notes are drawn from `CHANGELOG.md`, generated from conventional commit
messages by git-cliff (`cliff.toml`, `make changelog-release`), and which follows
[Keep a Changelog](https://keepachangelog.com/) and Semantic Versioning.

## Becoming a Maintainer

The project is open to growing its maintainer set as it grows. A contributor may
be invited to become a maintainer (with merge and release rights) after a
sustained track record of:

- High-quality, reviewed contributions across several areas of the codebase.
- Helpful, respectful participation in issues and reviews (see the
  [Code of Conduct](CODE_OF_CONDUCT.md)).
- Good judgment aligned with the guiding principles above.

There is no fixed threshold; the maintainer extends an invitation when the trust
and need are both present. Prospective maintainers can express interest by
emailing <info@yielab.com> or noting it on a relevant issue.

## Code of Conduct

All participation is governed by the [Code of Conduct](CODE_OF_CONDUCT.md).
Conduct concerns are handled by the maintainer at <info@yielab.com>.
