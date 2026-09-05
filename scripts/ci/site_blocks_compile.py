#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Compiles the Rust examples shown on pdfluent.com.

On 25-08-2026 the visible site carried 1130 code blocks and not one of them had
ever been compiled (#164). The name check in the website repository catches an
invented type, but not a real name in a shape that does not exist. One of the
blocks on the site read `doc.compress().add_watermark("DRAFT")
.convert_to_pdfa("PDF/A-1b")` -- three real method names, none of them with that
shape. Only a compiler sees that.

Every block is wrapped in a module of its own and built against the real facade.
It is a ratchet: `docs/SITE_BLOCKS.md` records how many fail today and that
number may only go down. So it does not have to be finished in one go, and it
cannot quietly go back up either.

EVERY BLOCK COMPILES, OR IT IS WRITTEN DOWN BY NAME AND REASON

One number answers the wrong question. On 30-08-2026 it stood at 364 of 551,
and 49 of those 364 were not documentation faults: 42 were steps two through
five of a numbered list, with their `use` lines in step one, and 7 were Lambda
and WebAssembly recipes needing crates this harness deliberately does not link.
Compiled on their own those can never pass. They counted as debt that cannot be
repaid, on top of the blocks that really are wrong.

Hence the rule: a published block compiles, or it is named in
`docs/site/blocks_exceptions.toml` with what it is and why. The ratchet counts
what remains, and that is the number that can reach zero.

The register is read in both directions. An entry for a block that compiles is
as wrong as a block that fails without an entry -- an exception list consulted
only on failure only ever grows, and one nobody prunes means nothing after a
while. The same shape as a test that stopped asserting.

THE HANDOVER

`docs/site-rust-blocks.json` is a copy of an export generated in the website
repository. A copy drifts: between 25-08-2026 and 03-09-2026 the site gained 17
blocks, lost 41 and rewrote 60, and this gate compiled none of that -- it was
still building the August snapshot and reporting a number about a site that no
longer existed. `handover.digest` closes it. The website refuses to build when
its blocks hash to something this file has not accepted, so a new block on the
site cannot go live before it has been through the compiler here.
"""
import argparse
import hashlib
import json
import os
import pathlib
import re
import subprocess
import sys
import tempfile
import tomllib

REPO = pathlib.Path(__file__).resolve().parents[2]
REPORT = REPO / "docs" / "SITE_BLOCKS.md"
EXCEPTIONS = REPO / "docs" / "site" / "blocks_exceptions.toml"
EXPORT = REPO / "docs" / "site-rust-blocks.json"

# FLOOR: programs >= 250 -- the site has well over five hundred (817 separate
# blocks, grouped per section). Find fewer and the export is broken, not the
# site empty.
MIN_BLOCKS = 250

# FLOOR: blocks that do compile >= 150 -- measured 200 of 551 on 30-08-2026.
# The ratchet does not cover this. Shrink the export to a third, or break
# `wrap` so every block turns into a single parse error, and the number of
# failures *falls* -- which the ratchet reads as progress and lets through as
# soon as somebody updates the report. A floor on what passes is the half of
# the measurement the ratchet cannot see.
MIN_COMPILING = 150

# The register must not become the answer. At 49 of 551 it sits below a tenth;
# at a fifth this gate approves more than it checks, and then the honest move is
# to repair the export (#247) rather than to add entries to it.
MAX_EXCEPTION_SHARE = 0.20

# Four kinds, and no fifth. Every reason a block cannot build on its own is one
# of these; an invented API is none of them, and that is exactly what this
# closed list holds back.
KINDS = {"fragment", "external-crate", "not-rust", "harness-bundle"}

# rustc error codes, grouped by what the reader has to do about them.
# Mechanical, not by eye: the code is in the output and means one thing.
#
#   UNKNOWN  the path or the name does not exist -- the documentation points
#            nowhere. Remove it or build it; that is a decision, not a typo.
#   SHAPE    the name exists, in another shape. A different argument count, a
#            `?` on something that is not a `Result`, a missing conversion.
#            Repairing costs one line and does not touch the promise.
#
# Anything not listed here shows up under "other", with the code attached, so
# that a new kind of failure stands out instead of disappearing into a bucket.
UNKNOWN = {"E0405", "E0412", "E0422", "E0423", "E0425", "E0432", "E0433",
           "E0531", "E0560", "E0576", "E0599", "E0609"}
SHAPE = {"E0061", "E0107", "E0271", "E0277", "E0308", "E0369", "E0614", "E0624"}


def digest_of(blocks: list[dict]) -> str:
    """The handover fingerprint of a block list.

    Computed the same way here and in the website repository
    (`scripts/verify-site-blocks-handover.mjs`), which is the whole point: the
    two sides have to be able to disagree out loud. Only the three fields that
    decide what gets compiled go in, sorted, so that reordering the export or
    adding a field to it is not mistaken for a changed example.
    """
    core = sorted(
        ({"bestand": b["bestand"], "pad": b["pad"], "code": b["code"]} for b in blocks),
        key=lambda b: (b["bestand"], b["pad"]),
    )
    payload = json.dumps(core, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    return hashlib.sha256(payload.encode("utf-8")).hexdigest()


def wrap(index: int, code: str) -> str:
    """Put a block in a module of its own so it can be compiled separately.

    A module rather than just a function: blocks carry their own `use` lines and
    those collide at module level.

    Note the multi-line `use`. Splitting it on the first line broke the whole
    file -- and then rustc reports six parse errors and stops. The first version
    of this script therefore reported "1 of 817 does not compile", which read as
    excellent news while nothing had been measured. The same failure as
    everywhere in docs/KWALITEITSSPOOR.md: the guard stopped looking and said
    green.
    """
    uses, rest, in_use = [], [], False
    for line in code.splitlines():
        if in_use:
            uses.append(line)
            if line.rstrip().endswith(";"):
                in_use = False
            continue
        if line.strip().startswith("use "):
            uses.append(line)
            if not line.rstrip().endswith(";"):
                in_use = True
            continue
        rest.append(line)

    # 89 of the 551 units bundle a whole section: the export concatenates the
    # code blocks of a section and puts the count in the path
    # (`.sections[0] (5 blokken)`). The site therefore carries 817 blocks, not 551.
    #
    # Without de-duplication that counts as broken what is not. Five blocks each
    # starting with `use pdfluent::PdfDocument;` yield, after hoisting, five
    # copies of the same import:
    #
    #     the name `PdfDocument` is defined multiple times
    #
    # Eighteen of the fifty reported errors were that, and not one of them is on
    # the site. The same goes for the `fn main` each block brings along.
    #
    # The real fix belongs in the export: one unit per block. The count in the
    # path proves it knows the boundaries. Splitting back apart here cannot be
    # done reliably -- not every block starts with `use` and some are loose
    # fragments without `fn main` -- and a wrong split invents errors instead of
    # removing them.
    #
    # De-duplication is by name, not by line. The five blocks of annotations.json
    # import `Annotation` in four different spellings --
    # `use pdfluent::{PdfDocument, Annotation, Color, Rect};` next to
    # `use pdfluent::{Annotation, Rect, StampStyle};` -- so five unique lines that
    # still collide after hoisting. A first version de-duplicated the lines and
    # changed nothing.
    per_prefix: dict[str, list[str]] = {}
    other: list[str] = []
    seen_other: set[str] = set()
    text = "\n".join(uses)
    for statement in [v.strip() for v in text.split(";") if v.strip()]:
        statement = " ".join(statement.split())
        m = re.match(r"use ([\w:]+)::\{(.+)\}$", statement)
        if m and " as " not in m.group(2):
            prefix = m.group(1)
            for name in (n.strip() for n in m.group(2).split(",")):
                if name and name not in per_prefix.setdefault(prefix, []):
                    per_prefix[prefix].append(name)
            continue
        m = re.match(r"use ([\w:]+)::(\w+)$", statement)
        if m:
            prefix, name = m.group(1), m.group(2)
            if name not in per_prefix.setdefault(prefix, []):
                per_prefix[prefix].append(name)
            continue
        # Globs, aliases and everything that does not fit here stay as they are:
        # merging what you do not understand produces code nobody wrote.
        if statement not in seen_other:
            seen_other.add(statement)
            other.append(statement + ";")

    uses = [f"use {p}::{{{', '.join(n)}}};" if len(n) > 1 else f"use {p}::{n[0]};"
            for p, n in per_prefix.items()] + other

    renamed, counters = [], {}
    for line in rest:
        m = re.match(r"(\s*)(?:pub )?fn (\w+)\(", line)
        if m:
            name = m.group(2)
            counters[name] = counters.get(name, 0) + 1
            if counters[name] > 1:
                line = line.replace(f"fn {name}(", f"fn {name}_{counters[name]}(", 1)
        renamed.append(line)
    rest = renamed
    body = "\n".join("        " + r for r in rest)
    head = "\n".join("    " + u for u in uses)
    return (
        f"#[allow(unused, clippy::all)]\nmod blok_{index} {{\n"
        + head
        + "\n    pub fn draai() -> pdfluent::Result<()> {\n"
        + body
        + "\n        Ok(())\n    }\n}\n"
    )


def build(workdir: pathlib.Path, pieces: list[str]) -> tuple[int, str]:
    (workdir / "src" / "lib.rs").write_text("".join(pieces))
    # A build directory of its own, not the caller's.
    #
    # In CI `CARGO_TARGET_DIR` points at a shared lock directory. This throwaway
    # crate is called `siteblocks` and built along in there, next to the real
    # workspace. That is not merely untidy -- it is the likely reason this check
    # reported "0 of 551" in CI on 26-08-2026 where it found 388 locally.
    env = dict(os.environ)
    env["CARGO_TARGET_DIR"] = str(workdir / "target")
    # No colour.
    #
    # This is the reason this check reported "0 of 551" in CI where it found 388
    # locally. Cargo colours its output when it thinks a terminal is watching,
    # and then there is
    #
    #   src/lib.rs:511:9: \x1b[1m\x1b[91merror\x1b[0m[E0428]: ...
    #
    # between the line number and the word `error`. The regex below does not
    # recognise that, so no error could be attributed to a block -- and zero
    # errors on 551 blocks reads as perfect. The errors were there.
    env["CARGO_TERM_COLOR"] = "never"
    r = subprocess.run(
        ["cargo", "build", "--quiet", "--offline", "--color", "never",
         "--message-format=short"],
        cwd=workdir, capture_output=True, text=True, env=env,
    )
    # And to be sure: whatever colour does come through is stripped before
    # matching. A parser that falls silent on formatting is not a parser.
    return r.returncode, ANSI.sub("", r.stderr)


ANSI = re.compile(r"\x1b\[[0-9;]*m")
PARSE_ERROR = re.compile(r"error: expected ")
# The error line, with its code. That code was always there and was thrown away;
# it is the only mechanical difference between "this path does not exist" and
# "this path exists, in another shape", and those two ask for different work.
ERROR_LINE = re.compile(r"src/lib\.rs:(\d+):\d+: error(?:\[([^\]]+)\])?: (.*)")


def key(block: dict) -> tuple[str, str]:
    """Where a block sits on the site. File plus field path is unique (551/551)."""
    return (block["bestand"], block["pad"])


def read_register(path: pathlib.Path) -> tuple[dict[tuple[str, str], dict], list[str]]:
    """The exception register, plus whatever is wrong with its form.

    Form errors are collected rather than raised: a register that is half right
    has to show every complaint at once, or it costs as many rounds as it holds
    mistakes.
    """
    if not path.exists():
        return {}, [f"{path.relative_to(REPO)} is missing."]
    try:
        raw = tomllib.loads(path.read_text(encoding="utf-8"))
    except tomllib.TOMLDecodeError as e:
        return {}, [f"{path.relative_to(REPO)} is not valid TOML: {e}"]

    out: dict[tuple[str, str], dict] = {}
    complaints: list[str] = []
    for i, item in enumerate(raw.get("block", []), 1):
        missing = [v for v in ("bestand", "pad", "kind", "why", "issue")
                   if not str(item.get(v, "")).strip()]
        if missing:
            complaints.append(f"entry {i}: missing {', '.join(missing)}")
            continue
        if item["kind"] not in KINDS:
            complaints.append(
                f"entry {i} ({item['bestand']} {item['pad']}): kind "
                f"{item['kind']!r} does not exist. Choose from: "
                f"{', '.join(sorted(KINDS))}."
            )
            continue
        # A three-word reason is not a reason. The next reader has to be able to
        # judge whether it still holds, and that takes a sentence.
        if len(item["why"].split()) < 5:
            complaints.append(
                f"entry {i} ({item['bestand']} {item['pad']}): `why` says too "
                f"little: {item['why']!r}"
            )
            continue
        k = (item["bestand"], item["pad"])
        if k in out:
            complaints.append(f"entry {i}: {k[0]} {k[1]} is in there twice")
            continue
        out[k] = item
    return out, complaints


def judge(
    every: set[tuple[str, str]],
    failing: set[tuple[str, str]],
    register: dict[tuple[str, str], dict],
) -> list[str]:
    """The two directions in which the register can be wrong.

    Kept apart from the build, so that `test_site_blocks_register.py` can
    exercise both directions without compiling five hundred blocks -- and so
    that losing one direction gives a red test rather than a gate that quietly
    does half the job.
    """
    complaints = []
    for k in sorted(register):
        if k not in every:
            complaints.append(
                f"{k[0]} {k[1]}: is in the register and no longer in the "
                f"export. The block is gone or renamed; drop the entry."
            )
        elif k not in failing:
            complaints.append(
                f"{k[0]} {k[1]}: is in the register and compiles. The reason has "
                f"lapsed, so the entry has too -- leave it and the register will "
                f"soon be covering blocks that have been right for months."
            )
    return complaints


def kind_of(codes: set[str]) -> str:
    """What a failing block asks for, read off the rustc codes."""
    if codes & UNKNOWN:
        return "path or name does not exist"
    if codes & SHAPE:
        return "exists, but not in that shape"
    if codes & {"PARSE"}:
        return "does not parse as Rust"
    return "other: " + ",".join(sorted(codes) or ["?"])


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("blocks", nargs="?", default=str(EXPORT),
                   help="path to site-rust-blocks.json (default: the checked-in copy)")
    p.add_argument("--check", action="store_true", help="fail if the failure count rises")
    a = p.parse_args()

    raw = json.loads(pathlib.Path(a.blocks).read_text())
    # The checked-in copy carries a note and a handover; a bare export does not.
    blocks = raw["blocks"] if isinstance(raw, dict) else raw
    if len(blocks) < MIN_BLOCKS:
        print(
            f"FLOOR: {len(blocks)} blocks, expected >= {MIN_BLOCKS}. "
            "The export is broken -- this is not a green.",
            file=sys.stderr,
        )
        return 1

    # The handover, if this copy claims one. A hand-edited block is the quiet
    # way to make this gate green: change the example here, leave the site
    # alone, and the number falls while the reader still gets the broken code.
    handover = raw.get("handover") if isinstance(raw, dict) else None
    if handover:
        recorded, actual = handover.get("digest"), digest_of(blocks)
        if recorded != actual:
            print(
                f"HANDOVER: {pathlib.Path(a.blocks).name} carries digest "
                f"{recorded} and its blocks hash to {actual}.\n"
                "Either the blocks were edited here -- they may not be, the "
                "site is the source -- or the export was refreshed without "
                "updating `handover`. The website refuses to build against a "
                "digest this file has not accepted, so leaving these two apart "
                "means the site and the compiler are looking at different code.",
                file=sys.stderr,
            )
            return 1
        if handover.get("blocks") != len(blocks):
            print(
                f"HANDOVER: `handover.blocks` says {handover.get('blocks')} "
                f"and the file carries {len(blocks)}.",
                file=sys.stderr,
            )
            return 1

    all_keys = {key(b) for b in blocks}
    register, form_errors = read_register(EXCEPTIONS)
    if len(register) > len(blocks) * MAX_EXCEPTION_SHARE:
        form_errors.append(
            f"{len(register)} of {len(blocks)} blocks are in the register. "
            f"Above {MAX_EXCEPTION_SHARE:.0%} this gate approves more than it "
            f"checks; repair the export (#247) instead of adding entries to it."
        )

    workdir = pathlib.Path(tempfile.mkdtemp(prefix="siteblocks-"))
    (workdir / "src").mkdir()
    # Take the workspace lockfile along.
    #
    # `--offline` can only choose versions already in the registry cache. Without
    # a lockfile cargo resolves afresh and trips over something that is not in
    # there -- on 26-08-2026 that was `spin ^0.9.8`, and because that error
    # yields no line number in src/lib.rs at all, the check counted zero errors
    # on 551 blocks and read that as perfect.
    lock = REPO / "Cargo.lock"
    if lock.exists():
        (workdir / "Cargo.lock").write_bytes(lock.read_bytes())
    (workdir / "Cargo.toml").write_text(
        "[package]\nname = \"siteblocks\"\nversion = \"0.0.0\"\nedition = \"2021\"\n"
        f"[dependencies]\npdfluent = {{ path = \"{REPO / 'crates' / 'pdfluent'}\" }}\n"
        "[workspace]\n"
    )

    # In groups, not everything in one file: one block with a parse error would
    # otherwise wreck the whole file and then nothing is measured. A group that
    # does not parse is redone block by block, so the damage stays bounded.
    GROUP = 40
    failed = []
    last_returncode, last_stderr = 0, ""
    for start in range(0, len(blocks), GROUP):
        group = blocks[start : start + GROUP]
        pieces, bounds = [], []
        line = 1
        for i, b in enumerate(group):
            text = wrap(start + i, b["code"])
            pieces.append(text)
            bounds.append((line, line + text.count("\n"), b))
            line += text.count("\n")

        code, stderr = build(workdir, pieces)
        last_returncode, last_stderr = code, stderr
        if code != 0 and PARSE_ERROR.search(stderr):
            # Parse error: unreliable to attribute to blocks. Do them singly.
            for b in group:
                c2, e2 = build(workdir, [wrap(0, b["code"])])
                if c2 == 0:
                    continue
                codes, first = set(), "unknown error"
                for ln in e2.splitlines():
                    m = ERROR_LINE.match(ln)
                    if m:
                        codes.add(m.group(2) or "PARSE")
                        if first == "unknown error":
                            first = m.group(3)
                    elif ln.startswith("error"):
                        codes.add("PARSE")
                        if first == "unknown error":
                            first = ln.split(": ", 1)[-1]
                failed.append({"bestand": b["bestand"], "pad": b["pad"],
                               "fout": first[:110], "codes": codes or {"PARSE"}})
            continue

        error_per_line: dict[int, tuple[str, str]] = {}
        for ln in stderr.splitlines():
            m = ERROR_LINE.match(ln)
            if m:
                error_per_line.setdefault(
                    int(m.group(1)), (m.group(2) or "PARSE", m.group(3)[:110])
                )
        for begin, end, b in bounds:
            errors = [t for r_, t in error_per_line.items() if begin <= r_ < end]
            if errors:
                failed.append({"bestand": b["bestand"], "pad": b["pad"],
                               "fout": errors[0][1],
                               "codes": {c for c, _ in errors}})

    failing = {(m["bestand"], m["pad"]) for m in failed}
    compiling = len(blocks) - len(failing)
    unexplained = [m for m in failed if (m["bestand"], m["pad"]) not in register]
    count = len(unexplained)

    # A build that fails as a whole counts as zero -- and that reads as perfect.
    #
    # Blocks are only counted as failing when an error can be attributed to a
    # line in src/lib.rs. If cargo fails for another reason (a dependency that
    # cannot be resolved offline, a build directory that collides), that list is
    # empty and this check reports zero errors on 551 blocks.
    #
    # On 26-08-2026 that happened: CI reported "0 of 551" while the same commit
    # gave 388 errors locally. Zero is not an outcome here but a symptom.
    if not failed and last_returncode != 0:
        print(
            "site_blocks: cargo failed, but not one error could be attributed to "
            "a block.\nThat counts as zero and reads as perfect, and it is not. "
            "Last stderr:",
            file=sys.stderr,
        )
        for ln in last_stderr.splitlines()[:8]:
            print(f"  {ln[:120]}", file=sys.stderr)
        return 1

    if compiling < MIN_COMPILING:
        print(
            f"FLOOR: {compiling} of {len(blocks)} blocks compile, expected >= "
            f"{MIN_COMPILING}. A ratchet that only counts failures reads a "
            f"shrunken or broken measurement as progress -- this is not a green.",
            file=sys.stderr,
        )
        return 1

    # The two directions of the register, and its form. Apart from the ratchet:
    # a lapsed exception is a fault in itself, even when the number falls.
    complaints = form_errors + judge(all_keys, failing, register)
    if complaints:
        print(
            f"[site-blocks] {EXCEPTIONS.relative_to(REPO)} is not right:",
            file=sys.stderr,
        )
        for r in complaints:
            print(f"  - {r}", file=sys.stderr)
        return 1

    per_kind: dict[str, int] = {}
    for m in unexplained:
        s = kind_of(m["codes"])
        per_kind[s] = per_kind.get(s, 0) + 1

    print(
        f"{compiling} of {len(blocks)} blocks compile. "
        f"{len(failing)} do not, of which {len(register)} with a reason in "
        f"{EXCEPTIONS.relative_to(REPO)} and {count} without."
    )
    for s, n in sorted(per_kind.items(), key=lambda x: -x[1]):
        print(f"  {n:4}  {s}")

    if a.check:
        if not REPORT.exists():
            print(f"{REPORT} is missing; run without --check first.", file=sys.stderr)
            return 1
        m = re.search(r"\*\*(\d+) of (\d+)\*\*", REPORT.read_text())
        before = int(m.group(1)) if m else 10**9
        if count > before:
            print(
                f"The number of non-compiling blocks rose from {before} to {count}.",
                file=sys.stderr,
            )
            return 1
        if count < before:
            print(
                f"The number fell from {before} to {count}. Update {REPORT.name} and "
                "commit it, or the progress drops out of sight.",
                file=sys.stderr,
            )
            return 1
        return 0

    per_file: dict[str, int] = {}
    for m in unexplained:
        per_file[m["bestand"]] = per_file.get(m["bestand"], 0) + 1

    lines = [
        "# Code blocks on pdfluent.com that do not compile",
        "",
        "Generated by `scripts/ci/site_blocks_compile.py`. Do not edit by hand.",
        "",
        f"**{count} of {len(blocks)}** Rust blocks on the English site do not build against",
        "the real facade, and have no reason recorded for it.",
        "",
        f"{compiling} do build. The remaining {len(register)} are named with a kind and a",
        "reason in `docs/site/blocks_exceptions.toml`: fragments continuing an earlier block",
        "on the same page, and recipes needing a crate this harness does not link. Those",
        "cannot build on their own, so they do not count as debt -- and they are written",
        "down one by one, not as a category.",
        "",
        "This number may only go down. A name check catches an invented type; only a",
        "compiler catches a real name in a shape that does not exist -- and that was exactly",
        "the mistake a language model made twice while writing replacement examples.",
        "",
        "## What it asks for",
        "",
        "Read off the rustc error code, not by eye. The two ask for different work: a path",
        "that points nowhere is a decision (build it or drop it), a name in the wrong shape",
        "is one line to repair.",
        "",
        "| blocks | what is going on |",
        "|---:|---|",
    ]
    for s, n in sorted(per_kind.items(), key=lambda x: -x[1]):
        lines.append(f"| {n} | {s} |")
    lines += ["", "## Per file", "", "| file | blocks |", "|---|---:|"]
    for file, n in sorted(per_file.items(), key=lambda x: -x[1]):
        lines.append(f"| `{file}` | {n} |")
    lines += ["", "## The first fifty", "", "| file | field | error |", "|---|---|---|"]
    for m in unexplained[:50]:
        lines.append(f"| `{m['bestand']}` | `{m['pad']}` | {m['fout']} |")
    REPORT.write_text("\n".join(lines) + "\n")
    print(f"-> {REPORT.relative_to(REPO)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
