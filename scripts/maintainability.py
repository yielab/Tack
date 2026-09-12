#!/usr/bin/env python3
"""Measures and enforces the size budgets that keep this tree readable by a person.

Subcommands (run from the repository root):

  measure   [paths]        per-file table: production, comment and test lines, test
                           count and shape. --json for machines, --totals for one line.
  baseline                 write scripts/maintainability-baseline.json from the tree.
  check     [--changed]    fail when a file breaks a budget AND is worse than the
                           baseline (ratchet), or when the workspace test:prod ratio
                           grew. --changed scans only files git sees as modified.
  extract-tests [paths]    move a file's trailing `#[cfg(test)] mod tests { … }` into
                           `<module>/tests.rs`. Dry run unless --apply. Refuses files
                           the move would break and says why.
  comment-worklist         every comment block over budget, largest first, so a person
                           or an agent can work one file at a time. --json available.
  extract-module-docs      move an over-budget `//!` preamble into
                           docs/dev-notes/<crate>/<module>.md, leaving the first lines
                           and a pointer. Dry run unless --apply.
  duplicate-tests          test names that are near-identical across files of one
                           crate: the same claim written twice.

Budgets live in BUDGETS below and are the only numbers to tune. Every count here is
line-based and relies on rustfmt having run, which pre-push guarantees.
"""

from __future__ import annotations

import argparse
import difflib
import json
import os
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BASELINE = Path(os.environ.get("MAINTAINABILITY_BASELINE", ROOT / "scripts" / "maintainability-baseline.json"))
DEV_NOTES = ROOT / "docs" / "dev-notes"

# Per-file budgets. `check` reports a file only when it breaks one of these AND is
# worse than the baseline, so the tree can be brought under budget one card at a time
# while nothing already inside a budget is allowed back out.
BUDGETS = {
    "inline_test_lines": 150,  # a trailing test module larger than this goes to <module>/tests.rs
    "test_file_lines": 1000,  # any file under tests/ or any <module>/tests.rs
    "test_fn_max_lines": 60,  # one test, signature to closing brace
    "test_name_max_chars": 60,
    "test_module_doc_lines": 10,  # `//!` preamble of a test file
    "src_module_doc_lines": 30,  # `//!` preamble of a production file
    "doc_block_max_lines": 15,  # one `///` block
    "src_comment_share": 0.35,  # comment lines / (code + comment) in production code
    "test_sleeps": 0,  # fixed waits in test code; poll or pause time instead
    "env_gated_tests": 0,  # a test that early-returns on an env var belongs under tests/live, #[ignore]d
}
WORKSPACE_RATIO_KEY = "test_to_prod_ratio"

TEST_ATTR = re.compile(r"^\s*#\[(tokio::test|test|sqlx::test)")
FN_LINE = re.compile(r"^(\s*)(pub(\([a-z]+\))?\s+)?(async\s+)?fn\s+([A-Za-z0-9_]+)\s*[<(]")
SLEEP = re.compile(r"\bsleep\(")
ENV_GATE = re.compile(r"env::var\(")


# ---------------------------------------------------------------- file parsing


def rs_files(paths: list[str] | None) -> list[Path]:
    if paths:
        out: list[Path] = []
        for p in paths:
            pp = ROOT / p
            out += [pp] if pp.is_file() else sorted(pp.rglob("*.rs"))
        return [f for f in out if "/target/" not in str(f)]
    tracked = subprocess.run(
        ["git", "ls-files", "crates/*.rs"], cwd=ROOT, capture_output=True, text=True, check=True
    ).stdout.split()
    return [ROOT / f for f in tracked]


def changed_files() -> list[Path]:
    out = subprocess.run(
        ["git", "status", "--porcelain", "--", "crates"], cwd=ROOT, capture_output=True, text=True
    ).stdout.splitlines()
    files = []
    for line in out:
        name = line[3:].split(" -> ")[-1]
        if name.endswith(".rs") and (ROOT / name).exists():
            files.append(ROOT / name)
    return files


def is_test_file(f: Path) -> bool:
    parts = f.relative_to(ROOT).parts
    return "tests" in parts or f.name == "tests.rs"


def trailing_test_module(lines: list[str]) -> int | None:
    """Index of the `#[cfg(test)]` that opens a file's trailing `mod tests`, or None."""
    for i, line in enumerate(lines):
        if line.rstrip() == "#[cfg(test)]":
            j = i + 1
            while j < len(lines) and lines[j].startswith("#["):
                j += 1
            if j < len(lines) and re.match(r"^(pub(\([a-z]+\))?\s+)?mod tests\s*\{", lines[j]):
                return i
    return None


