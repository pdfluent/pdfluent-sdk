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
import inspect
import json
import os
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


def the_compiler_wrapper(sbc) -> list[tuple[str, bool, str]]:
    """The count may not depend on who is measuring.

    Measured 09-09-2026 on one commit: 87 non-compiling programs on the
    operator's Mac, 97 on a bare Linux instance -- same rustc 1.94.0, same
    lockfile, same blocks. The Mac's cargo config sets sccache as RUSTC_WRAPPER;
    sccache invokes rustc with `-o`, and that run stops after name resolution,
    so every E0599 and E0277 in the same crate was never emitted. Ten programs
    calling methods that exist nowhere in this workspace were counted as
    compiling, and the recorded ratchet was ten too low for as long as the
    baseline had been taken on a machine with a cache.

    So `build()` clears the wrapper, and this asserts it does, by running the
    real function against a fake `cargo` that reports the environment it was
    given. Reading the source for `RUSTC_WRAPPER` would pass a function that
    computes the right environment and then hands cargo another one -- and a
    cache that can change a measurement is precisely the thing that looks fine
    until two machines disagree. (#474)
    """
    out = []
    with tempfile.TemporaryDirectory() as d:
        work = Path(d)
        (work / "src").mkdir()
        nep = work / "bin"
        nep.mkdir()
        (nep / "cargo").write_text(
            "#!/usr/bin/env bash\n"
            'printf "wrapper=[%s] build_wrapper=[%s]\\n" '
            '"${RUSTC_WRAPPER-UNSET}" "${CARGO_BUILD_RUSTC_WRAPPER-UNSET}" >&2\n'
            "exit 1\n"
        )
        (nep / "cargo").chmod(0o755)
        pad, wrapper = os.environ.get("PATH", ""), os.environ.get("RUSTC_WRAPPER")
        os.environ["PATH"] = f"{nep}:{pad}"
        os.environ["RUSTC_WRAPPER"] = "sccache"
        try:
            _code, stderr = sbc.build(work, ["fn f() {}\n"])
        finally:
            os.environ["PATH"] = pad
            if wrapper is None:
                os.environ.pop("RUSTC_WRAPPER", None)
            else:
                os.environ["RUSTC_WRAPPER"] = wrapper
    out.append(("a compiler wrapper in the environment does not reach cargo",
                "wrapper=[UNSET]" in stderr, stderr.strip()[:160]))
    out.append(("and neither does cargo's own spelling of it",
                "build_wrapper=[]" in stderr or "build_wrapper=[UNSET]" in stderr,
                stderr.strip()[:160]))
    return out


