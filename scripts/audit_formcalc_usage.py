#!/usr/bin/env python3
"""
FormCalc usage audit script.

Scans XFA PDFs for FormCalc script blocks and aggregates function call
frequencies. Compares against implemented builtins in
crates/formcalc-interpreter/src/builtins.rs.

Usage:
    python3 scripts/audit_formcalc_usage.py --input crates/xfa-golden-tests/golden/
    python3 scripts/audit_formcalc_usage.py --input crates/xfa-golden-tests/golden/ corpus/
    python3 scripts/audit_formcalc_usage.py --input crates/xfa-golden-tests/golden/ \
        --output benchmarks/formcalc_usage_corpus.json
"""

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import zlib
from collections import defaultdict
from pathlib import Path
from typing import Any


# ---------------------------------------------------------------------------
# Full Adobe FormCalc Reference function list (XFA 3.3 §25)
# Keyed lowercase → canonical casing per spec.
# ---------------------------------------------------------------------------
ALL_FORMCALC_FUNCTIONS: dict[str, str] = {
    # Arithmetic (§25.3)
    "abs": "Abs",
    "avg": "Avg",
    "ceil": "Ceil",
    "count": "Count",
    "floor": "Floor",
    "max": "Max",
    "min": "Min",
    "mod": "Mod",
    "round": "Round",
    "sum": "Sum",
    # String (§25.4)
    "at": "At",
    "concat": "Concat",
    "decode": "Decode",
    "encode": "Encode",
    "format": "Format",
    "left": "Left",
    "len": "Len",
    "lower": "Lower",
    "ltrim": "Ltrim",
    "parse": "Parse",
    "replace": "Replace",
    "right": "Right",
    "rtrim": "Rtrim",
    "space": "Space",
    "str": "Str",
    "stuff": "Stuff",
    "substr": "Substr",
    "trim": "Trim",
    "upper": "Upper",
    "uuid": "Uuid",
    "wordnum": "WordNum",
    "unittype": "UnitType",
    "unitvalue": "UnitValue",
    # Logical (§25.5)
    "choose": "Choose",
    "exists": "Exists",
    "hasvalue": "HasValue",
    "if": "If",
    "isfinite": "IsFinite",
    "isnull": "IsNull",
    "isnumber": "IsNumber",
    "isstring": "IsString",
    "not": "Not",
    "null": "Null",
    "oneof": "Oneof",
    "within": "Within",
    # Date/Time (§25.6)
    "date": "Date",
    "date2num": "Date2Num",
    "datefmt": "DateFmt",
    "dategmt": "DateGMT",
    "isodate2num": "ISODate2Num",
    "isotime2num": "ISOTime2Num",
    "localdatefmt": "LocalDateFmt",
    "localtimefmt": "LocalTimeFmt",
    "num2date": "Num2Date",
    "num2gmtime": "Num2GMTime",
    "num2time": "Num2Time",
    "time": "Time",
    "time2num": "Time2Num",
    "timefmt": "TimeFmt",
    "timegmt": "TimeGMT",
    # Financial (§25.7)
    "apr": "Apr",
    "cterm": "CTerm",
    "fv": "FV",
    "ipmt": "IPmt",
    "npv": "NPV",
    "pmt": "Pmt",
    "ppmt": "PPmt",
    "pv": "PV",
    "rate": "Rate",
    "term": "Term",
    # Misc / Node (§25.8)
    "eval": "Eval",
    "get": "Get",
    "post": "Post",
    "put": "Put",
    "ref": "Ref",
    # Not in some editions but commonly used
    "index": "Index",
    "applyxsl": "ApplyXSL",
    "translate": "Translate",
}

# Functions implemented in builtins.rs (from match arms in call_builtin)
IMPLEMENTED: set[str] = {
    # Arithmetic
    "abs", "avg", "ceil", "count", "floor", "max", "min", "mod", "round", "sum",
    # String
    "at", "concat", "decode", "encode", "format", "left", "len", "lower",
    "ltrim", "parse", "replace", "right", "rtrim", "space", "str", "stuff",
    "substr", "unittype", "unitvalue", "upper", "uuid", "wordnum",
    # Logical
    "choose", "exists", "if", "oneof", "within",
    # Date/Time
    "date", "date2num", "datefmt", "dategmt", "isodate2num", "isotime2num",
    "localdatefmt", "localtimefmt", "num2date", "num2gmtime", "time",
    "time2num", "timegmt", "timefmt", "num2time",
    # Financial
    "apr", "cterm", "fv", "ipmt", "npv", "pmt", "ppmt", "pv", "rate", "term",
    # Misc
    "eval", "hasvalue", "null", "ref", "get", "post", "put",
}

