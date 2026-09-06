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

A page section is one program -- `primaryCode` and the steps under it, which is
how the page reads -- wrapped in a module of its own and built against the real
facade. It is a ratchet: `docs/SITE_BLOCKS.md` records how many fail today and
that number may only go down. So it does not have to be finished in one go, and
it cannot quietly go back up either.

WHAT THE COUNT WAS MEASURING

On 05-09-2026 the report said 238 of 527, and 88 of those 238 were not on the
site at all. The export cut a how-to page into one unit per step and the
wrapper nested each unit in its own `fn main`, so step three could not see the
document step one opened; rustc said `cannot find value \`doc\`` and the report
called it documentation debt. Repairing both -- one program per page section
there, one shared scope here -- took the number to 94 without touching a word
of the site. A number nobody can work off is worse than no number: it is the
shape that makes a guard get switched off.

EVERY BLOCK COMPILES, OR IT IS WRITTEN DOWN BY NAME AND REASON

One number still answers the wrong question. Ten programs are cloud and browser
recipes on `tokio`, `reqwest`, `aws_sdk_*`, `lambda_runtime` and `wasm_bindgen`
-- crates this harness deliberately does not link, so they can never pass here.
They counted as debt that cannot be repaid, on top of the blocks that really
are wrong.

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
import contextlib
import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib

REPO = pathlib.Path(__file__).resolve().parents[2]
REPORT = REPO / "docs" / "SITE_BLOCKS.md"
EXCEPTIONS = REPO / "docs" / "site" / "blocks_exceptions.toml"
EXPORT = REPO / "docs" / "site-rust-blocks.json"

# FLOOR: programs >= 200, and the blocks they are made of >= 600.
#
# Two floors, because the first one alone measures the grouping rather than the
# site. Until 05-09-2026 the export cut a how-to page into one unit per step,
# which gave 527 units; grouping a page into the one program it actually is
# gives 241 of the same material. A single floor at 250 would have read that
# repair as a broken export and refused it, and a floor that a correct change
# trips is a floor that gets lowered without being thought about.
#
# So the floor that matters counts the blocks on the site (`onderdelen`), which
# no regrouping changes: 744 today. The unit count keeps a floor of its own for
# the case where grouping collapses everything into a handful of programs.
MIN_BLOCKS = 200
MIN_SOURCE_BLOCKS = 600

# FLOOR: the blocks behind the programs that do compile >= 350 -- measured 421
# of 744 on 05-09-2026.
#
# The ratchet does not cover this. Shrink the export to a third, or break `wrap`
# so every block turns into a single parse error, and the number of failures
# *falls* -- which the ratchet reads as progress and lets through as soon as
# somebody updates the report. A floor on what passes is the half of the
# measurement the ratchet cannot see.
#
# In blocks and not in programs, for the same reason as the floors above: how
# many programs there are is a matter of grouping, how many blocks there are is
# a matter of the site. The first version of this floor stood at 150 programs
# out of 551 units and had to be moved the moment the grouping was repaired --
# which is how a floor gets lowered on autopilot instead of being thought about.
MIN_COMPILING_BLOCKS = 350

# The register must not become the answer. At 49 of 551 it sits below a tenth;
# at a fifth this gate approves more than it checks, and then the honest move is
# to repair the export (#247) rather than to add entries to it.
MAX_EXCEPTION_SHARE = 0.20

# Two kinds, and no third. Every reason a block cannot build on its own is one
# of these; an invented API is neither, and that is exactly what this closed
# list holds back.
#
# There were four. `fragment` (a block continuing an earlier one) and
# `harness-bundle` (two blocks in one unit defining the same item) were both
# artefacts of how the export cut the site up, and both are gone since it groups
# a page section into the one program it is (#164): 39 entries of the first kind
# and every entry of the second lapsed on the same day. They are out of this set
# rather than left standing unused -- a kind nothing can legitimately claim is
# an excuse waiting for a bad afternoon.
KINDS = {"external-crate", "not-rust"}

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


# `fn main() { ... }` opened by a block, and its declared error type.
# Anchored at the start of the line: a nested `fn main` does not exist, and a
# `fn main` inside a string does not start a line.
MAIN_OPEN = re.compile(
    r"^(\s*)(?:pub\s+)?(?:async\s+)?fn\s+main\s*\(\s*\)\s*(?:->\s*(?P<ret>.+?)\s*)?\{\s*$"
)
TRAILING_OK = re.compile(r"\s*Ok\(\(\)\)\s*;?\s*$")


