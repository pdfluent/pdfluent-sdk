#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The files a stranger meets are there, say what they must, and travel (#233).

WHAT WENT WRONG WITHOUT IT

Measured on all four public repositories on 31-08-2026: zero CODEOWNERS, zero
pull-request templates, and a CONTRIBUTING that named the DCO nowhere while
#223 had already decided the DCO was the route in. Every one of those is a file
somebody meant to write. None of them is a file anything reads, so each went
missing quietly and stayed missing for weeks.

The half that is easy to miss is the third question. This repository is seeded
into the public one by `scripts/release/seed_public_repo.sh`, which strips every
path `docs/PUBLIC_TREE.toml` calls internal -- over the whole history, not only
the tip. So a contribution file is not published because it exists here; it is
published because it is not on that list. And the same applies to what it points
at: `SECURITY.md` described the fuzz schedule by naming `.gitlab-ci.yml`, which
the seed strips, so the published copy sent a reader to a file that is not there.

WHAT IT ASKS, IN ORDER

  1. Does every file in the register exist?                            FATAL
  2. Does each say the thing it exists to say?                         FATAL
  3. Does each travel with the seed, and does each path it names?      FATAL

Question 2 is spelled as "this phrase, in this file" rather than as a summary,
because a check that only counts bytes passes a template somebody emptied.