# Functions that are present but use todo_builtin() (partial / stub).
# Verified by grep for todo_builtin in crates/formcalc-interpreter/src/builtins.rs:
#   builtin_parse   → todo_builtin("Parse", ...)
#   builtin_eval    → todo_builtin("Eval", ...)
#   builtin_ref     → todo_builtin("Ref", ...)
#   builtin_get     → todo_builtin("Get", ...)
#   builtin_post    → todo_builtin("Post", ...)
#   builtin_put     → todo_builtin("Put", ...)
PARTIAL: set[str] = {
    "parse",  # Picture mask parsing not implemented; always returns error
    "eval",   # Eval() – full sandboxed re-entry not yet wired
    "get",    # HTTP GET stub – always returns Null
    "post",   # HTTP POST stub – always returns Null
    "put",    # HTTP PUT stub – always returns Null
    "ref",    # Ref() – SOM bridge incomplete
}

# Known behavioural divergence notes
BEHAVIOUR_NOTES: dict[str, str] = {
    "format": "Format() picture mask: not all picture clauses implemented (date/time masks ok, number masks partial)",
    "parse": "Parse() picture mask: mirrors Format() gaps",
    "wordnum": "WordNum() locale support limited to 'en' and 'de'; other locales fall back to 'en'",
    "date2num": "Date2Num() handles ISO 8601 and common US/EU patterns; edge-case locale patterns may differ from Acrobat",
    "num2date": "Num2Date() uses Gregorian proleptic calendar; Hijri / other calendars not supported",
    "isodate2num": "ISODate2Num() parses T-separated ISO 8601; fractional seconds silently truncated",
    "decode": "Decode('html') entity set limited to HTML 4 named entities; numeric entities &#x; handled",
    "encode": "Encode('html') only encodes &<>'\"; full HTML 5 entity expansion not implemented",
    "eval": "Eval() does not support full re-entrant interpreter context (SOM bridge not wired)",
    "get":  "Get() is a no-op stub; always returns Null",
    "post": "Post() is a no-op stub; always returns Null",
    "put":  "Put() is a no-op stub; always returns Null",
    "uuid": "Uuid() uses rand-based UUID v4; not seeded from XFA context",
    "str":  "Str() with negative width uses absolute value per spec; edge behaviour on NaN may differ",
    "mod":  "Mod() follows IEEE 754 remainder; Acrobat uses truncated division – differs for negatives",
    "round": "Round() uses banker's rounding (round-half-to-even); Acrobat uses round-half-away-from-zero",
}


# ---------------------------------------------------------------------------
# PDF / XFA extraction helpers
# ---------------------------------------------------------------------------

def _run_mutool(args: list[str]) -> bytes:
    """Run mutool and return stdout bytes. Returns b'' on error."""
    try:
        result = subprocess.run(
            ["mutool"] + args,
            capture_output=True,
            timeout=30,
        )
        return result.stdout
    except (subprocess.TimeoutExpired, FileNotFoundError, OSError):
        return b""


def _extract_xfa_object_ids(pdf_path: str) -> list[tuple[str, int]]:
    """
    Parse the AcroForm /XFA entry and return [(name, object_id), ...].

    Handles two XFA storage formats:
    - Array form: /XFA [(preamble) 1 0 R (template) 3 0 R ...]
      Used by Acrobat/Designer for multi-packet XDP envelopes.
    - Single-stream form: /XFA 1 0 R
      Used by minimal XDP PDFs where the entire XDP is one stream.
    """
    # Find root catalog
    trailer = _run_mutool(["show", pdf_path, "trailer"]).decode("latin-1", errors="replace")
    root_match = re.search(r"/Root\s+(\d+)\s+\d+\s+R", trailer)
    if not root_match:
        return []

    root_id = root_match.group(1)
    catalog = _run_mutool(["show", pdf_path, root_id]).decode("latin-1", errors="replace")

    acroform_match = re.search(r"/AcroForm\s+(\d+)\s+\d+\s+R", catalog)
    if not acroform_match:
        return []

    af_id = acroform_match.group(1)
    af_obj = _run_mutool(["show", pdf_path, af_id]).decode("latin-1", errors="replace")

    # Try array form first: /XFA [ ... ]
    xfa_section = re.search(r"/XFA\s*\[([^\]]+)\]", af_obj, re.DOTALL)
    if xfa_section:
        xfa_content = xfa_section.group(1)
        pairs = re.findall(r"\((\w+)\)\s+(\d+)\s+\d+\s+R", xfa_content)
        return [(name, int(obj_id)) for name, obj_id in pairs]

    # Fall back to single-stream form: /XFA N 0 R
    single_match = re.search(r"/XFA\s+(\d+)\s+\d+\s+R", af_obj)
    if single_match:
        return [("xdp", int(single_match.group(1)))]

    return []