def unfold_main(lines: list[str]) -> tuple[list[str], str | None]:
    """Lift the body of every `fn main` out of its function, into one scope.

    The single largest class of failures this gate reported was not a fault on
    the site at all. 88 of the 238 read

        expected value, found built-in attribute `doc`

    which is rustc saying `doc` is not bound. It is bound: the section's first
    block is a whole program that opens the document, and the steps after it are
    loose statements that go on using it -- which is exactly how the page reads.
    Nesting the first block's `fn main` inside the wrapper hid its bindings from
    the statements that followed, so the harness invented an error the reader
    never sees. Counting that as documentation debt buries the blocks that are
    genuinely wrong under noise nobody can work off.

    So the bodies come out and share one scope, in page order. What is being
    measured is unchanged: every name and every shape still meets the real
    facade. Only the wrapper stops being visible to the code inside it.

    The closing brace is found by its exact line (`}` at the opening indent),
    not by counting braces: a `{` inside a string literal would move the count
    and a wrong cut invents errors instead of removing them -- the same trap
    that keeps the export from being split back apart here. A `fn main` whose
    close cannot be found that way is left exactly as it stands.

    Returns the lines, and the error type of the first `fn main` seen. The
    wrapper adopts it, so a block promising `pdfluent::Result<()>` is still held
    to it -- `std::fs::write(..)?` in such a block does not convert, and that is
    a real defect in a copied example, not an artefact of this harness.
    """
    out: list[str] = []
    declared: str | None = None
    i = 0
    while i < len(lines):
        m = MAIN_OPEN.match(lines[i])
        if not m:
            out.append(lines[i])
            i += 1
            continue
        close = m.group(1) + "}"
        end = next((j for j in range(i + 1, len(lines)) if lines[j].rstrip() == close), None)
        if end is None:
            out.append(lines[i])
            i += 1
            continue
        if declared is None and m.group("ret"):
            declared = m.group("ret")
        body = lines[i + 1 : end]
        while body and not body[-1].strip():
            body.pop()
        if body and TRAILING_OK.fullmatch(body[-1]):
            body.pop()
        out.extend(ln[4:] if ln.startswith("    ") else ln for ln in body)
        i = end + 1
    return out, declared


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

    # Most units bundle a whole section: the export concatenates the code blocks
    # of a section and puts the count in the path (`.sections[0] (5 blokken)`,
    # or `(page) (4 blokken)` for a page that has no sections). 241 programs,
    # made of the 744 blocks a reader sees.
    #
    # Without de-duplication that counts as broken what is not. Five blocks each
    # starting with `use pdfluent::PdfDocument;` yield, after hoisting, five
    # copies of the same import:
    #
    #     the name `PdfDocument` is defined multiple times
    #
    # Eighteen of the fifty reported errors were that, and not one of them is on
    # the site. The `fn main` each block brings along was the same kind of
    # phantom, and `unfold_main` above deals with it.
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

    rest, declared = unfold_main(rest)

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
    # The block's own promise, where it makes one. A block that says
    # `pdfluent::Result<()>` is held to it; one that says
    # `Result<(), Box<dyn Error>>` is held to that. Imposing one of the two on
    # every block would either wave through a `?` that does not convert or
    # report one that does.
    ret = declared or "pdfluent::Result<()>"
    return (
        f"#[allow(unused, clippy::all)]\nmod blok_{index} {{\n"
        + head
        + f"\n    pub fn draai() -> {ret} {{\n"
        + body
        + "\n        Ok(())\n    }\n}\n"
    )


# The build directory of the throwaway crate, one per checkout.
#
# This used to be a fresh `mkdtemp(prefix="siteblocks-")` per run, and nothing
# ever removed it. Each one holds a full debug build of the facade -- two
# gigabytes -- so on 05-09-2026 forty-five of them stood in $TMPDIR, free disk
# fell from 72 GB to 35 GB in three hours, and every push stopped at the floor
# the pre-push gate keeps (#344). Nobody noticed, because a leak that costs
# nothing per run only shows up as somebody else's failure.
#
# So the directory lives in the checkout that is being measured, at
# `target/site-blocks`. Three things follow from that, and all three are the
# point:
#
#   * $TMPDIR stays as this gate found it. That is checked, in
#     `test_site_blocks_register.py`, in both directions.
#   * There is exactly one of them per checkout instead of one per run, and it
#     goes when the worktree goes -- `/target` is ignored by git, and the
#     sweeper that clears `target/` of landed work clears this with it.
#   * It stays warm. The build is what costs the time here: five hundred blocks
#     against a freshly compiled facade is tens of minutes, and cold every time
#     is what made a fresh directory per run expensive as well as leaky.
#
# Per checkout and not one shared directory, deliberately. The manifest below
# pins `pdfluent` by absolute path, and a build directory shared between
# worktrees bakes in the paths of whichever one filled it -- that is what broke
# the landing lane on 05-09-2026 when landings briefly shared a target.
#
# The fallback is the old behaviour for the case where the checkout cannot be
# written to, and it is removed on the way out whether the gate returns or
# raises. `finally`, not a line at the end of `main`: this function has eight
# ways to return one.
WARM_DIR = pathlib.Path("target") / "site-blocks"