def block_end(lines: list[str], start: int, indent: str) -> int:
    """Line index of the `}` closing the item opened at `start`, matched by indentation."""
    closer = indent + "}"
    for k in range(start + 1, len(lines)):
        if lines[k].rstrip() == closer:
            return k
    return len(lines) - 1


def tests_in(lines: list[str], lo: int, hi: int) -> list[tuple[str, int]]:
    """(name, body lines) for every test fn between lo and hi."""
    found = []
    i = lo
    while i < hi:
        if TEST_ATTR.match(lines[i]):
            j = i + 1
            while j < hi and not FN_LINE.match(lines[j]):
                if not lines[j].lstrip().startswith("#["):
                    break
                j += 1
            m = FN_LINE.match(lines[j]) if j < hi else None
            if m:
                end = block_end(lines, j, m.group(1))
                found.append((m.group(5), end - j + 1))
                i = end
        i += 1
    return found


def comment_runs(lines: list[str], lo: int, hi: int, marker: str) -> list[tuple[int, int]]:
    """(start index, length) of each run of consecutive lines starting with `marker`."""
    runs, start = [], None
    for k in range(lo, hi + 1):
        is_marker = k < hi and lines[k].lstrip().startswith(marker) and not lines[k].lstrip().startswith(marker + "/")
        if is_marker and start is None:
            start = k
        elif not is_marker and start is not None:
            runs.append((start, k - start))
            start = None
    return runs


def measure_file(f: Path) -> dict:
    lines = f.read_text(encoding="utf-8").split("\n")
    n = len(lines)
    if is_test_file(f):
        prod_hi = 0
    else:
        t = trailing_test_module(lines)
        prod_hi = n if t is None else t
    prod = lines[:prod_hi]
    code = sum(1 for l in prod if l.strip() and not l.strip().startswith("//"))
    comments = sum(1 for l in prod if l.strip().startswith("//"))
    tests = tests_in(lines, prod_hi, n)
    test_lines = n - prod_hi
    doc_blocks = comment_runs(lines, 0, prod_hi, "///")
    mod_doc = max((ln for _, ln in comment_runs(lines, 0, n, "//!")), default=0)
    test_part = "\n".join(lines[prod_hi:])
    env_gated = 0
    if not is_test_file(f) and tests:
        # a test that reads an env var and returns is a live test hiding as a unit test
        for k in range(prod_hi, n):
            m = FN_LINE.match(lines[k])
            if m and any(TEST_ATTR.match(lines[a]) for a in range(max(prod_hi, k - 3), k)):
                body = "\n".join(lines[k : block_end(lines, k, m.group(1)) + 1])
                if ENV_GATE.search(body) and re.search(r"^\s*return;", body, re.M):
                    env_gated += 1
    return {
        "file": str(f.relative_to(ROOT)),
        "prod_code": code,
        "prod_comment": comments,
        "src_comment_share": round(comments / (code + comments), 3) if code + comments else 0.0,
        "inline_test_lines": test_lines if not is_test_file(f) else 0,
        "test_file_lines": n if is_test_file(f) else 0,
        "test_lines": test_lines if not is_test_file(f) else n,
        "tests": len(tests),
        "test_fn_max_lines": max((ln for _, ln in tests), default=0),
        "test_fn_avg_lines": round(sum(ln for _, ln in tests) / len(tests), 1) if tests else 0,
        "test_name_max_chars": max((len(nm) for nm, _ in tests), default=0),
        "test_module_doc_lines": mod_doc if is_test_file(f) else 0,
        "src_module_doc_lines": mod_doc if not is_test_file(f) else 0,
        "doc_block_max_lines": max((ln for _, ln in doc_blocks), default=0),
        "test_sleeps": len(SLEEP.findall(test_part)),
        "env_gated_tests": env_gated,
    }


def totals(rows: list[dict]) -> dict:
    prod = sum(r["prod_code"] + r["prod_comment"] for r in rows)
    test = sum(r["test_lines"] for r in rows)
    return {
        "files": len(rows),
        "prod_lines": prod,
        "prod_comment_lines": sum(r["prod_comment"] for r in rows),
        "test_lines": test,
        "tests": sum(r["tests"] for r in rows),
        WORKSPACE_RATIO_KEY: round(test / prod, 3) if prod else 0.0,
        "test_sleeps": sum(r["test_sleeps"] for r in rows),
        "env_gated_tests": sum(r["env_gated_tests"] for r in rows),
    }


# ---------------------------------------------------------------- subcommands


