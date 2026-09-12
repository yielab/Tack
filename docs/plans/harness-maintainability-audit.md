# Harness integrations: a maintainability and scalability audit

Measured 2026-09-11 on `develop` at `1b9b7e6` plus uncommitted docs. Every number below
was produced by the script in [Appendix A](#appendix-a--the-measurement-script); re-run it
before quoting any of them. This document exists because two more harnesses are proposed
(docket, ADR 0066; opencode, ADR 0067) and the cost of adding one is currently set
by the shape of the two that exist.

**The finding in one sentence:** the harness adapters are not over-tested, they are
**implemented twice and therefore tested twice**, and each one is shaped in a way no
maintainer can hold in their head — a third and fourth copy of that shape would make the
runner the least maintainable crate in the workspace.

## 1. What the lines are made of

| File | Production code | Comments | Comment share | Test lines | Tests | Lines per test | Longest test |
|---|---|---|---|---|---|---|---|
| `harness/claude_code.rs` | 900 | 320 | 26 % | 1 743 | 37 | 31 | 77 |
| `harness/codex.rs` | 775 | 237 | 23 % | 1 159 | 27 | 27 | 61 |
| `harness/mod.rs` | 207 | 183 | 47 % | 778 | 13 | 32 | 127 |
| `tack-runner` crate, all of `src/` | 7 443 | 2 242 | 23 % | 11 253 | 255 | — | — |

Workspace context, so the crate is judged against its neighbours rather than in isolation:

| Scope | Production lines | Test lines | Ratio | Tests |
|---|---|---|---|---|
| All Rust crates | 61 846 | 74 014 | 1.20 | 1 504 (102 files under `tests/`) |
| `tack-runner` | 7 443 | 11 253 | 1.51 | 255 inline + 20 in `tests/` |
| The two adapters | 1 675 | 2 902 | 1.73 | 64 |
| Frontend | — | — | — | 831 unit in 91 files, 78 E2E in 16 specs |

The workspace-wide ratio of 1.2 is normal-to-high for a system with a wire protocol, 62
migrations, 97 routes and crash recovery, and the whole suite runs in about 15 s. The
adapters sit at 1.73 and are the outlier, together with a handful of test files elsewhere
(`tack-db/tests/repository/execution_repo.rs` is 4 322 lines on its own).

### 1.1 Layout after IX-M1 and IX-M2, same day

Part IX's first two cards landed after the table above was taken and changed *where* the
lines are without changing *what* they are. Re-measured with the repo's own tool
(`python3 scripts/maintainability.py measure crates/tack-runner/src/harness`):

| File | Production | Comments | Share | Tests | Longest test |
|---|---|---|---|---|---|
| `harness/claude_code.rs` | 903 | 257 | 22 % | — | — |
| `harness/claude_code/tests.rs` | — | — | — | 37 in 1 735 lines | 73 |
| `harness/codex.rs` | 778 | 167 | 17 % | — | — |
| `harness/codex/tests.rs` | — | — | — | 26 in 1 149 lines | 54 |
| `harness/` total | 3 591 | 872 | 24 % | 115 in 4 731 lines | ratio 1.32 |

The preambles now live in `harness/fixtures/<kind>/README.md`, which is where §2.4 said they
belonged; the live tests are `#[ignore]`d but still inside the unit modules, which IX-M5
moves. The duplication in §2.1 and §2.2 is untouched by either card — that is IX-M5's job.

## 2. Where the size actually comes from

### 2.1 The lifecycle is implemented in every adapter

Thirteen functions exist under the same name in both adapter files:

```
cancel · declared_capabilities · detect_version · discover · feature_capabilities
harness_kind · probe · reconcile · stage_run_log · start · validate · wait · with_providers
```

Of those, only `feature_capabilities`, the command-line construction inside `start`, and
the stream parsing inside `wait` are about the harness. Spawning with a cleared
environment, piping the prompt to stdin, staging the run log, escalating `SIGTERM` to
`SIGKILL`, recovering by pid, parsing a version string, and building a `Usage` with cost
unmeasured are the same job in both files, written twice. The largest production functions
in `claude_code.rs` — `start` at 128 lines, `validate` at 97, `wait` at 73 — are mostly
that shared job.

The shared primitives that *should* carry this already exist: `process.rs`,
`event_sink.rs`, `redact.rs`, `locate.rs`, and the `fake_harness.sh` fixture. The adapters
compose them and then re-implement the layer above them, rather than calling one layer.

### 2.2 Therefore the tests exist twice too

Twelve of `codex.rs`'s 27 tests are the same test as one in `claude_code.rs` — three with
identical names, nine with a name similarity above 0.6 (`difflib` ratio; see Appendix A).
Seven test helpers carry the same name in both files (`spec_with`, `test_secret_store`,
`journal_with_process`, `recorded_env_names`, `enabled_gateway_providers`, and the two
provider-spawn tests). These are not tests of Claude Code or of Codex; they are tests of
the lifecycle, written once per copy of the lifecycle.

### 2.3 The tests that remain have the wrong shape

- **Variants are written as separate functions.** Seven `validate_rejects_*` and five
  `reconcile_*` tests in one file are a table of cases expressed as twelve ~30-line
  functions. A reader has to open all twelve to learn what `validate` rejects.
- **Captured vendor output lives in code.** The real observed Claude Code transcript is a
  string returned by a helper (`real_success_stdout`). When the vendor changes its stream
  there is no file to re-capture and diff; there is a string literal to edit by hand.
- **Live tests are inside unit-test modules.** `claude_code.rs` holds three tests that
  invoke the real, billed `claude` binary, each 60–77 lines, each starting with
  `if env != "1" { return }`. They run (as no-ops) on every `nextest` invocation and are
  invisible in the file listing as anything other than ordinary tests.
- **Names narrate instead of naming.**
  `a_planted_canary_in_the_environment_never_survives_into_the_returned_outcome` is a
  sentence a card wrote to justify itself; `env_canary_is_redacted` is a name a
  maintainer can scan.

### 2.4 The comments are the wrong kind, not the wrong amount

23–26 % comment density is not itself a problem. The problem is what the comments are:
`claude_code.rs` opens with 71 lines of module documentation recording how one installed
version of the binary behaved — that `subtype` lies while `is_error` does not, that a
user settings file broke a run, that the Bash tool starts a new session. That knowledge is
valuable and it is about **the vendor at a version**, not about the code. Its home is the
README of the fixture directory for that version, next to the captures that prove it. A
doc comment on the code says what the function does and what breaks if it changes, in a
few lines, and points there.

## 3. What this costs at four harnesses

| | Copying today's shape | With a shared core |
|---|---|---|
| Production code per new harness | ≈ 850 lines | ≈ 250–350 (a descriptor plus an event grammar) |
| Tests per new harness | ≈ 30, of which ≈ 12 re-test the lifecycle | ≈ 10–15, all about that harness's grammar and fixtures |
| Lifecycle tests in the tree | four copies | one, in `harness/mod.rs`, run against the fake harness |
| A change to cancellation or `reconcile` | four files, four test sets | one file, one test set |
| A vendor changes its event stream | search 1 300 lines | one grammar file, plus re-capturing fixtures |
| Lines a maintainer must read to understand one harness | ≈ 3 000 | ≈ 700 |

The right-hand column is a target, not a measurement. It becomes a measurement when the
two existing adapters have been migrated — which is why that migration is the first card
below, not the last.

## 4. The shape that scales

Not a plugin framework and not a declarative DSL — one generic struct and one small trait,
justified by four real callers rather than by anticipation:

```
LocalProcessHarness<G: HarnessGrammar>      // owns: spawn, env clearing, stdin prompt,
                                            //       journal, redaction, cancel escalation,
                                            //       reconcile by pid, version parsing,
                                            //       usage with cost unmeasured
trait HarnessGrammar {
    fn descriptor(&self) -> &Descriptor;    // program name, install locations, version
                                            // command, tested-version range
    fn command(&self, spec: &ExecutionSpec) -> ProcessSpec;
    fn classify(&self, line: &str) -> Option<HarnessEvent>;
    fn outcome(&self, events: &[HarnessEvent], exit: ExitStatus) -> HarnessOutcome;
    fn capabilities(&self) -> FeatureCapabilities;
}
```

Per harness, the tree then holds three things:

1. **A descriptor** — data. Where the binary lives, how it reports its version, which
   version range the fixtures were captured against.
2. **A grammar** — the only genuinely harness-specific code: how its stream becomes
   `HarnessEvent`s and how those become a `HarnessOutcome`.
3. **Fixtures** — `fixtures/<kind>/<version>/*.jsonl`, captured from the real binary, each
   with a provenance line, parsed by the grammar's tests as files.

The tested-version range in the descriptor closes a gap the book already admits:
`installed_version` is "informational, never used to gate behavior today". Outside the
range, the probe reports `Advisory` with the reason — it never refuses, because
"untested" is not "unsupported", and unsupported is typed.

## 5. Rules that keep it small

These are the rules the reduction card applies to the two existing adapters and that every
later harness card is held to. Each is checkable by the script in Appendix A, so the
integrator verifies them the way `pre-push` verifies formatting.

1. **The lifecycle is tested once.** No adapter test asserts a property of the shared
   core — environment clearing, secret redaction, cancel escalation, reconcile by pid.
   Those tests live in `harness/mod.rs` and run against `fake_harness.sh`.
2. **One test per contract claim; variants are rows.** A family of `rejects_*` tests is
   one table-driven test whose cases fit on one screen.
3. **Vendor output is a file, never a string literal.** Fixtures carry a provenance line
   (captured, with the binary version, or constructed, with why).
4. **Anything that runs a real binary lives under `tests/live/`, marked `#[ignore]`.**
   Never inside a unit-test module behind an early return.
5. **A test name names the claim.** No articles, no narrative, no card vocabulary.
6. **Vendor-behaviour findings go in the fixture README**, not in a module preamble.
7. **A harness card has a budget:** at most 400 lines of production code and 15 tests for
   the harness itself, measured by Appendix A and recorded in the handoff. A card that
   needs more has found a gap in the shared core, which is a separate card.

## 6. What this means for the plans

**T0 is Part IX's card IX-M5 (Wave 30)**, and the board owns it — this document is its
specification, not a second copy of it. The card: extract `LocalProcessHarness` and
`HarnessGrammar`; migrate `claude_code` and `codex` onto them with **no behaviour change**,
using their current tests as the proof; move captured transcripts to fixture files; move
the live tests under `tests/live/`; then prune the migrated tests to the shape in §5. Exit
criteria, all measured by Appendix A and by `scripts/maintainability.py check`: zero
near-identical test pairs across adapters, each adapter under 400 lines of production
code, no live test under `src/`, the crash matrix and `harness/tests.rs` green.

The four-harness plan — docket (ADR 0066) and opencode (ADR 0067) as grammars on that
core — is `docs/plans/harnesses.md`. It touches no `tack-runner` file until IX-M5 lands;
the cost it expects per harness is the right-hand column of §3, which becomes a
measurement only once the two existing adapters have been migrated.

## 7. Why the code got this way, said plainly

Each adapter was written by a card that proved itself in isolation and never paid the cost
of reading what it left behind. That is how agent-written code fails: every card is
correct and the sum is unmaintainable. "Write fewer tests" is not a fix an agent can hold
to; a place where a test can live only once, and a per-card budget with a command behind
it, are.

---

## Appendix A — the measurement script

Run from the repository root. Every figure in this document comes from it.

```sh
python3 - <<'PY'
import re, glob, difflib
from collections import Counter

def split(path):
    L = open(path).read().split('\n')
    ts = next((i for i, l in enumerate(L) if l.startswith('#[cfg(test)]')), len(L))
    return L, ts

def categorize(seg):
    c = Counter()
    for l in seg:
        s = l.strip()
        c['blank' if not s else 'comment' if s.startswith('//') else 'code'] += 1
    return c

def tests(L, ts):
    out = []
    for i in range(ts, len(L)):
        m = re.match(r'\s*(?:async )?fn ([a-z_0-9]+)\(', L[i])
        if m and any(re.match(r'\s*#\[(tokio::)?test', L[j]) for j in range(max(0, i - 3), i)):
            j = i + 1
            while j < len(L) and not re.match(r'\s*(?:async )?fn |\s*#\[', L[j]): j += 1
            out.append((j - i, m.group(1)))
    return out

print(f"{'file':40s} {'code':>6s} {'cmnt':>6s} {'cmnt%':>6s} {'tests_l':>8s} {'#t':>4s} {'avg':>5s} {'max':>4s}")
for f in sorted(glob.glob('crates/tack-runner/src/**/*.rs', recursive=True)):
    L, ts = split(f); p = categorize(L[:ts]); t = categorize(L[ts:]); tt = tests(L, ts)
    if sum(p.values()) + sum(t.values()) < 250: continue
    pct = 100 * p['comment'] / max(1, p['code'] + p['comment'])
    avg = sum(n for n, _ in tt) / len(tt) if tt else 0
    print(f"{f:40s} {p['code']:6d} {p['comment']:6d} {pct:5.0f}% {sum(t.values()):8d} {len(tt):4d} {avg:5.1f} {max((n for n,_ in tt), default=0):4d}")

a = tests(*split('crates/tack-runner/src/harness/claude_code.rs'))
b = tests(*split('crates/tack-runner/src/harness/codex.rs'))
pairs = [(x, y) for _, x in a for _, y in b if difflib.SequenceMatcher(None, x, y).ratio() > 0.6]
print(f"\nnear-identical test pairs across adapters: {len(pairs)}")

prod = test = 0
for f in glob.glob('crates/**/*.rs', recursive=True):
    if '/target/' in f: continue
    L, ts = split(f)
    if '/tests/' in f: test += len(L)
    else: prod += ts; test += len(L) - ts
print(f"workspace: prod={prod} test={test} ratio={test/prod:.2f}")
PY
grep -rE '^\s*#\[(tokio::)?test' --include='*.rs' crates/ | wc -l          # Rust tests
grep -rE '^\s*(it|test)\(' --include='*.test.ts' --include='*.test.tsx' frontend/src | wc -l
```

Duplicated function names across the two adapters (§2.1):

```sh
for f in claude_code codex; do
  awk 'NR<'"$(grep -n '^#\[cfg(test)\]' crates/tack-runner/src/harness/$f.rs | head -1 | cut -d: -f1)"'' \
    crates/tack-runner/src/harness/$f.rs | grep -oE "^\s*(pub )?(async )?fn [a-z_0-9]+" | awk '{print $NF}' | sort -u > /tmp/fns_$f
done; comm -12 /tmp/fns_claude_code /tmp/fns_codex
```
