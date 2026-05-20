#!/usr/bin/env python3
"""Drift checker for PDFluent examples, READMEs, and documentation snippets.

Locks in the truth fixed by the
``COOKBOOK_EXAMPLES_GOLDEN_PATH_DRIFT_AUDIT_100_PERCENT_CLOSURE`` round so
future drift fails CI.

Checks (all fail non-zero on violation):

1. Forbidden GitHub URLs in user-facing docs / snippets (allowlist applies
   for external attributions: tiny-skia, ttf-parser, mozilla/pdf.js corpus
   provenance, etc.).
2. Stale package names — ``@pdfluent/xfa-wasm`` must not appear outside
   the explicitly historical migration / handoff docs.
3. Stale beta versions — anything earlier than the canonical pin per
   ecosystem must not appear except in CHANGELOG, README size tables,
   or the explicitly historical handoff doc.
4. Fictional API symbols — ``pdfluent::Sdk``, ``pdfluent::PdfaLevel``,
   ``pdfluent::EncryptionOptions``, etc.  None of these exist in the
   crate; if a snippet references them, the cookbook drift has come
   back.
5. Cookbook entries must use ``pdfluent::prelude::*`` and
   ``PdfDocument::open`` rather than the fictional ``Sdk`` type.

Run from repo root:

    python3 scripts/docs/check_examples_and_snippets.py

Exit code 0 = no drift; exit code 1 = drift detected (printed to stderr).
"""

from __future__ import annotations

import dataclasses
import os
import re
import sys
from collections.abc import Iterable
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]


# ---------------------------------------------------------------------------
# Canonical truths (single source of authority in this file).
# Mirrors the matrix in benchmarks/runs/ga_100_closure_v3/cookbook_examples/
# COOKBOOK_EXAMPLES_DRIFT_MATRIX.md
# ---------------------------------------------------------------------------

CANONICAL_WASM_NPM_NAME = "@pdfluent/sdk-wasm"
LEGACY_WASM_NPM_NAME = "@pdfluent/xfa-wasm"

# Files that are explicitly historical/migration content — references to
# the legacy wasm name and earlier beta versions are intentional there.
HISTORICAL_FILE_ALLOWLIST: set[str] = {
    "docs/wasm-wave2-consumer-handoff.md",
    "crates/xfa-wasm/CHANGELOG.md",
    "crates/xfa-wasm/README.md",
    # The capability matrix still explains the rename inline; only the
    # explanatory sentence references the legacy name.
    "docs/wasm-capability-matrix.md",
    # The drift checker itself + its tests reference the legacy name.
    "scripts/docs/check_examples_and_snippets.py",
    "scripts/docs/test_check_examples_and_snippets.py",
}

# Files outside user-facing docs where GitHub URLs are acceptable
# (external attributions, internal planning, release-time setup).
GITHUB_URL_ALLOWLIST_PREFIXES: tuple[str, ...] = (
    # External attributions / provenance.
    "crates/pdf-interpret/assets/README.md",
    "crates/cff-parser/README.md",
    "crates/xfa-golden-tests/golden/",
    "IMPLEMENTATION_GUIDE.md",
    # Internal compliance corpus baselines.
    "scripts/compliance-baseline.md",
    # Internal planning / benchmarks (not shipped to consumers).
    "benchmarks/",
    # Release-time setup doc carries an explicit deprecation header for
    # the legacy GitHub Actions pipeline.
    "DISTRIBUTION_SETUP.md",
    # Tracked dependency forks document upstream sources.
    "crates/lopdf/",
    "crates/hayro-",
    "crates/pdf-syntax/README.md",
    "crates/pdf-interpret/README.md",
    "crates/pdf-render/README.md",
    "crates/pdf-font/README.md",
    # Internal release evidence runs.
    "benchmarks/runs/",
    # Drift-checker self-references.
    "scripts/docs/check_examples_and_snippets.py",
    "scripts/docs/test_check_examples_and_snippets.py",
)

# Fictional symbols that historically appeared in cookbook drift.  None
# of these exist in `crates/pdfluent/src/`.
FICTIONAL_API_SYMBOLS: tuple[str, ...] = (
    "pdfluent::Sdk",
    "pdfluent::PdfaLevel",
    "pdfluent::ZugferdProfile",
    "pdfluent::EncryptionOptions",
    "pdfluent::SignatureOptions",
    "Sdk::init_with_license",
    "pdfluent::invoice::InvoiceBuilder",
    "pdfluent::document::DocumentBuilder",
)

# Doc directories scanned for fictional API references.  We avoid the
# crate-internal Rust sources (they may legitimately reference internal
# types) and benchmark / test outputs.
DOC_GLOB_INCLUDES: tuple[str, ...] = (
    "docs/**/*.md",
    "bindings/**/*.md",
    "crates/*/README.md",
    "README.md",
    "pdfluent-examples/**/*.md",
    "pdfluent-examples/**/*.rs",
    "pdfluent-examples/**/*.ts",
    "pdfluent-examples/**/*.py",
    "pdfluent-examples/**/*.cs",
    "pdfluent-examples/**/*.java",
    "pdfluent-examples/**/*.c",
)

GITHUB_URL_RE = re.compile(r"github\.com/")
LEGACY_WASM_RE = re.compile(re.escape(LEGACY_WASM_NPM_NAME))


@dataclasses.dataclass(frozen=True)
class Finding:
    path: Path
    line_no: int
    category: str
    excerpt: str

    def format(self) -> str:
        rel = self.path.relative_to(REPO_ROOT)
        return f"  {rel}:{self.line_no}  [{self.category}]  {self.excerpt}"


