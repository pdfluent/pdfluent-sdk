#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.

"""The site-blocks exception register is read in both directions.

`site_blocks_compile.py` lets a Rust block on pdfluent.com fail to compile,
provided it is named with a reason in `docs/site/blocks_exceptions.toml`. That
is a hole the size of the list, and it widens in exactly one way: the direction
"named here and compiles" drops out. After that the register keeps covering
blocks that were fixed long ago, nothing turns red, and the ratchet excuses a
number that no longer means anything -- the same shape as a test that stopped
asserting.

That direction cannot be proved by running the real gate: it compiles five
hundred blocks and takes three quarters of an hour, so it runs on one runner
and not on every push. The judgement therefore lives in `beoordeel()`, apart
from the build, and this file exercises that function on synthetic input.
Seconds, on both remotes, every round.

It then turns the same functions on the checked-in register: well-formed, under
its cap, and every entry naming a block the export still carries. That half
needs no compiler either, so a stale entry costs one round to find rather than
one build.

The same goes for `wrap()`, which decides what rustc is shown. A section on the
site is one program: the first block opens the document, the steps after it go
on using it. Leave each block inside its own `fn main` and those steps lose
their bindings, and the gate reports 88 documentation faults that are not on the
page. Nothing about that failure is visible in the count -- it looks like work
to do -- so the shape of the wrapper is checked here, on synthetic input, in
milliseconds.

Mutation-tested, which is the only reason to believe it. Each of the checks
below was removed from `site_blocks_compile.py` one at a time, and each time
this file went red. If it stays green while the gate is broken, it tests
nothing.
"""

import importlib.util
import json
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
GATE = ROOT / "scripts" / "ci" / "site_blocks_compile.py"
EXPORT = ROOT / "docs" / "site-rust-blocks.json"


def load():
    spec = importlib.util.spec_from_file_location("site_blocks_compile", GATE)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


ENTRY = ('[[block]]\nbestand = "{b}"\npad = "{p}"\nkind = "{k}"\n'
         'issue = "1"\nwhy = "{w}"\n')

# Long enough to pass the "a reason is a sentence" rule, so that a case testing
# something else does not fail on the reason instead.
REASON = "continues an earlier block on the same page, which carries the imports"


def register_from(sbc, entries: list[tuple[str, str, str, str]]):
    """Write a register and read it back through the real parser.

    Through the parser rather than by hand-building the dict: the form checks
    are half of what is under test, and a dict built here would skip them.
    """
    text = "\n".join(ENTRY.format(b=b, p=p, k=k, w=w) for b, p, k, w in entries)
    with tempfile.NamedTemporaryFile("w", suffix=".toml", delete=False) as fh:
        fh.write(text)
        path = Path(fh.name)
    try:
        return sbc.read_register(path)
    finally:
        path.unlink()


def cases(sbc) -> list[tuple[str, bool, str]]:
    """(name, passed, what it actually got) for every judgement the gate makes."""
    out = []
    every = {("a.json", ".x"), ("b.json", ".y"), ("c.json", ".z")}

    # 1. A block that fails and is not in the register is debt, not a register
    #    complaint. It has to reach the ratchet, and `beoordeel` must stay
    #    quiet about it -- a false complaint here would push whoever reads it
    #    towards adding an entry, which is the wrong repair.
    reg, form = register_from(sbc, [("a.json", ".x", "external-crate", REASON)])
    said = sbc.judge(every, {("a.json", ".x"), ("b.json", ".y")}, reg)
    out.append((
        "a failing block with no entry raises no register complaint",
        not form and said == [],
        f"form={form} said={said}",
    ))

    # 2. THE DIRECTION THAT GOES MISSING: named here, and it compiles.
    reg, _ = register_from(sbc, [("a.json", ".x", "external-crate", REASON)])
    said = sbc.judge(every, set(), reg)
    out.append((
        "an entry for a block that compiles is an error",
        len(said) == 1 and "compiles" in said[0],
        f"said={said}",
    ))

    # 3. An entry pointing at a block the export no longer carries.
    reg, _ = register_from(sbc, [("gone.json", ".x", "external-crate", REASON)])
    said = sbc.judge(every, {("gone.json", ".x")}, reg)
    out.append((
        "an entry for a block that no longer exists is an error",
        len(said) == 1 and "no longer in the export" in said[0],
        f"said={said}",
    ))

    # 4. The closed set of kinds. `api-missing` is precisely what must not get
    #    in: that is the thing being measured.
    _, form = register_from(sbc, [("a.json", ".x", "api-missing", REASON)])
    out.append((
        "an invented kind is refused",
        len(form) == 1 and "api-missing" in form[0],
        f"form={form}",
    ))

    # 5. Three words is not a reason. The next reader has to be able to judge
    #    whether it still holds.
    _, form = register_from(sbc, [("a.json", ".x", "external-crate", "needs a crate, skip")])
    out.append((
        "a two-word reason is refused",
        len(form) == 1 and "says too little" in form[0],
        f"form={form}",
    ))

    # 6. A missing field.
    _, form = register_from(sbc, [("a.json", ".x", "external-crate", "")])
    out.append((
        "an entry without a reason is refused",
        len(form) == 1 and "why" in form[0],
        f"form={form}",
    ))

    # 7. The same block twice, which is how a register silently disagrees with
    #    itself about why something is excused.
    _, form = register_from(sbc, [("a.json", ".x", "external-crate", REASON),
                                  ("a.json", ".x", "external-crate", REASON)])
    out.append((
        "the same block twice is refused",
        len(form) == 1 and "twice" in form[0],
        f"form={form}",
    ))

    # 8. The split by rustc error code. Mechanical, not by eye, and it carries
    #    the difference between "someone has to decide" and "fix one line".
    out.append((
        "E0433 reads as a path that does not exist",
        sbc.kind_of({"E0433"}) == "path or name does not exist",
        sbc.kind_of({"E0433"}),
    ))
    out.append((
        "E0277 reads as a name in the wrong shape",
        sbc.kind_of({"E0277"}) == "exists, but not in that shape",
        sbc.kind_of({"E0277"}),
    ))
    out.append((
        "an unknown code does not vanish into a bucket",
        sbc.kind_of({"E9999"}).startswith("other: E9999"),
        sbc.kind_of({"E9999"}),
    ))

    # 9. The cap on the register's size lives in `main()`, not in `beoordeel()`,
    #    but the constant has to exist and stay under half: a register covering
    #    the majority of the site checks nothing.
    out.append((
        "the cap on the register's size exists and is below 50%",
        0 < sbc.MAX_EXCEPTION_SHARE < 0.5,
        str(getattr(sbc, "MAX_EXCEPTION_SHARE", None)),
    ))

    return out


