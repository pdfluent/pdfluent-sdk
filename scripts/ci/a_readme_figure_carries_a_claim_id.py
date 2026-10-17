#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""A figure in a README carries a claim ID, and no README sells the withdrawn
model (#358, precondition 4 of the 1.0.0 release).

WHY THIS IS A GATE

The claims register (CLAIMS.md, in the internal repository) has one rule: a
number that goes out carries an ID that points at a run, a commit and a date.
The READMEs are where that rule was not being applied. Measured on 07-09-2026,
before this guard existed: two crate READMEs still said "0 fails on the 20K-PDF
corpus", one gave a median SSIM against a benchmark nobody could find, one said
"2.7x faster", one said "286/300 on macOS and 275/300 on Debian" where the
register says 300/300 on Linux, and the crates.io README opened with "Free for
evaluation. A valid license is required for production use" -- a model
withdrawn on 25-08-2026 (#199) and removed from the code on 06-09-2026 (#226).
The NuGet and Java READMEs documented a `Licensing` class that no longer exists.

Nothing failed, because nothing was reading the READMEs. This does.

WHAT IT CHECKS, on every README a registry renders or a stranger lands on

  figures     a line of prose (outside a fenced code block) that carries a
              percentage, a ratio such as 286/300, a "2.7x faster", an
              "N documents / N-PDF corpus", a size in KB/MB/GB, a duration in
              ms/seconds/minutes, or an "N+ things" count must also carry a
              claim ID, written `claim A15` (any letter A-E, two digits), in
              the same paragraph. The ID's existence in CLAIMS.md is not checked here -- that file
              lives in the internal repository -- but a figure with no ID at
              all is exactly the kind that was drifting.
  the offer   no README offers the withdrawn model: "free for evaluation",
              "production use requires a licence", an evaluation key or period,
              licence activation, trial mode, tiers, per-developer or per-seat
              pricing, the OEM Redistribution add-on, a retired price, the old
              /pricing and /trial routes, or the words "commercial SDK",
              "Enterprise PDF SDK", "enterprise-grade", "production-grade".
              Saying the model does NOT exist ("no licence key, no activation
              call and no tier") is allowed; that sentence is what a reader
              needs.
  the seam    every README that states the licence states it as AGPL-3.0 or a
              commercial licence, and points at the pricing page that sells it
              (https://pdfluent.com/sdk/pricing), so the offer is not spelled a
              second way in a place the offer guard cannot see.
  internal    no internal programme name (Wave 2, Track G, G1, Round 1), no
              internal tracker reference (#358), no link to pdfluent-internal.
              Those mean nothing to a reader of a registry page and they name
              things that are not public.

The metadata descriptions the five registries render beside the README
(Cargo.toml, pyproject.toml, package.json, the .csproj, the pom, and the
description the wasm transform writes) go through the offer and figure checks
too: a description is the first sentence a visitor reads.

FLOORS, because a scan that reads nothing approves everything: at least 30
READMEs in scope, and at least one claim ID somewhere in them, so the path that
recognises an ID is exercised on every run.

Usage:
    python3 scripts/ci/a_readme_figure_carries_a_claim_id.py

Exit codes:
    0  every figure carries an ID and no README sells the withdrawn model
    1  it does not, and it says where
"""
from __future__ import annotations

import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
PUBLIC_REPO = "https://github.com/pdfluent/pdfluent-sdk"
PRICING = "https://pdfluent.com/sdk/pricing"
MINIMUM_READMES = 30
MINIMUM_CLAIM_IDS = 1

# READMEs that are not published crates but are rendered by a registry, or are
# the landing page of the public repository.
ALWAYS = (
    "README.md",
    "crates/pdf-python/README.md",
    "crates/pdf-node/README.md",
    "crates/xfa-wasm/README.md",
    "bindings/dotnet/src/PDFluent/README.md",
    "bindings/java/README.md",
)

# Where the five registries read the one-line description from, and how.
DESCRIPTIONS: dict[str, tuple[str, str]] = {
    "crates.io":  ("crates/pdfluent/Cargo.toml",              r'^description\s*=\s*"([^"\n]*)"'),
    "PyPI":       ("crates/pdf-python/pyproject.toml",        r'^description\s*=\s*"([^"\n]*)"'),
    "npm node":   ("crates/pdf-node/package.json",            "json:description"),
    "NuGet":      ("bindings/dotnet/src/PDFluent/PDFluent.csproj", r"(?s)<Description>(.*?)</Description>"),
    "Maven":      ("bindings/java/pom.xml",                   r"(?s)<description>(.*?)</description>"),
    "npm wasm":   ("scripts/release/transform-wasm-pkg.sh",   r'd\["description"\]\s*=\s*"([^"\n]*)"'),
    "wasm crate": ("crates/xfa-wasm/Cargo.toml",              r'^description\s*=\s*"([^"\n]*)"'),
}

CLAIM_ID = re.compile(r"\bclaim [A-E]\d{2}\b", re.IGNORECASE)

FIGURES: list[tuple[str, re.Pattern[str]]] = [
    # `%20` in a share URL is an escape, not a percentage: a percent sign followed
    # by two hex digits is skipped.
    ("a percentage", re.compile(r"\d+([.,]\d+)?\s?%(?![0-9A-Fa-f]{2})")),
    ("a ratio", re.compile(r"(?<![\w./-])\d+/\d+(?![\w/])")),
    ("a speed factor", re.compile(r"\d+([.,]\d+)?\s?(×|x\s+(faster|slower|smaller|larger|less|more))")),
    ("a document count", re.compile(
        r"\b\d[\d,.]*\s?[Kk]?[- ](?:real-world\s)?(?:PDF|document|page|file)s?\b")),
    ("a size", re.compile(r"\b\d[\d.,]*\s?(?:KB|MB|GB|KiB|MiB|GiB)\b")),
    ("a duration", re.compile(r"\b\d[\d.,]*\s?(?:ms|milliseconds|seconds|minutes)\b")),
    ("an N+ count", re.compile(r"\b\d+\+\s+[a-z]")),
    ("an SSIM value", re.compile(r"\bSSIM\s*[~≥>=]*\s*\d")),
]

OFFER: list[tuple[str, re.Pattern[str]]] = [
    ("the withdrawn evaluation model", re.compile(
        r"free for evaluation|evaluation (?:key|period|licen[cs]e)|\b\d+-day\b", re.I)),
    ("a licence requirement for production use", re.compile(
        r"requires? a (?:valid )?(?:PDFluent )?(?:commercial )?licen[cs]e|"
        r"licen[cs]e is required", re.I)),
    ("licence activation", re.compile(
        r"activat(?:e|ing) (?:a|the|your) licen[cs]e|licen[cs]e activation|"
        r"Licensing\.Activate|activateKey|ActivateKey|PDFLUENT_LICENSE_KEY", re.I)),
    ("trial mode", re.compile(r"\btrial mode\b|pdfluent\.com/trial\b", re.I)),
    ("tiers", re.compile(
        r"tiered pricing|paid-tier|Lite to Unlimited|\btier:[a-z]|effectiveTier|EffectiveTier", re.I)),
    ("per-developer or per-seat pricing", re.compile(r"per[- ](?:developer|seat)\b", re.I)),
    ("the OEM add-on", re.compile(r"OEM Redistribution add-on", re.I)),
    ("a retired price", re.compile(
        r"(?:€|EUR)\s?(?:699|1[.,]?099|2[.,]?199|4[.,]?399|12[.,]?000)\b")),
    ("the old pricing route", re.compile(r"pdfluent\.com/pricing\b")),
    ("a grade nobody measured", re.compile(
        r"commercial (?:Rust )?(?:PDF )?SDK|Enterprise PDF SDK|enterprise-grade|production-grade", re.I)),
]

INTERNAL: list[tuple[str, re.Pattern[str]]] = [
    ("an internal programme name", re.compile(
        # CCITT Group 3 and Group 4 are written G3 / G4 in the fax decoders; those
        # are standards, not programmes.
        r"\bWave \d\b|\bTrack [A-Z]\b|(?<!CCITT )(?<!Group )(?<!/ )\bG[1-9]\b|\bRound [1-9]\b")),
    ("an internal tracker reference", re.compile(r"(?<![\w/])#\d{3,4}\b|pdfluent-internal")),
]

LICENCE_LINE = re.compile(r"AGPL-3\.0 or a commercial licence", re.I)


def shipped_readmes(root: pathlib.Path) -> list[str]:
    """Every crate README a registry renders, plus the landing page."""
    files: list[str] = list(ALWAYS)
    for manifest in sorted(root.glob("crates/*/Cargo.toml")):
        text = manifest.read_text(encoding="utf-8", errors="replace")
        if not re.search(r"^readme\s*=", text, re.M):
            continue
        if re.search(r"^publish\s*=\s*false", text, re.M):
            continue
        rel = str(manifest.parent.relative_to(root) / "README.md")
        if rel not in files:
            files.append(rel)
    return files


def prose_lines(text: str):
    """(line number, line) for every line outside a fenced code block."""
    fenced = False
    for number, line in enumerate(text.splitlines(), 1):
        if line.strip().startswith("```"):
            fenced = not fenced
            continue
        if not fenced:
            yield number, line


def paragraphs(text: str):
    """Runs of consecutive non-blank prose lines, as [(number, line), ...].

    A claim ID covers the paragraph it is written in, because a measurement is
    usually two sentences -- the corpus on one line, the result wrapped onto
    the next -- and asking for the ID on each of them would make the README
    worse in order to make the guard simpler.
    """
    run: list[tuple[int, str]] = []
    for number, line in prose_lines(text):
        if line.strip():
            run.append((number, line))
        elif run:
            yield run
            run = []
    if run:
        yield run


def judge_text(text: str, name: str, *, states_licence: bool = True) -> tuple[list[str], int]:
    """Every complaint about one README, and how many claim IDs it carries."""
    problems: list[str] = []
    ids = 0
    for paragraph in paragraphs(text):
        covered = any(CLAIM_ID.search(line) for _, line in paragraph)
        if covered:
            ids += 1
        for number, line in paragraph:
            if not covered:
                for what, pattern in FIGURES:
                    if pattern.search(line):
                        problems.append(
                            f"{name}:{number}: carries {what} with no claim ID -- "
                            f"`{line.strip()[:120]}`")
                        break
            for what, pattern in OFFER:
                if pattern.search(line):
                    problems.append(f"{name}:{number}: offers {what} -- `{line.strip()[:120]}`")
            for what, pattern in INTERNAL:
                if pattern.search(line):
                    problems.append(f"{name}:{number}: names {what} -- `{line.strip()[:120]}`")
    if states_licence:
        mentions = re.search(r"licen[cs]e", text, re.I)
        if mentions and not LICENCE_LINE.search(text):
            problems.append(
                f"{name}: talks about the licence but never states it as "
                f"'AGPL-3.0 or a commercial licence'")
        if mentions and PRICING not in text:
            problems.append(
                f"{name}: talks about the licence but does not link {PRICING}, "
                f"where the commercial one is sold")
    return problems, ids


def owns_licence(root: pathlib.Path, rel: str) -> bool:
    """Whether this README speaks for a crate PDFluent licenses.

    A forked crate under Apache-2.0 OR MIT must NOT state the dual offer --
    `a_forked_readme_sends_you_upstream.py` fails it if it does -- so the seam
    check reads the manifest beside the README and asks only the crates whose
    `license` field carries the commercial reference. Everything outside
    `crates/` (the landing page, the two binding READMEs) is ours.
    """
    if not rel.startswith("crates/"):
        return True
    manifest = root / pathlib.Path(rel).parent / "Cargo.toml"
    if not manifest.is_file():
        return True
    text = manifest.read_text(encoding="utf-8", errors="replace")
    m = re.search(r'^license\s*=\s*"(.*)"', text, re.M)
    return bool(m and "LicenseRef-PDFluent-Commercial" in m.group(1))


def read_description(root: pathlib.Path, path: str, how: str) -> str | None:
    p = root / path
    if not p.is_file():
        return None
    text = p.read_text(encoding="utf-8", errors="replace")
    if how == "json:description":
        return json.loads(text).get("description")
    m = re.search(how, text, re.M)
    return m.group(1).strip() if m else None


def judge(root: pathlib.Path = ROOT, files: list[str] | None = None,
          descriptions: dict[str, tuple[str, str]] | None = None) -> list[str]:
    """Every complaint about one tree. Empty when every README is clean."""
    shipped = files is None
    files = shipped_readmes(root) if shipped else files
    descriptions = DESCRIPTIONS if descriptions is None else descriptions
    problems: list[str] = []
    ids = 0
    seen = 0

    for rel in files:
        p = root / rel
        if not p.is_file():
            problems.append(f"{rel}: is missing, and a registry renders it")
            continue
        seen += 1
        found, n = judge_text(p.read_text(encoding="utf-8", errors="replace"), rel,
                              states_licence=owns_licence(root, rel))
        problems.extend(found)
        ids += n

    for channel, (path, how) in descriptions.items():
        text = read_description(root, path, how)
        if text is None:
            problems.append(f"{channel}: no description found in {path}")
            continue
        found, _ = judge_text(text, f"{path} ({channel} description)", states_licence=False)
        problems.extend(found)

    if shipped:
        if seen < MINIMUM_READMES:
            problems.append(
                f"only {seen} README(s) in scope, below the floor of {MINIMUM_READMES}: "
                f"the register has been emptied rather than satisfied")
        if ids < MINIMUM_CLAIM_IDS:
            problems.append(
                f"no README carries a claim ID at all, so the path that recognises "
                f"one has not run; the floor is {MINIMUM_CLAIM_IDS}")
    return problems


def main() -> int:
    problems = judge()
    files = shipped_readmes(ROOT)
    if not problems:
        print(f"[readme-claims] OK: {len(files)} README(s) and "
              f"{len(DESCRIPTIONS)} registry description(s) carry no figure "
              f"without a claim ID and sell nothing but AGPL-3.0 or a commercial "
              f"licence.")
        return 0
    for p in problems:
        print(f"[readme-claims] FATAL: {p}", file=sys.stderr)
    print(f"[readme-claims] {len(problems)} problem(s). A figure in a README is "
          f"a published claim; it carries an ID from the claims register or it "
          f"does not go out.", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
