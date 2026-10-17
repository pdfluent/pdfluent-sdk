#!/usr/bin/env python3
"""Generate the error catalogue from crates/pdfluent/src/error.rs.

Parses the Rust source to extract all Error variants, their stable codes,
and doc comments. Writes two files, both generated, neither hand-edited:

  docs/error_catalogue.md    -- the internal table, with per-binding columns.
  docs/sdk-error-codes.json  -- exactly the file pdfluent.com renders as the
                                anchor list on /errors. The website repository
                                carries a copy at src/data/sdk-error-codes.json
                                and its own test refuses a copy that is not
                                byte-identical to this one.

The JSON exists because the same drift happened three times (#320, #347,
#426): a code was added to error.rs and the published page never grew a
section for it, so the one link handed to a user at the moment something broke
led nowhere. Generating the page's input from error.rs makes that impossible
to do by accident; `scripts/ci/error_catalogue_is_generated.py` makes it
impossible to do on purpose without noticing.

Exit codes:
  0 -- files written successfully
  1 -- parse error or missing code
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
ERROR_RS = REPO_ROOT / "crates" / "pdfluent" / "src" / "error.rs"
OUT_FILE = REPO_ROOT / "docs" / "error_catalogue.md"
JSON_OUT_FILE = REPO_ROOT / "docs" / "sdk-error-codes.json"

# ---------------------------------------------------------------------------
# Parsing helpers
# ---------------------------------------------------------------------------

# Matches a code() arm such as:
#   Error::Foo { .. } => "E-CATEGORY-SPECIFIC",
CODE_ARM_RE = re.compile(
    # Struct variants `Error::Foo { .. } => "E-..."`, tuple variants
    # `Error::Foo(_) => "E-..."`, and unit variants `Error::Foo => "E-..."`.
    r'Error::(\w+)(?:\s*\{[^}]*\}|\s*\([^)]*\))?\s*=>\s*"(E-[A-Z0-9\-]+)"'
)

# Matches variant doc comments (/// lines) immediately preceding a variant.
# We do a two-pass approach: first collect all doc-comment blocks, then
# associate them with the variant name that follows.


def parse_error_rs(path: Path) -> list[dict]:
    """Return a list of variant dicts with keys: variant, code, doc."""
    text = path.read_text(encoding="utf-8")

    # Build variant -> code map from the code() match arms.
    code_map: dict[str, str] = {}
    for m in CODE_ARM_RE.finditer(text):
        variant, code = m.group(1), m.group(2)
        if variant in code_map and code_map[variant] != code:
            print(
                f"ERROR: variant {variant!r} maps to two different codes: "
                f"{code_map[variant]!r} and {code!r}",
                file=sys.stderr,
            )
            sys.exit(1)
        code_map[variant] = code

    if not code_map:
        print("ERROR: no code() arms found in error.rs", file=sys.stderr)
        sys.exit(1)

    # Walk the enum body to extract variants and their preceding doc comments.
    # Strategy: find the enum Error { … } block, then scan line by line.
    enum_block_re = re.compile(
        r"pub enum Error \{(.+?)^}", re.DOTALL | re.MULTILINE
    )
    m = enum_block_re.search(text)
    if not m:
        print("ERROR: could not locate 'pub enum Error {' block", file=sys.stderr)
        sys.exit(1)
    enum_body = m.group(1)

    # Variant declaration: an identifier followed by `{` (struct variant),
    # `(` (tuple variant), or `,` / end of line (unit variant), preceded by
    # optional doc comments.
    #
    # The `(` arm is not decoration: without it `Unsupported(String)` was
    # skipped, so `E-UNSUPPORTED` never reached the catalogue or the published
    # page. It is the same drift as #320/#347/#426, one layer earlier -- and
    # the sync gate did not catch it because it grepped for the bare code and
    # `E-PARSE-UNSUPPORTED-VERSION` contains `E-UNSUPPORTED` as a substring.
    variant_re = re.compile(
        r"((?:^\s*///[^\n]*\n)+)?\s*^\s*([A-Z][\w]*)\s*(?:\{|\(|,|$)",
        re.MULTILINE,
    )

    variants: list[dict] = []
    seen_codes: set[str] = set()

    for vm in variant_re.finditer(enum_body):
        raw_doc = vm.group(1) or ""
        variant_name = vm.group(2)

        # Strip out comment markers and leading whitespace from doc lines.
        doc_lines = []
        for line in raw_doc.splitlines():
            stripped = re.sub(r"^\s*///\s?", "", line)
            if stripped:
                doc_lines.append(stripped)
        doc_text = " ".join(doc_lines).strip()

        code = code_map.get(variant_name)
        if code is None:
            print(
                f"ERROR: variant {variant_name!r} has no entry in code() — "
                "add a match arm before regenerating the catalogue",
                file=sys.stderr,
            )
            sys.exit(1)

        if code in seen_codes:
            print(
                f"ERROR: duplicate code {code!r} for variant {variant_name!r}",
                file=sys.stderr,
            )
            sys.exit(1)
        seen_codes.add(code)
        variants.append({"variant": variant_name, "code": code, "doc": doc_text})

    if not variants:
        print("ERROR: no variants parsed from enum body", file=sys.stderr)
        sys.exit(1)

    return variants


# ---------------------------------------------------------------------------
# How-to-fix lookup (human-curated, keyed by code)
# ---------------------------------------------------------------------------

HOW_TO_FIX: dict[str, str] = {
    "E-IO-GENERIC": (
        "Check that the file path is accessible and the process has read/write "
        "permissions. Inspect `source` for the underlying OS error."
    ),
    "E-IO-FILE-NOT-FOUND": (
        "Verify the path exists before calling. Use `Path::exists()` or handle "
        "this variant to prompt the user for the correct path."
    ),
    "E-PARSE-INVALID-PDF": (
        "Ensure the bytes are a complete, undamaged PDF. Check `byte_offset` for "
        "the failure site. Re-download or re-export the file if corrupt."
    ),
    "E-PARSE-UNSUPPORTED-VERSION": (
        "The PDF version header exceeds what this build supports. Upgrade to a "
        "newer PDFluent release, or pre-process the file with a downgrader."
    ),
    "E-COMPLIANCE-PDFA-INVALID": (
        "Inspect `violations` for specific rule identifiers. Use "
        "`OpenOptions::convert_to_pdfa()` to auto-repair, or fix the source "
        "document before validation."
    ),
    "E-SECURITY-DECRYPTION-FAILED": (
        "Supply the correct password via `OpenOptions::password()`. Check "
        "`reason` to distinguish wrong-password from unsupported-algorithm cases."
    ),
    "E-SECURITY-INVALID-SIGNATURE": (
        "The signature in `field` failed verification. Check `reason` for "
        "details. Do not trust the document content if integrity is required."
    ),
    "E-ENV-UNSUPPORTED-ON-WASM": (
        "This operation (`operation`) cannot run in a WASM32 environment. Use "
        "the server-side API or guard with `#[cfg(not(target_arch = \"wasm32\"))]`."
    ),
    "E-ENV-MISSING-DEPENDENCY": (
        "Install the missing native library (`dep`) following `install_hint`. "
        "Ensure the library is on `LD_LIBRARY_PATH` / `DYLD_LIBRARY_PATH`."
    ),
    "E-BUDGET-MEMORY-EXCEEDED": (
        "Raise the memory limit via `OpenOptions::strict_memory_limit()`, or "
        "process the document in smaller chunks."
    ),
    "E-BUDGET-RESOURCE-LIMIT": (
        "Inspect `kind` to identify which cap fired, then raise the corresponding "
        "`ProcessingLimits` field. For untrusted input, keep limits tight and "
        "reject oversized files at the ingestion layer."
    ),
    "E-EDIT-TEXT-FAILED": (
        "The message carries the typed reason from the edit session. A stale "
        "or invalid match id means the document changed after the search: "
        "search again and stage against the new matches. `unsupported "
        "container` and `match spans multiple styles` mean the text cannot be "
        "edited in place -- redact and re-draw it instead. Overlapping or "
        "duplicated stages are a mistake in the caller's own batch."
    ),
    "E-PAGE-OUT-OF-RANGE": (
        "Pass a page number between 1 and `page_count()` inclusive. If you are "
        "porting from a 0-based API, add 1."
    ),
    "E-PAGE-RANGE-INVALID": (
        "Pass a range that lies inside 1..=`page_count()` and does not end "
        "before it starts. If you are porting from a 0-based API, add 1 to "
        "both ends; `0..n` becomes `1..=n`."
    ),
    "E-UNSUPPORTED": (
        "Read the message: it names what was asked for. Either the document "
        "lacks the structure the operation needs, or the capability is not "
        "part of this build."
    ),
    "E-INTERNAL": (
        "This should never occur under normal operation. File a bug report at "
        "https://pdfluent.com/support including `message` and `crate_version`."
    ),
}

# ---------------------------------------------------------------------------
# Public wording (what pdfluent.com shows), where it differs from the rustdoc
# ---------------------------------------------------------------------------
#
# The internal table quotes the doc comment verbatim, because that is what a
# maintainer reading the catalogue wants. The published page has a different
# reader: someone whose call just failed. A doc comment written for the first
# reader is often wrong for the second -- it carries rustdoc link syntax,
# issue numbers, and paragraphs of rationale.
#
# Only the codes listed here diverge; everything else uses the doc comment, so
# the default stays "one source, no editing".
PUBLIC_WHAT: dict[str, str] = {
    "E-BUDGET-RESOURCE-LIMIT": (
        "A processing limit you configured was exceeded while loading or "
        "working on the document. The `kind` field says which one."
    ),
    "E-PAGE-OUT-OF-RANGE": (
        "A page number outside the document was asked for. Pages are numbered "
        "from 1, so 0 is never valid -- it is the number a caller arriving "
        "from a 0-based API reaches for first."
    ),
    "E-PAGE-RANGE-INVALID": (
        "A page range the document cannot satisfy was asked for: it runs past "
        "the last page, starts at 0, or ends before it starts."
    ),
    "E-UNSUPPORTED": (
        "The requested operation is not supported for this document or in "
        "this build."
    ),
}

# ---------------------------------------------------------------------------
# Per-binding gap analysis (manually maintained, updated by C8-3..C8-8)
# ---------------------------------------------------------------------------

BINDING_STATUS: dict[str, dict[str, str]] = {
    # fmt: off
    "E-IO-GENERIC": {
        "python":  "PdfluentIoError",
        "wasm_ts": "OPERATION_FAILED (no dedicated code — gap)",
        "node":    "PdfluentError",
        "dotnet":  "PdfluentIoException",
        "java":    "PdfluentIoException",
        "c_abi":   "PDF_STATUS_ERROR_FILE_NOT_FOUND (partial — gap: generic I/O has no own code)",
    },
    "E-IO-FILE-NOT-FOUND": {
        "python":  "PdfluentIoError",
        "wasm_ts": "OPERATION_FAILED (no dedicated code — gap)",
        "node":    "PdfluentError",
        "dotnet":  "PdfluentIoException (via PDF_STATUS_ERROR_FILE_NOT_FOUND)",
        "java":    "PdfluentIoException",
        "c_abi":   "PDF_STATUS_ERROR_FILE_NOT_FOUND",
    },
    "E-PARSE-INVALID-PDF": {
        "python":  "PdfluentParseError",
        "wasm_ts": "INVALID_PDF",
        "node":    "PdfluentError",
        "dotnet":  "PdfluentParseException (via PDF_STATUS_ERROR_CORRUPT_PDF)",
        "java":    "PdfluentParseException",
        "c_abi":   "PDF_STATUS_ERROR_CORRUPT_PDF",
    },
    "E-PARSE-UNSUPPORTED-VERSION": {
        "python":  "PdfluentParseError (no dedicated subtype — gap)",
        "wasm_ts": "INVALID_PDF (no dedicated code — gap)",
        "node":    "PdfluentError (not raised by any Node operation)",
        "dotnet":  "PdfluentParseException (no dedicated subtype — gap)",
        "java":    "PdfluentParseException (no dedicated subtype — gap)",
        "c_abi":   "PDF_STATUS_ERROR_CORRUPT_PDF (no dedicated code — gap)",
    },
    "E-COMPLIANCE-PDFA-INVALID": {
        "python":  "PdfluentValidationError",
        "wasm_ts": "INVALID_ARGUMENT (no dedicated code — gap)",
        "node":    "PdfluentError (not raised by any Node operation)",
        "dotnet":  "PdfluentValidationException",
        "java":    "PdfluentValidationException",
        "c_abi":   "PDF_STATUS_ERROR_CONVERT (partial)",
    },
    "E-SECURITY-DECRYPTION-FAILED": {
        "python":  "PdfluentEncryptedError",
        "wasm_ts": "OPERATION_FAILED (no dedicated code — gap)",
        "node":    "PdfluentError",
        "dotnet":  "PdfluentPermissionException (via PDF_STATUS_ERROR_INVALID_PASS)",
        "java":    "PdfluentEncryptedDocumentException",
        "c_abi":   "PDF_STATUS_ERROR_INVALID_PASS",
    },
    "E-SECURITY-INVALID-SIGNATURE": {
        "python":  "TBD (no dedicated subtype — gap)",
        "wasm_ts": "OPERATION_FAILED (no dedicated code — gap)",
        "node":    "PdfluentError (not raised by any Node operation)",
        "dotnet":  "TBD (no dedicated subtype — gap)",
        "java":    "TBD (no dedicated subtype — gap)",
        "c_abi":   "PDF_STATUS_ERROR_SIGN (partial — sign covers both signing failure and invalid sig)",
    },
    "E-ENV-UNSUPPORTED-ON-WASM": {
        "python":  "N/A (Python binding is not WASM)",
        "wasm_ts": "OPERATION_FAILED (code exists but not specific — gap)",
        "node":    "N/A (Node binding is not WASM)",
        "dotnet":  "N/A (.NET binding is not WASM)",
        "java":    "N/A (Java binding is not WASM)",
        "c_abi":   "N/A (C ABI is not WASM)",
    },
    "E-ENV-MISSING-DEPENDENCY": {
        "python":  "TBD (no dedicated subtype — gap)",
        "wasm_ts": "N/A (WASM has no native dependencies)",
        "node":    "PdfluentError (not raised by any Node operation)",
        "dotnet":  "TBD (no dedicated subtype — gap)",
        "java":    "TBD (no dedicated subtype — gap)",
        "c_abi":   "PDF_STATUS_ERROR_UNKNOWN (gap)",
    },
    "E-BUDGET-MEMORY-EXCEEDED": {
        "python":  "PdfluentLimitError",
        "wasm_ts": "OPERATION_FAILED (no dedicated code — gap)",
        "node":    "PdfluentError (not raised by any Node operation)",
        "dotnet":  "PdfluentLimitException",
        "java":    "PdfluentLimitException",
        "c_abi":   "PDF_STATUS_ERROR_UNKNOWN (gap — no budget C code)",
    },
    "E-BUDGET-RESOURCE-LIMIT": {
        "python":  "PdfluentLimitError",
        "wasm_ts": "OPERATION_FAILED (no dedicated code — gap)",
        "node":    "PdfluentError",
        "dotnet":  "PdfluentLimitException",
        "java":    "PdfluentLimitException",
        "c_abi":   "PDF_STATUS_ERROR_UNKNOWN (gap — no budget C code)",
    },
    "E-PAGE-OUT-OF-RANGE": {
        "python":  "PdfluentValueError (gap — no dedicated subtype yet)",
        "wasm_ts": "INVALID_ARGUMENT (gap)",
        "node":    "PdfluentError",
        "dotnet":  "PdfluentArgumentException (gap)",
        "java":    "PdfluentPageRangeException",
        "c_abi":   "PDF_STATUS_ERROR_INVALID_ARGUMENT (gap)",
    },
    "E-PAGE-RANGE-INVALID": {
        "python":  "PdfluentValueError (gap — no dedicated subtype yet)",
        "wasm_ts": "INVALID_ARGUMENT (gap)",
        "node":    "PdfluentError (not raised by any Node operation)",
        "dotnet":  "PdfluentArgumentException (gap)",
        "java":    "PdfluentPageRangeException (gap — thrown from the binding's own index check, not from this code)",
        "c_abi":   "PDF_STATUS_ERROR_INVALID_ARGUMENT (gap)",
    },
    "E-UNSUPPORTED": {
        "python":  "PdfluentError (base, no dedicated subtype — gap)",
        "wasm_ts": "OPERATION_FAILED (no dedicated code — gap)",
        "node":    "PdfluentError",
        "dotnet":  "PdfluentException (base — gap)",
        "java":    "PdfluentException (base — gap)",
        "c_abi":   "PDF_STATUS_ERROR_UNKNOWN (gap)",
    },
    "E-INTERNAL": {
        "python":  "PdfluentError (base, no dedicated subtype)",
        "wasm_ts": "OPERATION_FAILED",
        "node":    "PdfluentError",
        "dotnet":  "PdfluentException (base)",
        "java":    "PdfluentException (base)",
        "c_abi":   "PDF_STATUS_ERROR_UNKNOWN",
    },
    # Only the Node column is confirmed for this code (#587); the rest stay TBD.
    "E-EDIT-TEXT-FAILED": {
        "node":    "PdfluentError",
    },
    # fmt: on
}

# ---------------------------------------------------------------------------
# Markdown generation
# ---------------------------------------------------------------------------

HEADER = """\
# PDFluent Error Catalogue