def the_ratchet(sbc) -> list[tuple[str, bool, str]]:
    """Which way the two counts may move.

    The ceiling is the point of the whole gate, and it is one line: a rise
    refuses. The other direction is not symmetrical and used to be, which cost
    a landing (#1738): master moves between measuring and pushing, an unrelated
    commit makes one more block compile, and a gate that demands equality
    refuses a tree that is strictly better than the one it accepted an hour
    before. A guard that refuses good news gets overridden, and an overridden
    guard checks nothing.
    """
    out = []
    ok, notes = sbc.ratchet([87, 286], [88, 286])
    out.append(("a rise in the programs refuses", not ok, "; ".join(notes)))

    ok, notes = sbc.ratchet([87, 286], [87, 287])
    out.append((
        "a rise in the blocks refuses, even when the programs hold",
        not ok,
        "; ".join(notes),
    ))

    ok, notes = sbc.ratchet([87, 286], [87, 286])
    out.append(("standing still passes, and says nothing", ok and not notes,
                "; ".join(notes) or "silent"))

    ok, notes = sbc.ratchet([87, 286], [85, 280])
    out.append((
        "a fall passes, and does not pass in silence",
        ok and len(notes) == 2,
        "; ".join(notes) or "silent",
    ))
    return out


def the_wrapper(sbc) -> list[tuple[str, bool, str]]:
    """What `wrap()` hands to rustc.

    Two properties, and both were wrong once.

    A page is one program. `sections[0].primaryCode` opens the document and
    `steps[1]` goes on using it, which is how a reader reads it. Compile the
    steps inside the first block's `fn main` and they cannot see `doc`; rustc
    answers "expected value, found built-in attribute `doc`", and on 05-09-2026
    that was 88 of the 238 blocks the report called debt. Not one of them is
    wrong on the site.

    And a block's own promise is kept. A block that declares
    `fn main() -> pdfluent::Result<()>` and then calls `std::fs::write(..)?` does
    not compile, because that error does not convert -- a real defect in an
    example somebody copies. Wrapping every block in `Result<(), Box<dyn Error>>`
    would wave it through; wrapping every block in `pdfluent::Result<()>` would
    invent the same complaint against blocks that promised `Box<dyn Error>` and
    are right. So the wrapper adopts what the block says.
    """
    out = []

    page = (
        "use pdfluent::PdfDocument;\n"
        "\n"
        "fn main() -> pdfluent::Result<()> {\n"
        "    let doc = PdfDocument::open(\"a.pdf\")?;\n"
        "    doc.save(\"b.pdf\")?;\n"
        "    Ok(())\n"
        "}\n"
        "\n"
        "let n = doc.page_count();\n"
    )
    wrapped = sbc.wrap(0, page)
    out.append((
        "a section is one scope: the steps can see what the first block opened",
        "fn main" not in wrapped and "let doc = PdfDocument::open" in wrapped
        and "let n = doc.page_count();" in wrapped,
        wrapped,
    ))
    out.append((
        "the block's own error type is what it is held to",
        "pub fn draai() -> pdfluent::Result<()> {" in wrapped,
        wrapped,
    ))

    boxed = (
        "fn main() -> Result<(), Box<dyn std::error::Error>> {\n"
        "    let bytes = std::fs::read(\"a.png\")?;\n"
        "    Ok(())\n"
        "}\n"
    )
    out.append((
        "a block promising Box<dyn Error> is not held to pdfluent::Result",
        "pub fn draai() -> Result<(), Box<dyn std::error::Error>> {"
        in sbc.wrap(0, boxed),
        sbc.wrap(0, boxed),
    ))

    # A block without a `fn main` is a loose fragment. Nothing to unfold, and
    # the default promise stays what it was.
    loose = "let text = doc.extract_text()?;\n"
    out.append((
        "a block without a fn main keeps the default promise",
        "pub fn draai() -> pdfluent::Result<()> {" in sbc.wrap(0, loose),
        sbc.wrap(0, loose),
    ))

    # Unbalanced: the closing brace is not where the shape says it is. Cutting
    # anyway would invent errors, which is the one thing this gate must not do,
    # so the block is left exactly as it stands.
    truncated = "fn main() -> pdfluent::Result<()> {\n    let doc = open()?;\n"
    out.append((
        "a fn main whose close cannot be found is left alone",
        "fn main" in sbc.wrap(0, truncated),
        sbc.wrap(0, truncated),
    ))

    # The `use` lines still come out, from inside the unfolded body as well:
    # they may not sit in a function.
    inner_use = (
        "fn main() -> pdfluent::Result<()> {\n"
        "    use pdfluent::PdfDocument;\n"
        "    let doc = PdfDocument::open(\"a.pdf\")?;\n"
        "    Ok(())\n"
        "}\n"
    )
    w = sbc.wrap(0, inner_use)
    out.append((
        "use lines are hoisted out of the unfolded body",
        w.index("use pdfluent::PdfDocument;") < w.index("pub fn draai"),
        w,
    ))
    return out