def _extract_stream_text(pdf_path: str, obj_id: int) -> str:
    """Extract decoded stream text for a PDF object."""
    raw = _run_mutool(["show", pdf_path, str(obj_id)]).decode("latin-1", errors="replace")
    return raw


def extract_xfa_xml(pdf_path: str) -> list[tuple[str, str]]:
    """
    Extract XFA XML parts from a PDF.
    Returns [(part_name, xml_text), ...].
    """
    parts = []
    ids = _extract_xfa_object_ids(pdf_path)
    if not ids:
        return parts

    for name, obj_id in ids:
        text = _extract_stream_text(pdf_path, obj_id)
        if text.strip():
            parts.append((name, text))
    return parts


# ---------------------------------------------------------------------------
# FormCalc function extraction from XML
# ---------------------------------------------------------------------------

# Regex to find <script contentType="application/x-formcalc"> blocks.
#
# Important: mutool decodes compressed XFA streams into text where the XML
# closing '>' on a tag is sometimes on its own line, e.g.:
#   <script contentType="application/x-formcalc"\n>...body...</script\n>
# The [^>]* after the contentType value therefore must allow newlines, which
# it does because [^>] matches any character except '>'.  The closing tag
# pattern uses [^>]* to also tolerate whitespace/newlines before the final '>'.
_SCRIPT_RE = re.compile(
    r'<script[^>]+contentType\s*=\s*["\']application/x-formcalc["\'][^>]*>'
    r'(.*?)</script[^>]*>',
    re.DOTALL | re.IGNORECASE,
)

# Also match CDATA-wrapped scripts
_CDATA_RE = re.compile(r'<!\[CDATA\[(.*?)\]\]>', re.DOTALL)

# FormCalc identifier followed by '(' — function call
# Identifiers: start with letter or _, followed by alnum/_
# We exclude keywords: if/for/while/do/break/return/var/func/endfunc/end/endif/
#                      endfor/endwhile/else/elseif/in/not/and/or/eq/ne/lt/le/gt/ge/null
_FC_KEYWORDS = {
    "if", "for", "while", "do", "break", "return", "var",
    "func", "endfunc", "end", "endif", "endfor", "endwhile",
    "else", "elseif", "in", "not", "and", "or", "eq", "ne",
    "lt", "le", "gt", "ge", "null", "continue", "step",
}

# Match identifier( but NOT when preceded by '.' (method calls on objects).
# This filters out patterns like xfa.host.resetData() where 'resetData' is
# a method on the XFA object model, not a FormCalc built-in function.
_FUNC_CALL_RE = re.compile(r'(?<!\.)(?<!\w)\b([A-Za-z_][A-Za-z0-9_]*)\s*\(')


def _extract_user_defined_funcs(script_body: str) -> set[str]:
    """
    Return names of user-defined functions declared in this script body
    via the `func name(...)` syntax.
    """
    return {
        m.group(1).lower()
        for m in re.finditer(r'\bfunc\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(', script_body)
    }


def extract_formcalc_calls(xml_text: str) -> list[str]:
    """
    Return list of built-in function names called in FormCalc script blocks.

    User-defined functions (declared with `func name(...)`) are excluded
    so that only calls to the FormCalc built-in library are counted.
    """
    calls = []
    # Collect ALL user-defined function names across all scripts in this XML.
    all_user_funcs: set[str] = set()
    script_bodies: list[str] = []
    for script_match in _SCRIPT_RE.finditer(xml_text):
        body = script_match.group(1)
        cdata_match = _CDATA_RE.search(body)
        if cdata_match:
            body = cdata_match.group(1)
        all_user_funcs |= _extract_user_defined_funcs(body)
        script_bodies.append(body)

    for body in script_bodies:
        for m in _FUNC_CALL_RE.finditer(body):
            name_lower = m.group(1).lower()
            if name_lower in _FC_KEYWORDS:
                continue
            if name_lower in all_user_funcs:
                continue
            calls.append(name_lower)
    return calls