> **Append-only policy.** Error codes are frozen once assigned. You may add
> new codes; you must never rename, remove, or reassign an existing code.
> Any change to the Rust `code()` match arms must be reflected here before
> the release gate passes. The sync gate lives at
> `scripts/release/error_catalogue_sync.sh`.
>
> **Version scope:** PDFluent 1.x (GA hardening branch). Generated from
> `crates/pdfluent/src/error.rs`.
>
> **Node column:** every error a Node operation raises is a `PdfluentError`
> whose `.code` is the code in the first column; there are no per-family
> subclasses (#587). `scripts/release/error_catalogue_sync.sh` refuses a class
> in this column that the package does not export.
>
> **Per-binding columns:** status as of C8 survey. `TBD` = binding terminal
> (C8-3 through C8-8) has not yet confirmed the mapping. `gap` annotations
> indicate that the binding does not yet surface a dedicated type/code for
> this error; the binding terminal for that language should fill in the gap.

"""


def build_catalogue(variants: list[dict]) -> str:
    lines = [HEADER]

    # Summary count
    lines.append(f"**Total error variants:** {len(variants)}\n\n")

    # Main table
    lines.append(
        "| Code | Rust Variant | Description | How to Fix"
        " | Python | WASM/TS | Node | .NET | Java | C ABI |\n"
    )
    lines.append(
        "|------|-------------|-------------|------------|"
        "--------|---------|------|------|------|-------|\n"
    )

    for v in variants:
        code = v["code"]
        variant = v["variant"]
        doc = v["doc"] or "*(no doc comment)*"
        fix = HOW_TO_FIX.get(code, "TBD")
        bs = BINDING_STATUS.get(code, {})
        python = bs.get("python", "TBD")
        wasm = bs.get("wasm_ts", "TBD")
        node = bs.get("node", "TBD")
        dotnet = bs.get("dotnet", "TBD")
        java = bs.get("java", "TBD")
        cabi = bs.get("c_abi", "TBD")

        # Escape pipe characters inside cells
        def esc(s: str) -> str:
            return s.replace("|", "\\|")

        lines.append(
            f"| `{esc(code)}` | `{esc(variant)}` | {esc(doc)}"
            f" | {esc(fix)} | {esc(python)} | {esc(wasm)}"
            f" | {esc(node)} | {esc(dotnet)} | {esc(java)} | {esc(cabi)} |\n"
        )

    # Per-binding gap summary
    lines.append("\n---\n\n## Per-Binding Gap Summary\n\n")
    lines.append(
        "The following gaps were identified during the C8 survey. "
        "Each binding terminal (C8-3 through C8-8) is responsible for "
        "closing the gaps in its language.\n\n"
    )

    # Collect gaps per binding
    gap_map: dict[str, list[str]] = {
        "Python": [],
        "WASM/TS": [],
        "Node": [],
        ".NET": [],
        "Java": [],
        "C ABI": [],
    }
    key_to_label = {
        "python": "Python",
        "wasm_ts": "WASM/TS",
        "node": "Node",
        "dotnet": ".NET",
        "java": "Java",
        "c_abi": "C ABI",
    }
    for v in variants:
        code = v["code"]
        bs = BINDING_STATUS.get(code, {})
        for key, label in key_to_label.items():
            val = bs.get(key, "TBD")
            if "gap" in val.lower() or val == "TBD":
                gap_map[label].append(code)

    for label, gaps in gap_map.items():
        if gaps:
            lines.append(f"### {label}\n\n")
            for code in gaps:
                lines.append(f"- `{code}`\n")
            lines.append("\n")

    lines.append("---\n\n")
    lines.append(
        "*This file is generated by `scripts/docs/generate_error_catalogue.py`. "
        "Do not edit manually — regenerate after updating `error.rs`.*\n"
    )

    return "".join(lines)


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------


def build_sdk_error_codes(variants: list[dict]) -> str:
    """The public anchor list, in the shape pdfluent.com/errors renders."""
    entries = [
        {
            "code": v["code"],
            "variant": v["variant"],
            "what": PUBLIC_WHAT.get(v["code"], v["doc"]),
            "fix": HOW_TO_FIX.get(v["code"], "TBD"),
        }
        for v in variants
    ]
    return json.dumps(entries, indent=2, ensure_ascii=False) + "\n"


def main() -> int:
    if not ERROR_RS.exists():
        print(f"ERROR: {ERROR_RS} not found", file=sys.stderr)
        return 1

    variants = parse_error_rs(ERROR_RS)
    catalogue = build_catalogue(variants)

    OUT_FILE.parent.mkdir(parents=True, exist_ok=True)
    OUT_FILE.write_text(catalogue, encoding="utf-8")
    print(f"Wrote {len(variants)} variants to {OUT_FILE}")

    JSON_OUT_FILE.write_text(build_sdk_error_codes(variants), encoding="utf-8")
    print(f"Wrote {len(variants)} variants to {JSON_OUT_FILE}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