def the_real_register(sbc) -> list[tuple[str, bool, str]]:
    """The register as it is checked in, against the export as it is checked in.

    The half of the gate that needs no compiler, so it runs here rather than in
    the three-quarter-hour job: a stale entry costs one pipeline round to find
    instead of a full build. What it cannot answer is whether an excused block
    has started compiling — that takes the compiler, and it is the job's half.
    """
    out = []
    register, form = sbc.read_register(sbc.EXCEPTIONS)
    out.append((
        "the checked-in register is well-formed",
        not form,
        "; ".join(form) or "no complaints",
    ))

    if not EXPORT.exists():
        out.append((
            "the checked-in register points at blocks that exist",
            False,
            f"SKIPPED (not a pass): {EXPORT} is missing, so nothing was compared",
        ))
        return out

    raw = json.loads(EXPORT.read_text(encoding="utf-8"))
    blocks = raw["blocks"] if isinstance(raw, dict) else raw

    # FLOOR: blocks in the export >= 250 -- the same floor the gate declares.
    # Comparing the register against a handful of blocks would call almost
    # every entry stale, which sends the reader to fix the wrong file.
    out.append((
        "the export carries enough blocks to compare against",
        len(blocks) >= sbc.MIN_BLOCKS,  # FLOOR
        f"{len(blocks)} blocks",
    ))

    every = {sbc.key(b) for b in blocks}
    stale = [f"{k[0]} {k[1]}" for k in sorted(register) if k not in every]
    out.append((
        "every entry names a block the export still carries",
        not stale,
        "; ".join(stale) or "none stale",
    ))

    out.append((
        "the register stays under its cap",
        len(register) <= len(blocks) * sbc.MAX_EXCEPTION_SHARE,
        f"{len(register)} of {len(blocks)}",
    ))
    return out