def cmd_measure(args):
    rows = [measure_file(f) for f in rs_files(args.paths)]
    if args.json:
        print(json.dumps({"files": rows, "totals": totals(rows)}, indent=1))
        return 0
    t = totals(rows)
    if not args.totals:
        rows.sort(key=lambda r: -(r["test_lines"] + r["prod_code"]))
        print(f"{'file':58s} {'prod':>6s} {'cmnt':>5s} {'cm%':>4s} {'test':>6s} {'#t':>4s} {'avg':>5s} {'max':>4s} {'name':>4s} {'mdoc':>4s}")
        for r in rows[: args.top]:
            print(
                f"{r['file']:58s} {r['prod_code']:6d} {r['prod_comment']:5d} {int(100 * r['src_comment_share']):3d}% "
                f"{r['test_lines']:6d} {r['tests']:4d} {r['test_fn_avg_lines']:5.0f} {r['test_fn_max_lines']:4d} "
                f"{r['test_name_max_chars']:4d} {max(r['test_module_doc_lines'], r['src_module_doc_lines']):4d}"
            )
    print(
        f"totals: prod={t['prod_lines']} (comments {t['prod_comment_lines']}) test={t['test_lines']} "
        f"ratio={t[WORKSPACE_RATIO_KEY]} tests={t['tests']} sleeps_in_tests={t['test_sleeps']} env_gated={t['env_gated_tests']}"
    )
    return 0


def cmd_baseline(args):
    rows = [measure_file(f) for f in rs_files(None)]
    budgeted = lambda r: {k: r[k] for k in BUDGETS}
    data = {"budgets": BUDGETS, "totals": totals(rows), "files": {r["file"]: budgeted(r) for r in rows}}
    BASELINE.write_text(json.dumps(data, indent=1, sort_keys=True) + "\n")
    print(f"wrote {BASELINE}: {len(rows)} files, ratio {data['totals'][WORKSPACE_RATIO_KEY]}")
    return 0


def cmd_check(args):
    base = json.loads(BASELINE.read_text()) if BASELINE.exists() else {"files": {}, "totals": {}}
    files = changed_files() if args.changed else rs_files(args.paths)
    failures = []
    for f in files:
        r = measure_file(f)
        was = base["files"].get(r["file"], {})
        for key, cap in BUDGETS.items():
            val = r.get(key, 0)
            if val > cap and val > was.get(key, -1):
                failures.append(f"{r['file']}: {key}={val} (budget {cap}, baseline {was.get(key, 'new file')})")
    scratch = subprocess.run(
        ["git", "ls-files", "crates/*/tests/scratch_*.rs"], cwd=ROOT, capture_output=True, text=True
    ).stdout.split()
    for s in scratch:
        failures.append(f"{s}: scratch tests are for local proof only and are never tracked")
    if not args.changed and not args.paths:
        t = totals([measure_file(f) for f in files])
        was_ratio = base["totals"].get(WORKSPACE_RATIO_KEY)
        if was_ratio is not None and t[WORKSPACE_RATIO_KEY] > was_ratio + 0.005:
            failures.append(f"workspace {WORKSPACE_RATIO_KEY}={t[WORKSPACE_RATIO_KEY]} grew past baseline {was_ratio}")
    if failures:
        print("\nmaintainability budgets — a file got bigger than the budget allows and bigger than it was:\n")
        for line in failures:
            print("  " + line)
        print(
            "\nFix the size, not the check: split the test, shorten the name, move the module's tests to"
            "\n<module>/tests.rs (scripts/maintainability.py extract-tests <file> --apply), move the prose"
            "\nto docs/. Re-baseline only after a card that deliberately brought a file down."
        )
        return 1
    print(f"✓ maintainability budgets hold ({len(files)} files checked)")
    return 0