def the_build_directory(sbc) -> list[tuple[str, bool, str]]:
    """Where this gate builds, and what it leaves behind.

    A leak that costs nothing per run is invisible until it is somebody else's
    failure. This one built the throwaway crate in a fresh `mkdtemp` and never
    removed it: two gigabytes each, forty-five of them in $TMPDIR by
    05-09-2026, free disk from 72 GB to 35 GB in three hours, and every push in
    the house stopped at the pre-push floor (#344). Nothing was red anywhere.

    So the property is stated the way the fault was found: after this gate has
    run, $TMPDIR holds exactly what it held before. Both paths are exercised --
    the warm directory in the checkout, and the fallback for a checkout that
    cannot be written to -- and the fallback is exercised twice, because the
    return path and the raising path are different code and only one of them
    used to be written down.

    $TMPDIR is pointed at a private directory for the duration. Three terminals
    share this machine and write temporary files while this runs; comparing the
    real one before and after would be a coin toss, and a flaky guard is a
    guard that gets switched off.
    """
    out = []

    def with_private_tmp(work):
        """Run `work(private)`, with $TMPDIR pointed at a directory of our own."""
        keep_dir, keep_env = tempfile.tempdir, os.environ.get("TMPDIR")
        private = Path(tempfile.mkdtemp(prefix="siteblocks-test-"))
        try:
            tempfile.tempdir = str(private)
            os.environ["TMPDIR"] = str(private)
            return work(private)
        finally:
            tempfile.tempdir = keep_dir
            if keep_env is None:
                os.environ.pop("TMPDIR", None)
            else:
                os.environ["TMPDIR"] = keep_env
            for child in private.rglob("*"):
                if child.is_file():
                    child.unlink()
            for child in sorted(private.rglob("*"), reverse=True):
                if child.is_dir():
                    child.rmdir()
            private.rmdir()

    # 1 and 2. The warm directory: in the checkout being measured, and the same
    #          one every time. Not merely tidy -- five hundred blocks against a
    #          cold facade is tens of minutes, and that is the whole reason a
    #          per-run directory was expensive as well as leaky.
    def warm_case(private):
        repo = private / "checkout"
        repo.mkdir()
        with sbc.build_workdir(repo) as first:
            (first / "cached").write_text("a build")
            first_seen = first
        after_first = sorted(os.listdir(private))
        with sbc.build_workdir(repo) as second:
            reused = second == first_seen and (second / "cached").is_file()
        return first_seen, after_first, reused

    first_seen, after_first, reused = with_private_tmp(warm_case)
    out.append((
        "the build directory sits in the checkout, not in $TMPDIR",
        first_seen.parts[-2:] == ("target", "site-blocks")
        and after_first == ["checkout"],
        f"{first_seen} left {after_first} in $TMPDIR",
    ))
    out.append((
        "the same checkout gets the same directory back, still warm",
        reused,
        f"reused={reused}",
    ))

    # 3 and 4. The fallback, for a checkout that cannot be written to. It is
    #          the old behaviour, and it has to disappear again -- on the way
    #          out through a return and on the way out through an exception.
    #          `main()` has eight ways to return one, which is how the removal
    #          got left off in the first place.
    def fallback_case(private):
        before = sorted(os.listdir(private))
        with sbc.build_workdir(Path(os.devnull)) as temp:
            inside, name = temp.is_dir(), temp.name
        return before, sorted(os.listdir(private)), inside, name, temp

    before, after, inside, name, temp = with_private_tmp(fallback_case)
    out.append((
        "a checkout that cannot be written to falls back to a temp directory",
        inside and name.startswith("siteblocks-"),
        f"{name} existed={inside}",
    ))
    out.append((
        "the fallback leaves $TMPDIR as it found it when the gate returns",
        before == after and not temp.exists(),
        f"before={before} after={after} still there={temp.exists()}",
    ))

    def raising_case(private):
        before = sorted(os.listdir(private))
        held = None
        raised = False
        try:
            with sbc.build_workdir(Path(os.devnull)) as temp:
                held = temp
                raise RuntimeError("cargo fell over")
        except RuntimeError:
            raised = True
        return before, sorted(os.listdir(private)), held, raised

    before, after, held, raised = with_private_tmp(raising_case)
    out.append((
        "the fallback leaves $TMPDIR as it found it when the gate raises",
        raised and before == after and held is not None and not held.exists(),
        f"raised={raised} before={before} after={after}",
    ))

    # 5. And the gate goes through it. Building a directory of its own next to
    #    the context manager would pass every case above and leak exactly as
    #    before, so the two functions that do the work are held to using it.
    source = inspect.getsource(sbc.main) + inspect.getsource(sbc.compile_the_blocks)
    out.append((
        "the gate itself takes its build directory from build_workdir",
        "mkdtemp" not in source and "build_workdir(" in source,
        source[:200],
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

    # 4. The page's status is not bookkeeping, and it is in the digest. It
    #    decides whether a failing program is refused outright or counted as
    #    debt, so publishing a hidden page after the hand-over would switch the
    #    strict half off with nothing hashing differently (#499).
    published = {**a, "status": None}
    hidden = {**a, "status": "hidden"}
    out.append((
        "publishing a page changes the digest",
        sbc.digest_of([published, b]) != sbc.digest_of([hidden, b]),
        f"{sbc.digest_of([published, b])[:12]} vs {sbc.digest_of([hidden, b])[:12]}",
    ))

    # 5. Fields the site adds later must not count. The export carries
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

    # 6. The checked-in copy has to carry one at all. Without it the gate falls
    #    back to compiling whatever is in the file, which is the state this
    #    whole mechanism exists to leave behind.
    out.append((
        "the checked-in export carries a handover",
        bool(recorded and recorded.get("digest") and recorded.get("website_commit")),
        str(recorded and {k: v for k, v in recorded.items() if k != "digest"}),
    ))
    if not recorded:
        return out

    # 7. And it has to describe this file. A hand-edited block here is the quiet
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



GUIDE = "public/content/en/how-to-guides/pdfa.json"
OTHER_GUIDE = "public/content/en/how-to/split-pdf-rust.json"
ERROR_PAGE = "public/content/en/errors/itext-out-of-memory.json"
NOT_A_GUIDE = "public/content/en/solutions/pdf-a-conversion.json"

DEBT_ENTRY = 'file  = "{f}"\npath  = "{p}"\nerror = "{e}"\n'


def debt_from(sbc, entries: list[tuple[str, str, str]]):
    """Write a debt list and read it back through the real parser."""
    text = "\n".join("[[program]]\n" + DEBT_ENTRY.format(f=f, p=p, e=e)
                     for f, p, e in entries)
    with tempfile.NamedTemporaryFile("w", suffix=".toml", delete=False) as fh:
        fh.write(text)
        path = Path(fh.name)
    try:
        return sbc.read_debt(path)
    finally:
        path.unlink()


def the_published_guides(sbc) -> list[tuple[str, bool, str]]:
    """A broken example on a served how-to page is refused, not counted.

    The ratchet holds one ceiling over the whole site, and a ceiling cannot see
    who carries the damage. `how-to-guides/pdfa.json` showed `fix_pdf_a_errors`
    and `PdfAFixOptions` -- neither exists in any crate -- for months on a page
    that was in the sitemap. Three broken programs inside a ceiling of 97: fix
    something anywhere else and there is room for a new invented API here, so
    the number never moved and nothing was ever red (#499).

    The judgement is in `judge_guides`, apart from the build, for the same
    reason as `judge`: compiling five hundred programs takes three quarters of
    an hour and runs on one runner, so a rule that only exists inside that build
    is a rule nobody has tried to defeat. All four of its directions are
    exercised here on synthetic input.
    """
    out = []
    every = {(GUIDE, ".x"), (OTHER_GUIDE, ".y"), (NOT_A_GUIDE, ".z")}
    published = {(GUIDE, ".x"), (OTHER_GUIDE, ".y")}

    # 1. THE POINT OF THE FILE: a published guide program that does not compile
    #    and is not written down. This is the case that was green for months.
    debt, form = debt_from(sbc, [])
    said = sbc.judge_guides(every, published, {(GUIDE, ".x")}, debt)
    out.append((
        "a broken example on a published how-to page is refused outright",
        not form and len(said) == 1 and "published" in said[0] and GUIDE in said[0],
        f"form={form} said={said}",
    ))

    # 2. The same failure on a page nobody is served costs a reader nothing.
    #    That is debt, and the ratchet is the right instrument for it -- a
    #    complaint here would push whoever reads it towards hiding pages.
    said = sbc.judge_guides(every, published, {(NOT_A_GUIDE, ".z")}, debt)
    out.append((
        "the same failure outside the published guides is left to the ratchet",
        said == [],
        f"said={said}",
    ))

    # 3. Written down, and it compiles: the entry has lapsed. Without this the
    #    list only grows, and a list that only grows is the ceiling again.
    debt, _ = debt_from(sbc, [(GUIDE, ".x", "no method named `fix_pdf_a_errors`")])
    said = sbc.judge_guides(every, published, set(), debt)
    out.append((
        "an entry for a program that compiles is an error",
        len(said) == 1 and "compiles" in said[0],
        f"said={said}",
    ))

    # 4. Written down, and the page is no longer published. Hiding the page is
    #    a repair the gate accepts -- it costs the page -- but the entry has to
    #    go with it, or the next broken example there is waved through.
    said = sbc.judge_guides(every, {(OTHER_GUIDE, ".y")}, {(GUIDE, ".x")}, debt)
    out.append((
        "an entry for a page that is no longer published is an error",
        len(said) == 1 and "published guide" in said[0],
        f"said={said}",
    ))

    # 5. Written down, and gone from the export.
    debt, _ = debt_from(sbc, [("public/content/en/how-to/gone.json", ".x", "e")])
    said = sbc.judge_guides(every, published, set(), debt)
    out.append((
        "an entry for a program the export no longer carries is an error",
        len(said) == 1 and "no longer in the export" in said[0],
        f"said={said}",
    ))

    # 6. An entry has to say what went wrong, or the list cannot be read by the
    #    next person and gets renewed on autopilot.
    _, form = debt_from(sbc, [(GUIDE, ".x", "")])
    out.append((
        "an entry without the compiler's complaint is refused",
        len(form) == 1 and "error" in form[0],
        f"form={form}",
    ))

    # 7. The same program twice is how a list silently disagrees with itself.
    _, form = debt_from(sbc, [(GUIDE, ".x", "e"), (GUIDE, ".x", "e")])
    out.append((
        "the same program twice is refused",
        len(form) == 1 and "twice" in form[0],
        f"form={form}",
    ))

    # 8-10. Which programs the rule applies to. `hidden` is the website's own
    #       word for "not in the sitemap and not routed"; every other status,
    #       and no status at all, means served.
    out.append((
        "a page under how-to-guides with no status is a published guide",
        sbc.is_published_guide({"bestand": GUIDE, "pad": ".x", "status": None}),
        "",
    ))
    out.append((
        "a hidden page is not a published guide",
        not sbc.is_published_guide(
            {"bestand": OTHER_GUIDE, "pad": ".x", "status": "hidden"}),
        "",
    ))
    out.append((
        "a page outside the two guide trees is not a published guide",
        not sbc.is_published_guide(
            {"bestand": NOT_A_GUIDE, "pad": ".x", "status": "core-1.0"}),
        "",
    ))

    # 11. The floor under the whole rule. Lose the status field, move the
    #     directories, and the set goes empty: every entry then reads as "no
    #     longer published", the strict half has nothing to refuse, and the gate
    #     says green while checking nothing.
    out.append((
        "there is a floor under the number of published guide programs",
        getattr(sbc, "MIN_PUBLISHED_GUIDE_PROGRAMS", 0) >= 50,
        str(getattr(sbc, "MIN_PUBLISHED_GUIDE_PROGRAMS", None)),
    ))

    # 16. The error tree has a floor of its own, because the one over both
    #     trees cannot see it: the how-to trees alone clear 110, so an export
    #     that stops emitting error pages passes the combined floor. The real
    #     export is checked against both floors in the_real_debt below; this
    #     case is the arithmetic that makes the second one necessary.
    out.append((
        "the combined floor cannot see the error tree disappear",
        (getattr(sbc, "MIN_PUBLISHED_ERROR_PROGRAMS", 0) >= 5
         and sbc.MIN_PUBLISHED_GUIDE_PROGRAMS
         < 121 + sbc.MIN_PUBLISHED_ERROR_PROGRAMS),
        f"guides={sbc.MIN_PUBLISHED_GUIDE_PROGRAMS} "
        f"errors={getattr(sbc, 'MIN_PUBLISHED_ERROR_PROGRAMS', None)}",
    ))

    # 15. The list may only shrink, and until #1885's review nothing said so:
    #     the four directions above all pass on an entry added in the same
    #     commit that breaks the program, and the site-wide ratchet can stay
    #     level because something else was repaired. The ceiling is a number in
    #     site_blocks_compile.py, so growing the list means editing that line
    #     where a reviewer sees it.
    many = [(f"public/content/en/errors/p{i}.json", "(page) (1 blokken)", "e")
            for i in range(sbc.MAX_DEBT_ENTRIES + 1)]
    big, _ = debt_from(sbc, many)
    said = sbc.judge_guides(set(big), set(), set(), big)
    out.append((
        "a debt list over the ceiling is refused",
        any("ceiling" in s for s in said),
        f"entries={len(big)} ceiling={sbc.MAX_DEBT_ENTRIES}",
    ))

    # 12-14. The error pages are judged the same way (#476).
    #
    #        A reader arrives on one of them with a stack trace and pastes the
    #        fix; `errors/itext-out-of-memory.json` and
    #        `errors/pdfbox-slow-performance.json` both applied `?` to a `Page`,
    #        which is not a `Result`, and both sat inside the ceiling from the
    #        day it was drawn -- repair a how-to elsewhere and there was room
    #        for them.
    #
    #        Drop `errors/` out of the trees above and 12 and 13 go red, which
    #        is the point of writing them down here rather than trusting the
    #        tuple to stay as it is.
    out.append((
        "a page under errors/ is judged like a how-to",
        sbc.is_published_guide(
            {"bestand": ERROR_PAGE, "pad": ".fixes[2].code", "status": "core-1.0"}),
        "",
    ))
    # The published set is derived the way `main` derives it -- through
    # `is_published_guide` -- so that this case is about the trees and not
    # about a set written out by hand here.
    export = [
        {"bestand": GUIDE, "pad": ".x", "status": None},
        {"bestand": ERROR_PAGE, "pad": ".fixes[2].code", "status": "core-1.0"},
    ]
    every_with_error = {(b["bestand"], b["pad"]) for b in export}
    published_with_error = {
        (b["bestand"], b["pad"]) for b in export if sbc.is_published_guide(b)}
    debt, _ = debt_from(sbc, [])
    said = sbc.judge_guides(
        every_with_error, published_with_error, {(ERROR_PAGE, ".fixes[2].code")}, debt)
    out.append((
        "a broken fix on a published error page is refused outright",
        len(said) == 1 and ERROR_PAGE in said[0] and "published" in said[0],
        f"said={said}",
    ))
    out.append((
        "a hidden error page is left to the ratchet",
        not sbc.is_published_guide(
            {"bestand": ERROR_PAGE, "pad": ".fixes[2].code", "status": "hidden"}),
        "",
    ))
    return out


def the_real_debt(sbc) -> list[tuple[str, bool, str]]:
    """The debt list as it is checked in, against the export as it is checked in.

    Two of the four directions need no compiler -- an entry naming a program the
    export lost, and an entry naming a page that is no longer published -- so
    they run on every pull request here instead of in the three-quarter-hour
    job. The third (it compiles now) is the job's half.
    """
    out = []
    debt, form = sbc.read_debt(sbc.GUIDES_DEBT)
    out.append((
        "the checked-in debt list is well-formed",
        not form,
        "; ".join(form) or "no complaints",
    ))

    if not EXPORT.exists():
        out.append((
            "the checked-in debt list points at published guide programs",
            False,
            f"SKIPPED (not a pass): {EXPORT} is missing, so nothing was compared",
        ))
        return out

    raw = json.loads(EXPORT.read_text(encoding="utf-8"))
    blocks = raw["blocks"] if isinstance(raw, dict) else raw
    every = {sbc.key(b) for b in blocks}
    published = {sbc.key(b) for b in blocks if sbc.is_published_guide(b)}

    out.append((
        "the export carries enough published guide programs to judge",
        len(published) >= sbc.MIN_PUBLISHED_GUIDE_PROGRAMS,  # FLOOR
        f"{len(published)} of {len(blocks)}",
    ))

    # Nothing in the export may be counted twice: a program excused as a cloud
    # recipe in blocks_exceptions.toml is not debt, and an entry in both files
    # would leave one of them stale as soon as the other is repaired.
    register, _ = sbc.read_register(sbc.EXCEPTIONS)
    both = sorted(f"{k[0]} {k[1]}" for k in set(debt) & set(register))
    out.append((
        "no program is in the exception register and the debt list at once",
        not both,
        "; ".join(both) or "none",
    ))

    said = [c for c in sbc.judge_guides(every, published, set(debt), debt)]
    out.append((
        "every entry names a published guide program the export still carries",
        not said,
        "; ".join(said) or "none stale",
    ))
    return out


def main() -> int:
    if not GATE.exists():
        print(f"SKIPPED (not a pass): {GATE} is missing", file=sys.stderr)
        return 1

    sbc = load()
    results = (cases(sbc) + the_ratchet(sbc) + the_wrapper(sbc)
               + the_compiler_wrapper(sbc)
               + the_build_directory(sbc) + the_real_register(sbc)
               + the_published_guides(sbc) + the_real_debt(sbc)
               + the_handover(sbc))

    # FLOOR: cases >= 35 -- this file builds its own cases, so an empty or
    # halved list is not a clean tree but a gutted file, and zero cases that
    # all pass reads as green.
    if len(results) < 35:
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