# ---------------------------------------------------------------------------
# Main scan logic
# ---------------------------------------------------------------------------

def scan_directory(input_dirs: list[str]) -> dict[str, Any]:
    """Scan all PDFs in input_dirs and return aggregated usage data."""
    pdf_paths: list[str] = []
    for d in input_dirs:
        p = Path(d)
        if p.is_file() and p.suffix.lower() == ".pdf":
            pdf_paths.append(str(p))
        elif p.is_dir():
            pdf_paths.extend(str(pp) for pp in sorted(p.rglob("*.pdf")))

    if not pdf_paths:
        print(f"WARNING: No PDFs found in {input_dirs}", file=sys.stderr)

    function_counts: dict[str, int] = defaultdict(int)
    scripts_found = 0
    pdfs_with_fc = 0
    pdfs_scanned = 0
    per_pdf: list[dict[str, Any]] = []

    for pdf_path in pdf_paths:
        pdfs_scanned += 1
        pdf_calls: list[str] = []

        xfa_parts = extract_xfa_xml(pdf_path)
        has_fc = False

        for part_name, xml_text in xfa_parts:
            # Count <script contentType="application/x-formcalc"> occurrences
            count = len(_SCRIPT_RE.findall(xml_text))
            scripts_found += count
            if count > 0:
                has_fc = True
            calls = extract_formcalc_calls(xml_text)
            pdf_calls.extend(calls)

        if has_fc:
            pdfs_with_fc += 1

        local_counts: dict[str, int] = defaultdict(int)
        for c in pdf_calls:
            function_counts[c] += 1
            local_counts[c] += 1

        per_pdf.append({
            "pdf": os.path.basename(pdf_path),
            "xfa_parts": [p for p, _ in xfa_parts],
            "formcalc_scripts": sum(
                len(_SCRIPT_RE.findall(xml))
                for _, xml in xfa_parts
            ),
            "function_calls": dict(sorted(local_counts.items(), key=lambda x: -x[1])),
        })

    # Sort by frequency descending
    sorted_counts = dict(
        sorted(function_counts.items(), key=lambda x: -x[1])
    )

    return {
        "meta": {
            "pdfs_scanned": pdfs_scanned,
            "pdfs_with_formcalc": pdfs_with_fc,
            "formcalc_scripts_found": scripts_found,
            "unique_functions_found": len(sorted_counts),
        },
        "function_frequency": sorted_counts,
        "per_pdf": per_pdf,
    }


# ---------------------------------------------------------------------------
# Coverage classification
# ---------------------------------------------------------------------------

def classify_function(name_lower: str) -> str:
    """Return 'implemented', 'partial', or 'missing'."""
    if name_lower in IMPLEMENTED:
        if name_lower in PARTIAL:
            return "partial"
        return "implemented"
    return "missing"


def build_coverage_table(
    function_frequency: dict[str, int],
    top_n: int = 50,
) -> list[dict[str, Any]]:
    """Build coverage table for top_n functions."""
    rows = []
    for name_lower, freq in list(function_frequency.items())[:top_n]:
        canonical = ALL_FORMCALC_FUNCTIONS.get(name_lower, name_lower)
        status = classify_function(name_lower)
        note = BEHAVIOUR_NOTES.get(name_lower, "")
        rows.append({
            "function": canonical,
            "frequency": freq,
            "status": status,
            "note": note,
        })
    return rows


# ---------------------------------------------------------------------------
# Output generation
# ---------------------------------------------------------------------------

def write_json(data: dict[str, Any], output_path: str) -> None:
    """Write JSON output."""
    os.makedirs(os.path.dirname(output_path) or ".", exist_ok=True)
    with open(output_path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)
    print(f"JSON written: {output_path}", file=sys.stderr)


def validate_json_schema(data: dict[str, Any]) -> None:
    """Validate that required keys are present in the JSON output."""
    assert "meta" in data, "Missing 'meta' key"
    assert "function_frequency" in data, "Missing 'function_frequency' key"
    assert "per_pdf" in data, "Missing 'per_pdf' key"
    meta = data["meta"]
    assert "pdfs_scanned" in meta
    assert "pdfs_with_formcalc" in meta
    assert "formcalc_scripts_found" in meta
    assert "unique_functions_found" in meta
    assert isinstance(data["function_frequency"], dict)
    assert isinstance(data["per_pdf"], list)
    print("JSON schema validation: OK", file=sys.stderr)