def extract_one(f: Path, apply: bool) -> str:
    lines = f.read_text(encoding="utf-8").split("\n")
    t = trailing_test_module(lines)
    if t is None:
        return f"skip  {f.relative_to(ROOT)}: no trailing test module"
    j = t + 1
    while lines[j].startswith("#["):
        j += 1
    body_lo = j + 1
    closer = None
    for k in range(len(lines) - 1, body_lo, -1):
        if lines[k].rstrip() == "}":
            closer = k
            break
    if closer is None or any(l.strip() for l in lines[closer + 1 :]):
        return f"skip  {f.relative_to(ROOT)}: test module is not the last item in the file"
    body = lines[body_lo:closer]
    if len(body) < BUDGETS["inline_test_lines"]:
        return f"skip  {f.relative_to(ROOT)}: {len(body)} lines, under the inline budget"
    if f.name in ("lib.rs", "main.rs", "mod.rs"):
        target = f.parent / "tests.rs"
        rel_path = "tests.rs"
        moves_deeper = False  # tests.rs lands beside the original file, not under it
    else:
        target = f.parent / f.stem / "tests.rs"
        rel_path = f"{f.stem}/tests.rs"
        moves_deeper = True
    if moves_deeper:
        # `include_str!("x")` is relative to the file, and the file moves one
        # directory down; prefixing every relative include with `../` keeps it
        # pointing at the same bytes. Absolute paths and `concat!(env!(...))`
        # forms are left alone. Not needed for lib.rs/main.rs/mod.rs, whose
        # tests.rs stays beside the original file.
        body = [re.sub(r'(include_(?:str|bytes)!\(\s*")(?!/)', r"\1../", l) for l in body]
        for k in range(len(body) - 1):
            if re.search(r"include_(str|bytes)!\(\s*$", body[k]):
                body[k + 1] = re.sub(r'^(\s*")(?!/)', r"\1../", body[k + 1], count=1)
    if target.exists():
        return f"skip  {f.relative_to(ROOT)}: {target.relative_to(ROOT)} already exists"
    dedented = [l[4:] if l.startswith("    ") else l for l in body]
    header = lines[t : j]  # the #[cfg(test)] and any attribute lines
    # An explicit #[path], never bare `mod tests;`: several files in this tree are
    # also pulled in a second time by a sibling's own explicit #[path] (this
    # codebase's own convention, documented in handlers/runner_protocol.rs and used
    # by tack-runner/src/client.rs) — under that double load, rustc's *implicit*
    # mod-directory inference for a `#[path]`-loaded file resolves to the file's own
    # directory, not `<stem>/`, so a bare `mod tests;` silently points at the wrong,
    # nonexistent path in that second copy while compiling fine in the first. An
    # explicit `#[path]` is always resolved relative to this file's own location,
    # regardless of how this file itself was loaded, so it is correct in both.
    if apply:
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text("\n".join(dedented).rstrip("\n") + "\n", encoding="utf-8")
        new_src = lines[:t] + header + [f'#[path = "{rel_path}"]', "mod tests;", ""]
        f.write_text("\n".join(new_src).rstrip("\n") + "\n", encoding="utf-8")
    verb = "moved" if apply else "would move"
    return f"{verb} {len(body):5d} lines  {f.relative_to(ROOT)} -> {target.relative_to(ROOT)}"


def cmd_extract_tests(args):
    files = [f for f in rs_files(args.paths) if not is_test_file(f)]
    n = 0
    for f in files:
        msg = extract_one(f, args.apply)
        if msg.startswith(("moved", "would")) or args.verbose:
            print(msg)
        n += msg.startswith(("moved", "would"))
    print(f"{'moved' if args.apply else 'would move'} {n} test modules; run `cargo fmt --all` then `cargo nextest run --workspace`")
    return 0


def cmd_comment_worklist(args):
    items = []
    for f in rs_files(args.paths):
        lines = f.read_text(encoding="utf-8").split("\n")
        rel = str(f.relative_to(ROOT))
        test = is_test_file(f)
        t = trailing_test_module(lines)
        prod_hi = 0 if test else (len(lines) if t is None else t)
        cap = BUDGETS["test_module_doc_lines"] if test else BUDGETS["src_module_doc_lines"]
        for start, ln in comment_runs(lines, 0, len(lines), "//!"):
            if ln > cap:
                items.append({"file": rel, "line": start + 1, "kind": "module doc", "lines": ln, "budget": cap, "head": lines[start].strip()[:80]})
        for start, ln in comment_runs(lines, 0, prod_hi, "///"):
            if ln > BUDGETS["doc_block_max_lines"]:
                items.append({"file": rel, "line": start + 1, "kind": "doc block", "lines": ln, "budget": BUDGETS["doc_block_max_lines"], "head": lines[start].strip()[:80]})
        r = measure_file(f)
        if not test and r["src_comment_share"] > BUDGETS["src_comment_share"] and r["prod_code"] > 100:
            items.append({"file": rel, "line": 1, "kind": "comment share", "lines": r["prod_comment"], "budget": int(BUDGETS["src_comment_share"] * 100), "head": f"{int(100 * r['src_comment_share'])}% of {r['prod_code'] + r['prod_comment']} lines"})
    items.sort(key=lambda i: -i["lines"])
    if args.json:
        print(json.dumps(items, indent=1))
        return 0
    by_file = defaultdict(list)
    for i in items:
        by_file[i["file"]].append(i)
    print(f"{len(items)} blocks over budget in {len(by_file)} files (largest first):\n")
    for i in items[: args.top]:
        print(f"  {i['lines']:4d} > {i['budget']:<3d} {i['kind']:13s} {i['file']}:{i['line']}  {i['head']}")
    return 0


