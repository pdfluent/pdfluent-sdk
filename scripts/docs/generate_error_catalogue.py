#!/usr/bin/env python3
"""Generate docs/error_catalogue.md from crates/pdfluent/src/error.rs.

Parses the Rust source to extract all Error variants, their stable codes,
and doc comments. Outputs a Markdown table with: code, variant, meaning,
how-to-fix, and per-binding mapping column headers.

Exit codes:
  0 -- catalogue written successfully
  1 -- parse error or missing code
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
ERROR_RS = REPO_ROOT / "crates" / "pdfluent" / "src" / "error.rs"
OUT_FILE = REPO_ROOT / "docs" / "error_catalogue.md"

# ---------------------------------------------------------------------------
# Parsing helpers
# ---------------------------------------------------------------------------

# Matches a code() arm such as:
#   Error::Foo { .. } => "E-CATEGORY-SPECIFIC",
CODE_ARM_RE = re.compile(
    r'Error::(\w+)\s*\{[^}]*\}\s*=>\s*"(E-[A-Z0-9\-]+)"'
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

    # Variant declaration: starts with an identifier followed by { or ,
    # preceded by optional doc comments.
    variant_re = re.compile(
        r"((?:^\s*///[^\n]*\n)+)?\s*^\s*([\w]+)\s*\{",
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
    "E-LICENSE-FEATURE-NOT-IN-TIER": (
        "Upgrade your license tier to at least `required_tier`. See "
        "https://pdfluent.com/pricing. Check `capability` to identify which "
        "feature triggered this error."
    ),
    "E-LICENSE-CAPABILITY-NOT-COMPILED": (
        "Rebuild with the `feature_flag` Cargo feature enabled, or use a "
        "pre-built binary that includes the capability."
    ),
    "E-LICENSE-INVALID": (
        "Re-issue the license key or call `activate_license()` again with a "
        "valid key. Check `reason` for the specific parse failure."
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
    "E-INTERNAL": (
        "This should never occur under normal operation. File a bug report at "
        "https://pdfluent.com/support including `message` and `crate_version`."
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
        "node":    "PdfluentIoError",
        "dotnet":  "PdfluentIoException",
        "java":    "PdfluentIoException",
        "c_abi":   "PDF_STATUS_ERROR_FILE_NOT_FOUND (partial — gap: generic I/O has no own code)",
    },
    "E-IO-FILE-NOT-FOUND": {
        "python":  "PdfluentIoError",
        "wasm_ts": "OPERATION_FAILED (no dedicated code — gap)",
        "node":    "PdfluentIoError",
        "dotnet":  "PdfluentIoException (via PDF_STATUS_ERROR_FILE_NOT_FOUND)",
        "java":    "PdfluentIoException",
        "c_abi":   "PDF_STATUS_ERROR_FILE_NOT_FOUND",
    },
    "E-PARSE-INVALID-PDF": {
        "python":  "PdfluentParseError",
        "wasm_ts": "INVALID_PDF",
        "node":    "PdfluentParseError",
        "dotnet":  "PdfluentParseException (via PDF_STATUS_ERROR_CORRUPT_PDF)",
        "java":    "PdfluentParseException",
        "c_abi":   "PDF_STATUS_ERROR_CORRUPT_PDF",
    },
    "E-PARSE-UNSUPPORTED-VERSION": {
        "python":  "PdfluentParseError (no dedicated subtype — gap)",
        "wasm_ts": "INVALID_PDF (no dedicated code — gap)",
        "node":    "PdfluentParseError (no dedicated subtype — gap)",
        "dotnet":  "PdfluentParseException (no dedicated subtype — gap)",
        "java":    "PdfluentParseException (no dedicated subtype — gap)",
        "c_abi":   "PDF_STATUS_ERROR_CORRUPT_PDF (no dedicated code — gap)",
    },
    "E-COMPLIANCE-PDFA-INVALID": {
        "python":  "PdfluentValidationError",
        "wasm_ts": "INVALID_ARGUMENT (no dedicated code — gap)",
        "node":    "TBD (no dedicated subtype)",
        "dotnet":  "PdfluentValidationException",
        "java":    "PdfluentValidationException",
        "c_abi":   "PDF_STATUS_ERROR_CONVERT (partial)",
    },
    "E-SECURITY-DECRYPTION-FAILED": {
        "python":  "PdfluentEncryptedError",
        "wasm_ts": "OPERATION_FAILED (no dedicated code — gap)",
        "node":    "PdfluentPasswordError",
        "dotnet":  "PdfluentPermissionException (via PDF_STATUS_ERROR_INVALID_PASS)",
        "java":    "PdfluentEncryptedDocumentException",
        "c_abi":   "PDF_STATUS_ERROR_INVALID_PASS",
    },
    "E-SECURITY-INVALID-SIGNATURE": {
        "python":  "TBD (no dedicated subtype — gap)",
        "wasm_ts": "OPERATION_FAILED (no dedicated code — gap)",
        "node":    "TBD (no dedicated subtype — gap)",
        "dotnet":  "TBD (no dedicated subtype — gap)",
        "java":    "TBD (no dedicated subtype — gap)",
        "c_abi":   "PDF_STATUS_ERROR_SIGN (partial — sign covers both signing failure and invalid sig)",
    },
    "E-LICENSE-FEATURE-NOT-IN-TIER": {
        "python":  "PdfluentLicenseError (.code = E-LICENSE-FEATURE-NOT-IN-TIER)",
        "wasm_ts": "PdfluentError (.code = E-LICENSE-FEATURE-NOT-IN-TIER)",
        "node":    "PdfluentLicenseError (.code = E-LICENSE-FEATURE-NOT-IN-TIER)",
        "dotnet":  "PdfluentLicenseException (.Code = E-LICENSE-FEATURE-NOT-IN-TIER)",
        "java":    "PdfluentLicenseException (getCode() = E-LICENSE-FEATURE-NOT-IN-TIER)",
        "c_abi":   "PDF_STATUS_ERROR_UNKNOWN (gap — no licence-specific C code in 1.0; tracked)",
    },
    "E-LICENSE-CAPABILITY-NOT-COMPILED": {
        "python":  "PdfluentLicenseError (.code = E-LICENSE-CAPABILITY-NOT-COMPILED)",
        "wasm_ts": "PdfluentError (.code = E-LICENSE-CAPABILITY-NOT-COMPILED)",
        "node":    "PdfluentLicenseError (.code = E-LICENSE-CAPABILITY-NOT-COMPILED)",
        "dotnet":  "PdfluentLicenseException (.Code = E-LICENSE-CAPABILITY-NOT-COMPILED)",
        "java":    "PdfluentLicenseException (getCode() = E-LICENSE-CAPABILITY-NOT-COMPILED)",
        "c_abi":   "PDF_STATUS_ERROR_UNKNOWN (gap — no licence-specific C code in 1.0; tracked)",
    },
    "E-LICENSE-INVALID": {
        "python":  "PdfluentLicenseError (.code = E-LICENSE-INVALID)",
        "wasm_ts": "PdfluentError (.code = E-LICENSE-INVALID)",
        "node":    "PdfluentLicenseError (.code = E-LICENSE-INVALID)",
        "dotnet":  "PdfluentLicenseException (.Code = E-LICENSE-INVALID)",
        "java":    "PdfluentLicenseException (getCode() = E-LICENSE-INVALID)",
        "c_abi":   "PDF_STATUS_ERROR_INVALID_LICENSE (=16) + PDF_STATUS_ERROR_LICENSE_ALREADY_SET (=17) + PDF_STATUS_ERROR_LICENSE_FILE (=18); upstream bindings consolidate all three under .code = E-LICENSE-INVALID",
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
        "node":    "TBD (no dedicated subtype — gap)",
        "dotnet":  "TBD (no dedicated subtype — gap)",
        "java":    "TBD (no dedicated subtype — gap)",
        "c_abi":   "PDF_STATUS_ERROR_UNKNOWN (gap)",
    },
    "E-BUDGET-MEMORY-EXCEEDED": {
        "python":  "PdfluentLimitError",
        "wasm_ts": "OPERATION_FAILED (no dedicated code — gap)",
        "node":    "TBD (no dedicated subtype — gap)",
        "dotnet":  "PdfluentLimitException",
        "java":    "PdfluentLimitException",
        "c_abi":   "PDF_STATUS_ERROR_UNKNOWN (gap — no budget C code)",
    },
    "E-BUDGET-RESOURCE-LIMIT": {
        "python":  "PdfluentLimitError",
        "wasm_ts": "OPERATION_FAILED (no dedicated code — gap)",
        "node":    "TBD (no dedicated subtype — gap)",
        "dotnet":  "PdfluentLimitException",
        "java":    "PdfluentLimitException",
        "c_abi":   "PDF_STATUS_ERROR_UNKNOWN (gap — no budget C code)",
    },
    "E-INTERNAL": {
        "python":  "PdfluentError (base, no dedicated subtype)",
        "wasm_ts": "OPERATION_FAILED",
        "node":    "PdfluentOperationError",
        "dotnet":  "PdfluentException (base)",
        "java":    "PdfluentException (base)",
        "c_abi":   "PDF_STATUS_ERROR_UNKNOWN",
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


def main() -> int:
    if not ERROR_RS.exists():
        print(f"ERROR: {ERROR_RS} not found", file=sys.stderr)
        return 1

    variants = parse_error_rs(ERROR_RS)
    catalogue = build_catalogue(variants)

    OUT_FILE.parent.mkdir(parents=True, exist_ok=True)
    OUT_FILE.write_text(catalogue, encoding="utf-8")
    print(f"Wrote {len(variants)} variants to {OUT_FILE}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