def the_handover(sbc) -> list[tuple[str, bool, str]]:
    """The digest the two repositories agree on.

    The export in this repository is a copy, and a copy drifts. Between
    25-08-2026 and 03-09-2026 the site gained 17 blocks, lost 41 and rewrote 60,
    and the gate compiled none of it: it was still building the August snapshot
    and reporting a number about a site that no longer existed. Nothing went
    red, because nothing was looking.

    `handover.digest` is what closes that. The website computes the same digest
    over the blocks it is about to publish and refuses to build when it differs
    from the one recorded here, so a block cannot reach a reader before it has
    been through the compiler. That only holds while the two sides compute it
    the same way, and while the recorded value actually describes this file --
    which is what these cases hold in place.
    """
    out = []
    a = {"bestand": "a.json", "pad": ".x", "code": "let a = 1;"}
    b = {"bestand": "b.json", "pad": ".y", "code": "let b = 2;"}

    # 1. Order is not content. The export is regenerated by a glob, and a glob
    #    changes order when a file is renamed. A digest that moved with it would
    #    demand a handover for a change no reader can see, and a guard that
    #    cries wolf gets switched off.
    out.append((
        "the digest does not depend on the order of the blocks",
        sbc.digest_of([a, b]) == sbc.digest_of([b, a]),
        f"{sbc.digest_of([a, b])[:12]} vs {sbc.digest_of([b, a])[:12]}",
    ))

    # 2. THE DIRECTION THAT MATTERS: an edited example is a different digest.
    #    If this stops holding, a rewritten block reaches the site with the old
    #    handover still on it and nothing ever compiles the new text.
    edited = {**a, "code": "let a = 2;"}
    out.append((
        "editing a block's code changes the digest",
        sbc.digest_of([a, b]) != sbc.digest_of([edited, b]),
        f"{sbc.digest_of([a, b])[:12]} vs {sbc.digest_of([edited, b])[:12]}",
    ))

    # 3. And so is adding or removing one.
    out.append((
        "adding a block changes the digest",
        sbc.digest_of([a]) != sbc.digest_of([a, b]),
        f"{sbc.digest_of([a])[:12]} vs {sbc.digest_of([a, b])[:12]}",
    ))

    # 4. Fields the site adds later must not count. The export carries
    #    `onderdelen` (which source blocks a section was assembled from) and
    #    that is bookkeeping, not example text.
    with_extra = {**a, "onderdelen": [".x"]}
    out.append((
        "a field that is not compiled does not change the digest",
        sbc.digest_of([a]) == sbc.digest_of([with_extra]),
        f"{sbc.digest_of([a])[:12]} vs {sbc.digest_of([with_extra])[:12]}",
    ))

    if not EXPORT.exists():
        out.append((
            "the checked-in export carries a handover that describes it",
            False,
            f"SKIPPED (not a pass): {EXPORT} is missing",
        ))
        return out

    raw = json.loads(EXPORT.read_text(encoding="utf-8"))
    blocks = raw["blocks"] if isinstance(raw, dict) else raw
    recorded = raw.get("handover") if isinstance(raw, dict) else None

    # 5. The checked-in copy has to carry one at all. Without it the gate falls
    #    back to compiling whatever is in the file, which is the state this
    #    whole mechanism exists to leave behind.
    out.append((
        "the checked-in export carries a handover",
        bool(recorded and recorded.get("digest") and recorded.get("website_commit")),
        str(recorded and {k: v for k, v in recorded.items() if k != "digest"}),
    ))
    if not recorded:
        return out

    # 6. And it has to describe this file. A hand-edited block here is the quiet
    #    way to make the ratchet green over code no visitor is shown.
    actual = sbc.digest_of(blocks)
    out.append((
        "the recorded digest is the digest of the blocks in the file",
        recorded.get("digest") == actual,
        f"recorded {str(recorded.get('digest'))[:12]}, actual {actual[:12]}",
    ))
    out.append((
        "the recorded block count is the count in the file",
        recorded.get("blocks") == len(blocks),
        f"recorded {recorded.get('blocks')}, actual {len(blocks)}",
    ))
    return out


def main() -> int:
    if not GATE.exists():
        print(f"SKIPPED (not a pass): {GATE} is missing", file=sys.stderr)
        return 1

    sbc = load()
    results = (cases(sbc) + the_ratchet(sbc) + the_wrapper(sbc)
               + the_real_register(sbc) + the_handover(sbc))

    # FLOOR: cases >= 20 -- this file builds its own cases, so an empty or
    # halved list is not a clean tree but a gutted file, and zero cases that
    # all pass reads as green.
    if len(results) < 20:
        print(
            f"[site-blocks-register] only {len(results)} cases. This file "
            f"builds them itself, so that is not a clean tree.",
            file=sys.stderr,
        )
        return 1

    for name, ok, _ in results:
        print(f"  {'ok  ' if ok else 'FAIL'}  {name}")

    failed = [(n, t) for n, ok, t in results if not ok]
    if failed:
        print(
            "\n[site-blocks-register] the register is no longer read in both "
            "directions:\n",
            file=sys.stderr,
        )
        for name, got in failed:
            print(f"  - {name}\n      got: {got}", file=sys.stderr)
        print(
            "\n  An exception list consulted only when something fails is a "
            "list that only grows.\n  The direction that usually goes missing "
            "is 'named here, and it compiles'.",
            file=sys.stderr,
        )
        return 1

    print(f"[site-blocks-register] {len(results)} cases, all good")
    return 0


if __name__ == "__main__":
    sys.exit(main())