@contextlib.contextmanager
def build_workdir(repo: pathlib.Path):
    """A directory to build the blocks in, that does not outlive its purpose."""
    warm = repo / WARM_DIR
    try:
        warm.mkdir(parents=True, exist_ok=True)
    except OSError:
        warm = None
    if warm is not None:
        yield warm
        return
    temp = pathlib.Path(tempfile.mkdtemp(prefix="siteblocks-"))
    try:
        yield temp
    finally:
        shutil.rmtree(temp, ignore_errors=True)


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


def ratchet(recorded: list[int], now: list[int]) -> tuple[bool, list[str]]:
    """Whether the two counts may pass, and what to say about them.

    Apart from the build, like `judge()`, so that
    `test_site_blocks_register.py` can hold both directions to account in
    milliseconds instead of three quarters of an hour. A ratchet that only ever
    runs inside a cargo build is a ratchet nobody has tried to defeat.

    A rise refuses. A fall passes and says so.

    It used to refuse both ways, on the reasoning that progress nobody records
    drops out of sight. In practice that made every landing that followed
    another landing fail: master moves between the measurement and the push,
    some unrelated commit makes one more block compile, and the gate refuses a
    tree that is strictly better than the one it accepted an hour earlier.
    #1738 died on exactly that, 88 against 87. A guard that refuses good news is
    a guard that gets pushed past with `PRE_PUSH_SKIP=1`, and then nothing is
    checked at all.

    The ceiling still holds, and it holds at the *recorded* number -- so leaving
    a fall unrecorded buys no room for a later rise. It only leaves the report
    overstating the debt, which the message says out loud.
    """
    names = ("programs", "blocks on the site")
    stale = []
    for name, before, after in zip(names, recorded, now):
        if after > before:
            return False, [f"The number of non-compiling {name} rose from "
                           f"{before} to {after}."]
        if after < before:
            stale.append(f"{name}: {before} recorded, {after} actual")
    return True, stale


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
            f"FLOOR: {len(blocks)} programs, expected >= {MIN_BLOCKS}. "
            "The export is broken -- this is not a green.",
            file=sys.stderr,
        )
        return 1
    source_blocks = sum(len(b.get("onderdelen") or [b["pad"]]) for b in blocks)
    if source_blocks < MIN_SOURCE_BLOCKS:
        print(
            f"FLOOR: those programs are made of {source_blocks} blocks on the "
            f"site, expected >= {MIN_SOURCE_BLOCKS}. Grouping may change how "
            "the blocks are cut up; it may not make them disappear.",
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

    with build_workdir(REPO) as workdir:
        return compile_the_blocks(workdir, blocks, source_blocks, all_keys,
                                  register, form_errors, a.check)


def compile_the_blocks(
    workdir: pathlib.Path,
    blocks: list[dict],
    source_blocks: int,
    all_keys: set[tuple[str, str]],
    register: dict[tuple[str, str], dict],
    form_errors: list[str],
    check: bool,
) -> int:
    """Build every block, judge the outcome, and write or check the report.

    Apart from `main` because the build directory is a resource with a lifetime:
    everything from here on is inside `build_workdir`, and there is no return
    path that can step around its clean-up.
    """
    (workdir / "src").mkdir(parents=True, exist_ok=True)
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
    # The crates a recipe may lean on, besides the facade.
    #
    # Not linking these was hiding the thing being measured. A block that opens
    # with `use anyhow::Result;` failed on that line and rustc never reached the
    # pdfluent names below it -- 28 blocks whose real content was never checked,
    # written off as "needs a crate this harness does not link". A reader
    # copying such a recipe adds the crate to their own manifest and gets on
    # with it; refusing to do the same here measures the harness, not the page.
    #
    # These four and no more: all four are already in this workspace, so
    # `--offline` resolves them from the lockfile that is copied in above and
    # nothing reaches the network. `aws_sdk_*`, `lambda_runtime` and
    # `wasm_bindgen` stay out -- they carry a crate graph nobody here maintains,
    # and those recipes remain named in the register with their reason.
    (workdir / "Cargo.toml").write_text(
        "[package]\nname = \"siteblocks\"\nversion = \"0.0.0\"\nedition = \"2021\"\n"
        f"[dependencies]\npdfluent = {{ path = \"{REPO / 'crates' / 'pdfluent'}\" }}\n"
        "anyhow = \"1\"\nbase64 = \"0.22\"\nserde_json = \"1\"\nrayon = \"1\"\n"
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

    # The same debt, counted in blocks on the site.
    #
    # A ratchet on programs alone is one regrouping away from meaningless: put
    # two pages in one unit and the count halves without a line of the site
    # changing. `onderdelen` says which blocks a program was assembled from, so
    # this number moves only when a block is repaired or leaves the site -- and
    # both numbers are held to the ratchet, so neither can be traded for the
    # other.
    parts = {(b["bestand"], b["pad"]): len(b.get("onderdelen") or [b["pad"]])
             for b in blocks}
    debt_blocks = sum(parts[(m["bestand"], m["pad"])] for m in unexplained)

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

    compiling_blocks = source_blocks - sum(
        parts[k] for k in failing if k in parts
    )
    if compiling_blocks < MIN_COMPILING_BLOCKS:
        print(
            f"FLOOR: {compiling} of {len(blocks)} programs compile, carrying "
            f"{compiling_blocks} of {source_blocks} blocks, expected >= "
            f"{MIN_COMPILING_BLOCKS}. A ratchet that only counts failures reads "
            f"a shrunken or broken measurement as progress -- this is not a "
            f"green.",
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
        f"{compiling} of {len(blocks)} programs compile. "
        f"{len(failing)} do not, of which {len(register)} with a reason in "
        f"{EXCEPTIONS.relative_to(REPO)} and {count} without "
        f"({debt_blocks} of {source_blocks} blocks on the site)."
    )
    for s, n in sorted(per_kind.items(), key=lambda x: -x[1]):
        print(f"  {n:4}  {s}")

    if check:
        if not REPORT.exists():
            print(f"{REPORT} is missing; run without --check first.", file=sys.stderr)
            return 1
        text = REPORT.read_text()
        recorded = [int(x) for x in re.findall(r"\*\*(\d+) of \d+\*\*", text)]
        # FLOOR: two numbers in the report, or the ratchet is holding one of
        # them and not the other. A missing number reads as "nothing to check".
        if len(recorded) < 2:
            print(
                f"{REPORT.name} records {len(recorded)} of the two counts "
                "(programs, and the blocks behind them). Run without --check.",
                file=sys.stderr,
            )
            return 1
        ok, notes = ratchet(recorded[:2], [count, debt_blocks])
        if not ok:
            for line in notes:
                print(line, file=sys.stderr)
            return 1
        if notes:
            print(
                f"{REPORT.name} overstates the debt and may be regenerated:\n  "
                + "\n  ".join(notes)
                + f"\nRun `python3 {pathlib.Path(__file__).relative_to(REPO)}` "
                "without --check and commit the result. Not a failure: the "
                "ceiling is what this gate holds, and it is still held."
            )
        return 0

    per_file: dict[str, int] = {}
    for m in unexplained:
        per_file[m["bestand"]] = per_file.get(m["bestand"], 0) + 1

    lines = [
        "# Code blocks on pdfluent.com that do not compile",
        "",
        "Generated by `scripts/ci/site_blocks_compile.py`. Do not edit by hand.",
        "",
        f"**{count} of {len(blocks)}** Rust programs on the English site do not build",
        "against the real facade, and have no reason recorded for it. Counted in the",
        f"blocks a reader sees, that is **{debt_blocks} of {source_blocks}**.",
        "",
        "Both numbers, because one of them alone is a regrouping away from meaningless:",
        "a program is a page section -- `primaryCode` and the steps under it, which is",
        "how the page reads -- so merging two pages into one unit would halve the first",
        "number without repairing anything. The second moves only when a block is fixed",
        "or leaves the site. The ratchet holds both.",
        "",
        f"{compiling} programs do build. The remaining {len(register)} are named with a kind and a",
        "reason in `docs/site/blocks_exceptions.toml`: cloud and browser recipes needing a",
        "crate this harness deliberately does not link. Those cannot build here at all, so",
        "they do not count as debt -- and they are written down one by one, with the crate",
        "named, not as a category.",
        "",
        "Neither number may go up. A name check catches an invented type; only a",
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