def write_coverage_report(
    data: dict[str, Any],
    output_path: str,
) -> None:
    """Write FORMCALC_COVERAGE_AUDIT.md."""
    meta = data["meta"]
    freq = data["function_frequency"]
    table = build_coverage_table(freq, top_n=50)

    missing_high_freq = [
        r for r in table if r["status"] == "missing"
    ][:5]

    partial_list = [r for r in table if r["status"] == "partial"]
    impl_list = [r for r in table if r["status"] == "implemented"]

    lines: list[str] = []
    lines += [
        "# FormCalc Coverage Audit",
        "",
        f"**Corpus:** {meta['pdfs_scanned']} PDFs scanned, "
        f"{meta['pdfs_with_formcalc']} containing FormCalc scripts, "
        f"{meta['formcalc_scripts_found']} script blocks total.",
        f"**Unique functions found:** {meta['unique_functions_found']}",
        "",
        "---",
        "",
        "## Top-50 Functions by Corpus Frequency",
        "",
        "| # | Function | Corpus Frequency | Status | Notes |",
        "|---|----------|-----------------|--------|-------|",
    ]

    for i, row in enumerate(table, 1):
        status_badge = {
            "implemented": "✅ implemented",
            "partial": "⚠️ partial",
            "missing": "❌ missing",
        }.get(row["status"], row["status"])
        note = row["note"].replace("|", "\\|") if row["note"] else "—"
        lines.append(
            f"| {i} | `{row['function']}` | {row['frequency']} "
            f"| {status_badge} | {note} |"
        )

    lines += [
        "",
        "---",
        "",
        "## Top-5 Missing High-Frequency Functions",
        "",
    ]

    if missing_high_freq:
        for row in missing_high_freq:
            lines += [
                f"### `{row['function']}`",
                "",
                f"- **Corpus frequency:** {row['frequency']}",
                f"- **Status:** {row['status']}",
                f"- **Note:** {row['note'] if row['note'] else 'No known notes'}",
                "",
            ]
    else:
        lines.append("_All top-frequency functions are implemented or partial._")
        lines.append("")

    lines += [
        "---",
        "",
        "## Adobe FormCalc Reference Compliance",
        "",
        "Functions that are implemented but have known behavioural differences",
        "from the Adobe FormCalc Reference (XFA 3.3 §25):",
        "",
        "| Function | Issue |",
        "|----------|-------|",
    ]

    for name_lower, note in BEHAVIOUR_NOTES.items():
        canonical = ALL_FORMCALC_FUNCTIONS.get(name_lower, name_lower)
        lines.append(f"| `{canonical}` | {note.replace('|', chr(124))} |")

    # Spec-gap: functions in the full spec not implemented at all
    spec_missing = sorted(
        set(ALL_FORMCALC_FUNCTIONS.keys()) - IMPLEMENTED,
        key=lambda n: ALL_FORMCALC_FUNCTIONS[n],
    )
    spec_partial = sorted(PARTIAL, key=lambda n: ALL_FORMCALC_FUNCTIONS.get(n, n))

    lines += [
        "",
        "---",
        "",
        "## Spec-Gap Analysis (XFA 3.3 §25 vs builtins.rs)",
        "",
        "Functions present in the Adobe FormCalc Reference §25 but **not implemented** "
        "in `crates/formcalc-interpreter/src/builtins.rs`:",
        "",
        "| Function | Category | Spec Section |",
        "|----------|----------|--------------|",
    ]

    _spec_sections: dict[str, tuple[str, str]] = {
        "trim":      ("String",   "§25.4.20"),
        "isnull":    ("Logical",  "§25.5.5"),
        "isnumber":  ("Logical",  "§25.5.6"),
        "isstring":  ("Logical",  "§25.5.7"),
        "isfinite":  ("Logical",  "§25.5.4"),
        "not":       ("Logical",  "§25.5.8"),
        "index":     ("String",   "§25.4.8"),
        "translate": ("String",   "§25.4.21"),
        "applyxsl":  ("Misc",     "§25.8.1"),
    }

    for fn_lower in spec_missing:
        canonical = ALL_FORMCALC_FUNCTIONS[fn_lower]
        category, section = _spec_sections.get(fn_lower, ("Misc", "§25"))
        lines.append(f"| `{canonical}` | {category} | {section} |")

    lines += [
        "",
        "Functions present in `builtins.rs` but calling `todo_builtin()` (stubs):",
        "",
        "| Function | Stub Behaviour |",
        "|----------|----------------|",
        "| `Parse`  | Always returns error; picture mask parser not implemented |",
        "| `Eval`   | Always returns error; re-entrant interpreter not wired |",
        "| `Ref`    | Always returns error; SOM bridge incomplete |",
        "| `Get`    | Always returns Null; HTTP client not implemented |",
        "| `Post`   | Always returns Null; HTTP client not implemented |",
        "| `Put`    | Always returns Null; HTTP client not implemented |",
        "",
        "---",
        "",
        "## Implementation Summary",
        "",
        f"- **Corpus functions found:** {len(table)}",
        f"- **Corpus coverage:** "
        f"{sum(1 for r in table if r['status'] in ('implemented', 'partial'))} / {len(table)} "
        f"({100 * sum(1 for r in table if r['status'] in ('implemented', 'partial')) // max(len(table), 1)}%)",
        f"- **Partial / stub (corpus):** {len(partial_list)}",
        f"- **Missing (corpus top-50):** "
        f"{sum(1 for r in table if r['status'] == 'missing')}",
        "",
        f"- **Spec functions catalogued:** {len(ALL_FORMCALC_FUNCTIONS)}",
        f"- **Implemented in builtins.rs:** {len(IMPLEMENTED)} "
        f"({100 * len(IMPLEMENTED) // len(ALL_FORMCALC_FUNCTIONS)}%)",
        f"- **Stubs (todo_builtin):** {len(spec_partial)}",
        f"- **Completely missing from spec:** {len(spec_missing)} "
        f"({', '.join(ALL_FORMCALC_FUNCTIONS[n] for n in spec_missing)})",
        "",
    ]

    os.makedirs(os.path.dirname(output_path) or ".", exist_ok=True)
    with open(output_path, "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")
    print(f"Coverage report written: {output_path}", file=sys.stderr)


