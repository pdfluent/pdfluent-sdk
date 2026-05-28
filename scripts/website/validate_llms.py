#!/usr/bin/env python3
"""
validate_llms.py — F4-LLMS-VALIDATOR

Validates PDFluent's `llms.txt` and `llms-full.txt` (the LLM-ingest manifests
served from the marketing site) against drift categories that have bitten us
historically:

  1. Locale leak     — non-canonical locale URLs (e.g. `/nl/…`, `/de/…`).
                       The site is English-only; locale routes redirect.
  2. Stale package   — `xfa-wasm` references that are NOT clearly marked
                       as a historical-rename note.
  3. Claim sanity    — assertive claims ("supports X", "works with Y") that
                       aren't on the curated allowlist. Soft fail by default,
                       hard fail with --strict.
  4. Feature denylist — phrases the SDK does NOT currently ship; presence in
                       llms files is a hard fail (false-advertising risk).

Exit code 0 = clean, non-zero = drift (count categorised in JSON output).

Usage:
  python3 validate_llms.py \\
      --llms PATH/llms.txt \\
      --llms-full PATH/llms-full.txt \\
      [--allowlist scripts/website/llms_claim_allowlist.txt] \\
      [--denylist scripts/website/llms_feature_denylist.txt] \\
      [--json-out benchmarks/runs/ga_hardening_plan/f4/LLMS_DRIFT_REPORT.json] \\
      [--strict]

The script is intentionally dependency-free (stdlib only) so it runs on any
runner without an extra `pip install` step.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass, field, asdict
from pathlib import Path
from typing import Iterable

# ---------------------------------------------------------------------------
# Configuration constants
# ---------------------------------------------------------------------------

# Locales we explicitly disallow in the canonical English-only LLM manifest.
# Keep this list in sync with the website i18n config. Adding a locale here
# means "if it appears anywhere in llms{,-full}.txt, that's drift".
DISALLOWED_LOCALES = (
    "nl", "de", "fr", "es", "it", "pt", "ja", "zh", "ko", "ru", "pl", "tr",
)

# Pattern that matches `pdfluent.com/<locale>/...` or a bare `/<locale>/...`
# path segment. We tolerate ISO-like two-letter codes only.
_LOCALE_RE = re.compile(
    r"(?:pdfluent\.com)?/(" + "|".join(DISALLOWED_LOCALES) + r")(?:/|\b)",
    re.IGNORECASE,
)

# The deprecated package identifier. Allowed ONLY in clearly-marked rename
# context: same line/paragraph also names the canonical package
# `@pdfluent/sdk-wasm` AND uses one of the rename markers below.
_STALE_PACKAGE_RE = re.compile(r"\bxfa-wasm\b")
_CANONICAL_WASM_PKG = "@pdfluent/sdk-wasm"
_RENAME_MARKERS = (
    "rename", "renamed", "deprecated", "deprecation",
    "former", "formerly", "previously", "legacy", "historical",
)

# Claim sanity — assertive verbs we want to cross-check. Tuned for false-
# positive control: only flag claims that read as feature assertions.
_CLAIM_PATTERNS = (
    re.compile(r"\b(supports?|supported|supporting)\b\s+[A-Z]?[\w/\- ]{3,40}", re.IGNORECASE),
    re.compile(r"\bcompatible with\b\s+[\w/\- ]{3,40}", re.IGNORECASE),
    re.compile(r"\b(works with|works for)\b\s+[\w/\- ]{3,40}", re.IGNORECASE),
    re.compile(r"\b(certified|compliant) (?:for|with)?\s*[\w/\- ]{3,40}", re.IGNORECASE),
)

# ---------------------------------------------------------------------------
# Data types
# ---------------------------------------------------------------------------


@dataclass
class Violation:
    category: str
    severity: str  # "error" | "warning"
    file: str
    line: int
    text: str
    note: str = ""


@dataclass
class Report:
    files_checked: list[str] = field(default_factory=list)
    violations: list[Violation] = field(default_factory=list)
    counts: dict[str, int] = field(default_factory=dict)
    strict: bool = False

    def add(self, v: Violation) -> None:
        self.violations.append(v)
        self.counts[v.category] = self.counts.get(v.category, 0) + 1

    def errors(self) -> list[Violation]:
        return [v for v in self.violations if v.severity == "error"]

    def warnings(self) -> list[Violation]:
        return [v for v in self.violations if v.severity == "warning"]

    def exit_code(self) -> int:
        if self.errors():
            return 1
        if self.strict and self.warnings():
            return 2
        return 0


# ---------------------------------------------------------------------------
# Checks
# ---------------------------------------------------------------------------


def _paragraph_for_line(lines: list[str], idx: int) -> str:
    """Return the paragraph (run of non-blank lines) containing line idx (1-based)."""
    if idx < 1 or idx > len(lines):
        return ""
    # walk backward to start of paragraph
    start = idx - 1
    while start > 0 and lines[start - 1].strip():
        start -= 1
    # walk forward to end
    end = idx - 1
    while end < len(lines) - 1 and lines[end + 1].strip():
        end += 1
    return "\n".join(lines[start:end + 1])


def check_locale_leak(path: Path, lines: list[str], report: Report) -> None:
    for i, line in enumerate(lines, 1):
        for m in _LOCALE_RE.finditer(line):
            # Allow mentions inside code fences or in a paragraph that
            # documents the redirect policy — heuristic: paragraph contains
            # the word "redirect" or "locale".
            para = _paragraph_for_line(lines, i).lower()
            if "redirect" in para or "translated locale" in para or "i18n" in para:
                continue
            report.add(Violation(
                category="locale_leak",
                severity="error",
                file=str(path),
                line=i,
                text=line.strip(),
                note=f"locale `/{m.group(1)}/` present in canonical English manifest",
            ))


def check_stale_package(path: Path, lines: list[str], report: Report) -> None:
    """
    `xfa-wasm` is the deprecated package name. We allow it only when the
    surrounding paragraph also names `@pdfluent/sdk-wasm` AND contains a
    rename/deprecation marker — this keeps legitimate migration notes legal
    while catching accidental regressions in install commands.
    """
    for i, line in enumerate(lines, 1):
        if not _STALE_PACKAGE_RE.search(line):
            continue
        para = _paragraph_for_line(lines, i).lower()
        has_canonical = _CANONICAL_WASM_PKG.lower() in para
        has_marker = any(m in para for m in _RENAME_MARKERS)
        if has_canonical and has_marker:
            continue  # acceptable historical reference
        report.add(Violation(
            category="stale_package",
            severity="error",
            file=str(path),
            line=i,
            text=line.strip(),
            note="stale `xfa-wasm` outside rename/deprecation context",
        ))


def check_claim_sanity(
    path: Path,
    lines: list[str],
    report: Report,
    allowlist: set[str],
) -> None:
    for i, line in enumerate(lines, 1):
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        for pat in _CLAIM_PATTERNS:
            for m in pat.finditer(stripped):
                claim = _normalise(m.group(0))
                if any(_normalise(a) in claim or claim in _normalise(a) for a in allowlist):
                    continue
                report.add(Violation(
                    category="claim_unreviewed",
                    severity="warning",
                    file=str(path),
                    line=i,
                    text=stripped,
                    note=f"claim '{m.group(0)}' not on allowlist — human review",
                ))


# Negation markers — denylist matches inside a negated/disclaimer sentence
# are dropped (e.g. "we do NOT claim full Adobe parity"). The marker must
# appear *before* the matched phrase on the same line.
_NEGATION_MARKERS = (
    "not ", "**not**", "no ", "never ", "without ", "isn't", "is not",
    "doesn't", "does not", "don't", "do not", "we do not claim",
    "we don't claim",
)


def _is_negated(line_lower: str, phrase_start: int) -> bool:
    prefix = line_lower[:phrase_start]
    return any(m in prefix for m in _NEGATION_MARKERS)


def check_feature_denylist(
    path: Path,
    lines: list[str],
    report: Report,
    denylist: list[str],
) -> None:
    for i, line in enumerate(lines, 1):
        low = line.lower()
        for phrase in denylist:
            phrase = phrase.strip()
            if not phrase or phrase.startswith("#"):
                continue
            idx = low.find(phrase.lower())
            if idx < 0:
                continue
            if _is_negated(low, idx):
                continue  # disclaimer / negated context — acceptable
            report.add(Violation(
                category="feature_denied",
                severity="error",
                file=str(path),
                line=i,
                text=line.strip(),
                note=f"denylisted phrase '{phrase}' — SDK does not currently ship this",
            ))


def _normalise(s: str) -> str:
    return re.sub(r"\s+", " ", s.strip().lower())


# ---------------------------------------------------------------------------
# I/O helpers
# ---------------------------------------------------------------------------


def _read_lines(p: Path) -> list[str]:
    return p.read_text(encoding="utf-8").splitlines()


def _load_allowlist(p: Path | None) -> set[str]:
    if not p or not p.exists():
        return set()
    out: set[str] = set()
    for raw in _read_lines(p):
        s = raw.strip()
        if s and not s.startswith("#"):
            out.add(s)
    return out


def _load_denylist(p: Path | None) -> list[str]:
    if not p or not p.exists():
        return []
    return [l.strip() for l in _read_lines(p) if l.strip() and not l.startswith("#")]


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------


def run(
    llms_path: Path,
    llms_full_path: Path,
    allowlist_path: Path | None,
    denylist_path: Path | None,
    strict: bool,
) -> Report:
    report = Report(strict=strict)
    allowlist = _load_allowlist(allowlist_path)
    denylist = _load_denylist(denylist_path)

    for p in (llms_path, llms_full_path):
        if not p.exists():
            # NEVER silently exit-0 on missing input — that's fake-green.
            print(f"FATAL: {p} does not exist", file=sys.stderr)
            sys.exit(3)
        report.files_checked.append(str(p))
        lines = _read_lines(p)
        check_locale_leak(p, lines, report)
        check_stale_package(p, lines, report)
        check_feature_denylist(p, lines, report, denylist)
        check_claim_sanity(p, lines, report, allowlist)

    return report


def _render_human(report: Report) -> str:
    lines: list[str] = []
    lines.append("PDFluent llms.txt drift validator")
    lines.append("=" * 60)
    lines.append(f"Files checked: {len(report.files_checked)}")
    for f in report.files_checked:
        lines.append(f"  - {f}")
    lines.append("")
    if not report.violations:
        lines.append("CLEAN — no drift detected.")
        return "\n".join(lines)
    lines.append("Violations by category:")
    for cat, n in sorted(report.counts.items()):
        lines.append(f"  {cat}: {n}")
    lines.append("")
    lines.append("Details:")
    for v in report.violations:
        lines.append(
            f"  [{v.severity.upper():7}] {v.category} "
            f"{Path(v.file).name}:{v.line} — {v.note}"
        )
        lines.append(f"           >> {v.text[:140]}")
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--llms", required=True, type=Path)
    ap.add_argument("--llms-full", required=True, type=Path)
    ap.add_argument("--allowlist", type=Path, default=None)
    ap.add_argument("--denylist", type=Path, default=None)
    ap.add_argument("--json-out", type=Path, default=None)
    ap.add_argument("--strict", action="store_true",
                    help="treat claim-sanity warnings as failures")
    args = ap.parse_args(argv)

    report = run(
        llms_path=args.llms,
        llms_full_path=args.llms_full,
        allowlist_path=args.allowlist,
        denylist_path=args.denylist,
        strict=args.strict,
    )

    print(_render_human(report))

    if args.json_out:
        args.json_out.parent.mkdir(parents=True, exist_ok=True)
        payload = {
            "files_checked": report.files_checked,
            "counts": report.counts,
            "strict": report.strict,
            "errors": [asdict(v) for v in report.errors()],
            "warnings": [asdict(v) for v in report.warnings()],
        }
        args.json_out.write_text(json.dumps(payload, indent=2), encoding="utf-8")
        print(f"\nJSON report → {args.json_out}")

    code = report.exit_code()
    if code:
        print(f"\nFAIL — exit {code}", file=sys.stderr)
    return code


if __name__ == "__main__":
    sys.exit(main())