def cmd_extract_module_docs(args):
    keep = args.keep
    n = 0
    for f in rs_files(args.paths):
        if is_test_file(f):
            continue
        lines = f.read_text(encoding="utf-8").split("\n")
        runs = [(s, ln) for s, ln in comment_runs(lines, 0, len(lines), "//!") if s < 5]
        if not runs or runs[0][1] <= BUDGETS["src_module_doc_lines"]:
            continue
        start, ln = runs[0]
        rel = f.relative_to(ROOT)
        crate = rel.parts[1]
        note = DEV_NOTES / crate / (rel.relative_to(Path("crates") / crate / "src").with_suffix(".md"))
        prose = [l.lstrip()[3:].rstrip() if l.lstrip().startswith("//! ") else l.lstrip()[3:] for l in lines[start : start + ln]]
        pointer = f"//! Design notes: {note.relative_to(ROOT)}"
        if apply := args.apply:
            note.parent.mkdir(parents=True, exist_ok=True)
            note.write_text(f"# `{rel}`\n\nMoved out of the module preamble; trim or delete freely.\n\n" + "\n".join(prose).strip() + "\n", encoding="utf-8")
            kept = lines[start : start + keep]
            new = lines[:start] + kept + ["//!", pointer] + lines[start + ln :]
            f.write_text("\n".join(new), encoding="utf-8")
        print(f"{'moved' if apply else 'would move'} {ln:4d} lines  {rel} -> {note.relative_to(ROOT)} (keeping {keep})")
        n += 1
    print(f"{n} module preambles over {BUDGETS['src_module_doc_lines']} lines")
    return 0


def cmd_duplicate_tests(args):
    by_crate: dict[str, list[tuple[str, str]]] = defaultdict(list)
    for f in rs_files(args.paths):
        lines = f.read_text(encoding="utf-8").split("\n")
        rel = f.relative_to(ROOT)
        for name, _ in tests_in(lines, 0, len(lines)):
            by_crate[rel.parts[1]].append((name, str(rel)))
    total = 0
    for crate, tests in sorted(by_crate.items()):
        pairs = []
        for i, (a, fa) in enumerate(tests):
            for b, fb in tests[i + 1 :]:
                if fa == fb or abs(len(a) - len(b)) > 12:
                    continue
                sm = difflib.SequenceMatcher(None, a, b)
                if sm.real_quick_ratio() > args.ratio and sm.quick_ratio() > args.ratio and sm.ratio() > args.ratio:
                    pairs.append((a, fa, b, fb))
        total += len(pairs)
        print(f"\n{crate}: {len(pairs)} near-identical pairs across files (ratio > {args.ratio})")
        for a, fa, b, fb in pairs[: args.top]:
            print(f"  {a}\n    {fa}\n  {b}\n    {fb}")
    print(f"\n{total} pairs in total")
    return 0


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    m = sub.add_parser("measure"); m.add_argument("paths", nargs="*"); m.add_argument("--json", action="store_true"); m.add_argument("--totals", action="store_true"); m.add_argument("--top", type=int, default=40)
    sub.add_parser("baseline")
    c = sub.add_parser("check"); c.add_argument("paths", nargs="*"); c.add_argument("--changed", action="store_true")
    e = sub.add_parser("extract-tests"); e.add_argument("paths", nargs="*"); e.add_argument("--apply", action="store_true"); e.add_argument("--verbose", action="store_true")
    w = sub.add_parser("comment-worklist"); w.add_argument("paths", nargs="*"); w.add_argument("--json", action="store_true"); w.add_argument("--top", type=int, default=60)
    d = sub.add_parser("extract-module-docs"); d.add_argument("paths", nargs="*"); d.add_argument("--apply", action="store_true"); d.add_argument("--keep", type=int, default=6)
    u = sub.add_parser("duplicate-tests"); u.add_argument("paths", nargs="*"); u.add_argument("--ratio", type=float, default=0.75); u.add_argument("--top", type=int, default=15)
    args = p.parse_args(argv)
    os.chdir(ROOT)
    return {
        "measure": cmd_measure, "baseline": cmd_baseline, "check": cmd_check, "extract-tests": cmd_extract_tests,
        "comment-worklist": cmd_comment_worklist, "extract-module-docs": cmd_extract_module_docs, "duplicate-tests": cmd_duplicate_tests,
    }[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