def write_issues_file(
    data: dict[str, Any],
    output_path: str,
) -> None:
    """Write FORMCALC_MISSING_ISSUES.md with issue blocks for top-5 missing.

    If the corpus shows 100% coverage (all corpus functions implemented),
    the issues are generated from the spec-gap instead — functions that exist
    in the Adobe FormCalc Reference §25 but are absent from builtins.rs.
    The spec-gap list is sorted by implementation priority (simplest first).
    """
    freq = data["function_frequency"]
    table = build_coverage_table(freq, top_n=50)
    missing_corpus = [r for r in table if r["status"] == "missing"]

    # Spec-gap: functions in spec but completely absent from builtins.rs
    _SPEC_PRIORITY_ORDER = [
        "trim",       # trivial string op, very commonly needed
        "isnull",     # trivial logical, essential for defensive scripting
        "isnumber",   # trivial logical, essential for validation
        "isstring",   # trivial logical
        "isfinite",   # trivial logical
        "not",        # logical NOT function form
        "index",      # string index search
        "translate",  # string character substitution
        "applyxsl",   # XSLT transform – complex, rarely needed
    ]
    spec_missing_ordered = [
        fn for fn in _SPEC_PRIORITY_ORDER
        if fn in ALL_FORMCALC_FUNCTIONS and fn not in IMPLEMENTED
    ]
    missing_top5 = (
        missing_corpus[:5]
        if missing_corpus
        else [
            {
                "function": ALL_FORMCALC_FUNCTIONS[fn],
                "frequency": 0,
                "status": "missing",
                "note": "",
            }
            for fn in spec_missing_ordered[:5]
        ]
    )

    # Issue metadata per function
    issue_meta: dict[str, dict[str, str]] = {
        "trim": {
            "section": "§25.4.20",
            "page": "1101",
            "signature": "Trim(s1)",
            "description": (
                "Remove leading and trailing whitespace from a string value. "
                "Equivalent to `Ltrim(Rtrim(s))` but required as a single "
                "built-in per spec."
            ),
            "effort": "XS (< 1h)",
            "files": "crates/formcalc-interpreter/src/builtins.rs",
            "test_cases": (
                'Trim("  hello  ") == "hello"\n'
                'Trim("\\t\\nfoo\\n") == "foo"\n'
                'Trim("") == ""\n'
                'Trim(null) == null'
            ),
        },
        "isnull": {
            "section": "§25.5.5",
            "page": "1108",
            "signature": "IsNull(n1)",
            "description": (
                "Return 1 when n1 is null, 0 otherwise. "
                "Required for defensive scripting in XFA forms."
            ),
            "effort": "XS (< 1h)",
            "files": "crates/formcalc-interpreter/src/builtins.rs",
            "test_cases": (
                "IsNull(null) == 1\n"
                'IsNull("") == 0\n'
                "IsNull(0) == 0\n"
                "IsNull(HasValue(null)) == 0"
            ),
        },
        "isnumber": {
            "section": "§25.5.6",
            "page": "1109",
            "signature": "IsNumber(n1)",
            "description": (
                "Return 1 when n1 can be interpreted as a number, 0 otherwise. "
                "Commonly used for validation guard clauses."
            ),
            "effort": "XS (< 1h)",
            "files": "crates/formcalc-interpreter/src/builtins.rs",
            "test_cases": (
                "IsNumber(42) == 1\n"
                'IsNumber("3.14") == 1\n'
                'IsNumber("abc") == 0\n'
                "IsNumber(null) == 0"
            ),
        },
        "isstring": {
            "section": "§25.5.7",
            "page": "1109",
            "signature": "IsString(s1)",
            "description": (
                "Return 1 when s1 is a string value, 0 otherwise. "
                "Used alongside IsNumber for type dispatching in dynamic scripts."
            ),
            "effort": "XS (< 1h)",
            "files": "crates/formcalc-interpreter/src/builtins.rs",
            "test_cases": (
                'IsString("hello") == 1\n'
                "IsString(42) == 0\n"
                "IsString(null) == 0"
            ),
        },
        "isfinite": {
            "section": "§25.5.4",
            "page": "1108",
            "signature": "IsFinite(n1)",
            "description": (
                "Return 1 when n1 is a finite number (not NaN, not Inf), "
                "0 otherwise. Required for robust financial calculation guards."
            ),
            "effort": "XS (< 1h)",
            "files": "crates/formcalc-interpreter/src/builtins.rs",
            "test_cases": (
                "IsFinite(42) == 1\n"
                "IsFinite(0 / 0) == 0  // NaN\n"
                "IsFinite(1e308 * 10) == 0  // Inf\n"
                "IsFinite(null) == 0"
            ),
        },
        "not": {
            "section": "§25.5.8",
            "page": "1110",
            "signature": "Not(n1)",
            "description": (
                "Logical NOT: return 1 if n1 is 0 or null, 0 otherwise. "
                "Note: the `not` keyword is also a unary operator in FC grammar; "
                "this is the function form."
            ),
            "effort": "XS (< 1h)",
            "files": "crates/formcalc-interpreter/src/builtins.rs",
            "test_cases": (
                "Not(0) == 1\n"
                "Not(1) == 0\n"
                "Not(null) == 1\n"
                'Not("") == 1'
            ),
        },
        "translate": {
            "section": "§25.4.21",
            "page": "1102",
            "signature": "Translate(s1, s2, s3)",
            "description": (
                "Replace each character in s1 that appears in s2 with the "
                "corresponding character in s3 (like Unix `tr`). "
                "Used for character substitution in form data normalisation."
            ),
            "effort": "S (2–4h)",
            "files": "crates/formcalc-interpreter/src/builtins.rs",
            "test_cases": (
                'Translate("abc", "ac", "AC") == "AbC"\n'
                'Translate("hello", "aeiou", "AEIOU") == "hEllO"\n'
                'Translate("", "a", "A") == ""\n'
                "Translate(null, \"a\", \"A\") == null"
            ),
        },
        "applyxsl": {
            "section": "§25.8.1",
            "page": "1130",
            "signature": "ApplyXSL(s1, s2)",
            "description": (
                "Apply an XSLT stylesheet (s2) to an XML document (s1) and "
                "return the transformed string. "
                "Rarely used in typical forms but present in government templates."
            ),
            "effort": "XL (2–4 days, requires XSLT engine)",
            "files": (
                "crates/formcalc-interpreter/src/builtins.rs, "
                "Cargo.toml (add xslt crate)"
            ),
            "test_cases": (
                'ApplyXSL("<r><a>1</a></r>", "<xsl:stylesheet...>") contains "1"'
            ),
        },
    }

    source_note = (
        "from corpus frequency analysis (top-50 corpus functions that are missing)"
        if missing_corpus
        else "from Adobe FormCalc Reference §25 spec-gap (corpus shows 100% coverage; "
             "these functions are absent from `builtins.rs` entirely)"
    )

    lines: list[str] = [
        "# FormCalc Missing Functions — Issue Backlog",
        "",
        "Auto-generated from `scripts/audit_formcalc_usage.py`.",
        f"Issues sourced {source_note}.",
        "Each block is a self-contained issue description.",
        "",
    ]

    if not missing_top5:
        lines.append(
            "_No missing functions found. "
            "All spec functions are implemented or stubbed._"
        )
    else:
        for rank, row in enumerate(missing_top5, 1):
            fn_lower = row["function"].lower()
            meta = issue_meta.get(fn_lower, {})
            lines += [
                f"---",
                "",
                f"## Issue {rank}: Implement `{row['function']}()`",
                "",
                f"**Label:** `formcalc`, `missing-builtin`  ",
                f"**Priority:** P{rank}  ",
                f"**Corpus frequency:** {row['frequency']}  ",
                "",
                "### Goal",
                "",
                meta.get("description", f"Implement `{row['function']}` per XFA 3.3."),
                "",
                "### Adobe Spec Reference",
                "",
                f"XFA 3.3 FormCalc Reference "
                f"{meta.get('section', '§25')} (p{meta.get('page', '?')}): "
                f"`{meta.get('signature', row['function'] + '(...)')}`",
                "",
                "### Estimated Effort",
                "",
                meta.get("effort", "S (2–4h)"),
                "",
                "### Files to Touch",
                "",
                f"`{meta.get('files', 'crates/formcalc-interpreter/src/builtins.rs')}`",
                "",
                "### Example Test Cases",
                "",
                "```",
                meta.get("test_cases", f"// TODO: add test cases for {row['function']}"),
                "```",
                "",
            ]

    os.makedirs(os.path.dirname(output_path) or ".", exist_ok=True)
    with open(output_path, "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")
    print(f"Issues file written: {output_path}", file=sys.stderr)