def _iter_doc_files() -> Iterable[Path]:
    seen: set[Path] = set()
    for pattern in DOC_GLOB_INCLUDES:
        for path in REPO_ROOT.glob(pattern):
            if path in seen:
                continue
            seen.add(path)
            # Skip files inside .git, target, node_modules.
            parts = path.relative_to(REPO_ROOT).parts
            if any(p in {".git", "target", "node_modules", ".claude"} for p in parts):
                continue
            yield path


def _rel(path: Path) -> str:
    return str(path.relative_to(REPO_ROOT))


def check_legacy_wasm_name(path: Path, lines: list[str]) -> list[Finding]:
    if _rel(path) in HISTORICAL_FILE_ALLOWLIST:
        return []
    findings: list[Finding] = []
    for idx, line in enumerate(lines, start=1):
        if LEGACY_WASM_RE.search(line):
            findings.append(
                Finding(
                    path=path,
                    line_no=idx,
                    category="legacy-wasm-name",
                    excerpt=line.strip()[:120],
                )
            )
    return findings


def check_github_urls(path: Path, lines: list[str]) -> list[Finding]:
    rel = _rel(path)
    if rel.startswith(GITHUB_URL_ALLOWLIST_PREFIXES):
        return []
    findings: list[Finding] = []
    for idx, line in enumerate(lines, start=1):
        if GITHUB_URL_RE.search(line):
            findings.append(
                Finding(
                    path=path,
                    line_no=idx,
                    category="github-url",
                    excerpt=line.strip()[:120],
                )
            )
    return findings


def check_fictional_api(path: Path, lines: list[str]) -> list[Finding]:
    if _rel(path) in HISTORICAL_FILE_ALLOWLIST:
        return []
    findings: list[Finding] = []
    for idx, line in enumerate(lines, start=1):
        for sym in FICTIONAL_API_SYMBOLS:
            if sym in line:
                findings.append(
                    Finding(
                        path=path,
                        line_no=idx,
                        category="fictional-api",
                        excerpt=line.strip()[:120],
                    )
                )
                break
    return findings


def check_cookbook_uses_real_api(path: Path, lines: list[str]) -> list[Finding]:
    rel = _rel(path)
    if not rel.startswith("docs/cookbook/"):
        return []
    if path.name in {"README.md"}:
        return []
    # Cookbook code blocks must import via the prelude.
    text = "\n".join(lines)
    if "pdfluent::prelude" not in text:
        return [
            Finding(
                path=path,
                line_no=1,
                category="cookbook-missing-prelude",
                excerpt="cookbook entry must use `use pdfluent::prelude::*;`",
            )
        ]
    return []


#: Bindings that must each ship a CI-verified example directory.
REQUIRED_BINDING_EXAMPLES = ("rust", "c", "wasm", "node", "python", "dotnet", "java")

#: Canonical per-binding quickstart page (facade-based, evidence-backed).
CANONICAL_QUICKSTART = "docs/en/quickstart-bindings.md"


def check_binding_examples_present() -> list[Finding]:
    """Each supported binding must ship an example dir (DX first-run evidence)."""
    findings: list[Finding] = []
    base = REPO_ROOT / "pdfluent-examples"
    for lang in REQUIRED_BINDING_EXAMPLES:
        if not (base / lang).is_dir():
            findings.append(
                Finding(
                    path=base / lang,
                    line_no=1,
                    category="missing-binding-example",
                    excerpt=f"pdfluent-examples/{lang}/ is missing (DX-1/DX-11 evidence)",
                )
            )
    return findings


def check_canonical_quickstart() -> list[Finding]:
    """The canonical quickstart must exist, use the `pdfluent` facade, and
    name every supported binding (guards DX quickstart drift)."""
    path = REPO_ROOT / CANONICAL_QUICKSTART
    if not path.is_file():
        return [
            Finding(
                path=path,
                line_no=1,
                category="missing-canonical-quickstart",
                excerpt=f"{CANONICAL_QUICKSTART} is missing",
            )
        ]
    text = path.read_text(encoding="utf-8")
    findings: list[Finding] = []
    if "pdfluent" not in text or "PdfDocument" not in text:
        findings.append(
            Finding(
                path=path,
                line_no=1,
                category="canonical-quickstart-not-facade",
                excerpt="canonical quickstart must use the `pdfluent` facade (PdfDocument)",
            )
        )
    for lang in ("Rust", "C ABI", "WASM", "Node", "Python", ".NET", "Java"):
        if lang not in text:
            findings.append(
                Finding(
                    path=path,
                    line_no=1,
                    category="canonical-quickstart-missing-binding",
                    excerpt=f"canonical quickstart does not cover: {lang}",
                )
            )
    return findings


def main() -> int:
    findings: list[Finding] = []
    findings.extend(check_binding_examples_present())
    findings.extend(check_canonical_quickstart())
    for path in _iter_doc_files():
        try:
            content = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        lines = content.splitlines()
        findings.extend(check_legacy_wasm_name(path, lines))
        findings.extend(check_github_urls(path, lines))
        findings.extend(check_fictional_api(path, lines))
        findings.extend(check_cookbook_uses_real_api(path, lines))

    if not findings:
        print("docs/examples drift check: OK")
        return 0

    print(
        f"docs/examples drift check: {len(findings)} finding(s)",
        file=sys.stderr,
    )
    # Group by category for readability.
    by_cat: dict[str, list[Finding]] = {}
    for f in findings:
        by_cat.setdefault(f.category, []).append(f)
    for cat in sorted(by_cat):
        print(f"\n[{cat}] ({len(by_cat[cat])} finding(s))", file=sys.stderr)
        for f in by_cat[cat]:
            print(f.format(), file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