# FLOOR: the register must hold >= 6 files. A register that has quietly emptied
# approves every file in it, which is none.
"""
from __future__ import annotations

import pathlib
import re
import sys
import tomllib

REPO = pathlib.Path(__file__).resolve().parents[2]
PUBLIC_TREE = REPO / "docs" / "PUBLIC_TREE.toml"
ISSUE_FORMS = pathlib.Path(".github/ISSUE_TEMPLATE")

MINIMUM_FILES = 6
MINIMUM_ISSUE_FORMS = 2

# What each file has to say, as a phrase somebody would have to delete on
# purpose. The wording is matched case-insensitively and with runs of whitespace
# collapsed, so re-wrapping a paragraph does not turn this red.
REQUIRED: dict[str, list[tuple[str, str]]] = {
    "CONTRIBUTING.md": [
        ("git commit -s", "the command that signs a commit off"),
        ("Developer Certificate of Origin", "what the sign-off certifies (#223)"),
    ],
    "SECURITY.md": [
        ("security@pdfluent.com", "the address a report goes to"),
        ("do not open a public github issue",
         "that a vulnerability is not filed in the open"),
        ("there is no bug bounty",
         "that no reward is offered, said plainly rather than left to be assumed"),
    ],
    ".github/PULL_REQUEST_TEMPLATE.md": [
        ("git commit -s", "the sign-off, where the author still has time to add it"),
    ],
    ".github/CODEOWNERS": [],
    ".github/ISSUE_TEMPLATE/config.yml": [
        ("blank_issues_enabled: false",
         "that the forms are the route in and not a suggestion"),
        ("security.md", "where a vulnerability goes instead of an issue"),
    ],
    ".github/ISSUE_TEMPLATE/bug_report.yml": [
        ("git commit -s", "the sign-off, for the patch pasted into an issue"),
        ("security.md", "that a vulnerability does not belong in a bug report"),
    ],
    ".github/ISSUE_TEMPLATE/feature_request.yml": [
        ("git commit -s", "the sign-off, for the sender who implements it"),
    ],
}

# A promise of money is the one thing a security policy must not make by
# accident: a report written in the expectation of a payment costs somebody
# their evening. Any line naming one of these has to deny it in the same
# breath.
PAYMENT_WORDS = ("bounty", "reward", "payout", "cash prize")
DENIALS = ("no ", "not ", "never ", "none ", "nor ")


def normalise(text: str) -> str:
    return re.sub(r"\s+", " ", text).lower()


def internal_paths(register: pathlib.Path = PUBLIC_TREE) -> list[str]:
    """The paths the seeding strips, from the one list that decides it."""
    if not register.is_file():
        raise SystemExit(
            f"[contribution] FATAL: {register} is missing, so nothing was checked.")
    data = tomllib.loads(register.read_text(encoding="utf-8"))
    return list(data.get("internal", {}).get("paths", []))


def is_stripped(path: str, stripped: list[str]) -> str | None:
    """The entry that keeps `path` out of the published tree, or None."""
    for entry in stripped:
        if entry.endswith("/"):
            if path == entry.rstrip("/") or path.startswith(entry):
                return entry
        elif path == entry:
            return entry
    return None


def paths_named_in(text: str) -> set[str]:
    """Repository paths a file points a reader at, from its backticked spans.

    Only what exists here can be checked for whether it travels, so a span that
    names no tracked file is skipped rather than guessed at -- `git commit -s`
    and `cargo test` are not paths, and a guard that argued they were would be
    switched off within the week.
    """
    found = set()
    for span in re.findall(r"`([^`\n]+)`", text):
        candidate = span.strip()
        if " " in candidate or not ("/" in candidate or candidate.startswith(".")):
            continue
        candidate = candidate.rstrip("/.,)")
        if (REPO / candidate).exists():
            found.add(candidate)
    return found


def judge_payment_promises(text: str) -> list[str]:
    problems = []
    for line in text.splitlines():
        low = line.lower()
        if not any(word in low for word in PAYMENT_WORDS):
            continue
        if not any(denial in low + " " for denial in DENIALS):
            problems.append(line.strip())
    return problems


def judge(root: pathlib.Path, stripped: list[str]) -> tuple[list[str], list[str]]:
    """Every verdict about one tree. Returns (fatal, reported)."""
    fatal: list[str] = []
    reported: list[str] = []

    if len(REQUIRED) < MINIMUM_FILES:
        fatal.append(
            f"the register holds {len(REQUIRED)} files, below the floor of "
            f"{MINIMUM_FILES}: it has been emptied rather than satisfied")

    for name, phrases in REQUIRED.items():
        path = root / name
        if not path.is_file():
            fatal.append(f"{name} is missing -- a contributor meets nothing here")
            continue
        text = path.read_text(encoding="utf-8", errors="replace")
        flat = normalise(text)

        for phrase, why in phrases:
            if normalise(phrase) not in flat:
                fatal.append(f"{name} no longer says {phrase!r} -- {why}")

        if name.endswith("CODEOWNERS"):
            rules = [ln for ln in text.splitlines()
                     if ln.strip() and not ln.lstrip().startswith("#")]
            if not rules:
                fatal.append(
                    "CODEOWNERS carries no rule, so no review is ever requested")

        entry = is_stripped(name, stripped)
        if entry:
            fatal.append(
                f"{name} is stripped by the seeding ({entry!r} in PUBLIC_TREE.toml), "
                f"so the public repository never receives it")

        for target in sorted(paths_named_in(text)):
            entry = is_stripped(target, stripped)
            if entry:
                fatal.append(
                    f"{name} points at {target}, which the seeding strips "
                    f"({entry!r}) -- the published copy sends a reader to a file "
                    f"that is not there")

    security = root / "SECURITY.md"
    if security.is_file():
        for line in judge_payment_promises(
                security.read_text(encoding="utf-8", errors="replace")):
            fatal.append(
                f"SECURITY.md promises a payment without denying it: {line!r}")

    forms = sorted(p for p in (root / ISSUE_FORMS).glob("*.yml")
                   if p.name != "config.yml")
    if len(forms) < MINIMUM_ISSUE_FORMS:
        fatal.append(
            f"{len(forms)} issue form(s) below {ISSUE_FORMS}, fewer than the "
            f"{MINIMUM_ISSUE_FORMS} the register expects")
    for form in forms:
        flat = normalise(form.read_text(encoding="utf-8", errors="replace"))
        if "git commit -s" not in flat:
            fatal.append(
                f"{form.relative_to(root)} does not name the sign-off, so a patch "
                f"pasted into it arrives certifying nothing")

    if not fatal:
        reported.append(
            f"{len(REQUIRED)} files, {len(forms)} issue forms: present, saying "
            f"what they must, and none of them or of what they name is stripped "
            f"by the seeding")
    return fatal, reported


def main() -> int:
    stripped = internal_paths()
    fatal, reported = judge(REPO, stripped)
    for line in reported:
        print(f"[contribution] {line}")
    for line in fatal:
        print(f"[contribution] FATAL: {line}")
    if fatal:
        print(f"[contribution] {len(fatal)} problem(s); see docs/PUBLIC_TREE.toml "
              f"for what travels and CONTRIBUTING.md for what is promised.")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