# ---------------------------------------------------------------------------
# CLI entry point
# ---------------------------------------------------------------------------

def main() -> None:
    parser = argparse.ArgumentParser(
        description="Audit FormCalc function usage across XFA PDF corpus."
    )
    parser.add_argument(
        "--input",
        nargs="+",
        required=True,
        help="One or more directories (or individual PDF files) to scan.",
    )
    parser.add_argument(
        "--output",
        default="benchmarks/formcalc_usage_corpus.json",
        help="Path for the JSON output (default: benchmarks/formcalc_usage_corpus.json).",
    )
    parser.add_argument(
        "--coverage-report",
        default="benchmarks/FORMCALC_COVERAGE_AUDIT.md",
        help="Path for the Markdown coverage report.",
    )
    parser.add_argument(
        "--issues-file",
        default="benchmarks/FORMCALC_MISSING_ISSUES.md",
        help="Path for the missing-functions issues file.",
    )
    parser.add_argument(
        "--no-reports",
        action="store_true",
        help="Only write the JSON file, skip Markdown reports.",
    )
    args = parser.parse_args()

    print(f"Scanning: {args.input}", file=sys.stderr)
    data = scan_directory(args.input)

    meta = data["meta"]
    print(
        f"Scanned {meta['pdfs_scanned']} PDFs, "
        f"{meta['pdfs_with_formcalc']} with FormCalc, "
        f"{meta['formcalc_scripts_found']} script blocks, "
        f"{meta['unique_functions_found']} unique functions.",
        file=sys.stderr,
    )

    write_json(data, args.output)
    validate_json_schema(data)

    if not args.no_reports:
        write_coverage_report(data, args.coverage_report)
        write_issues_file(data, args.issues_file)

    # Print summary to stdout
    print(json.dumps(
        {
            "pdfs_scanned": meta["pdfs_scanned"],
            "pdfs_with_formcalc": meta["pdfs_with_formcalc"],
            "formcalc_scripts_found": meta["formcalc_scripts_found"],
            "unique_functions_found": meta["unique_functions_found"],
            "top_10": list(data["function_frequency"].items())[:10],
        },
        indent=2,
    ))


if __name__ == "__main__":
    main()
