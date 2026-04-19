#!/usr/bin/env python3
"""Apply exclusion filters to a corpus inventory and produce a filtered subset.

Modes:
  all                — no exclusions applied (pass-through)
  non-adversarial    — exclude adversarial inputs, encrypted PDFs, tiny files, zero-page docs
  xfa-only           — all non-adversarial exclusions + exclude non-XFA PDFs
  xfa-with-reference — xfa-only + exclude PDFs without a pdfRest reference

Usage:
    python3 scripts/apply_corpus_exclusions.py \\
        --inventory benchmarks/corpus_inventory.json \\
        --mode xfa-with-reference \\
        --output benchmarks/corpus_b.json
"""
import argparse
import json
import sys
from pathlib import Path

ADVERSARIAL_PREFIXES = ("ghostscript-", "mozilla-", "pdfium-")
TINY_FILE_THRESHOLD_BYTES = 1024

MODES = ("all", "non-adversarial", "xfa-only", "xfa-with-reference")


# ---------------------------------------------------------------------------
# Exclusion logic
# ---------------------------------------------------------------------------

def is_adversarial(entry: dict) -> bool:
    fname = entry.get("file", "").lower()
    return any(p in fname for p in ADVERSARIAL_PREFIXES)


def is_encrypted(entry: dict) -> bool:
    """The inventory does not directly flag encryption; check tags or parse_error."""
    tags = entry.get("tags", [])
    if "encrypted" in tags:
        return True
    parse_error = entry.get("parse_error", "") or ""
    return "encrypt" in parse_error.lower() or "password" in parse_error.lower()


def is_tiny(entry: dict) -> bool:
    return entry.get("file_size_bytes", 0) < TINY_FILE_THRESHOLD_BYTES


def is_zero_page(entry: dict) -> bool:
    """A document is zero-page if page_count_template == 0 AND it's an XFA document.
    For non-XFA the field may simply not be populated."""
    if entry.get("xfa_type") in ("static", "dynamic"):
        return entry.get("page_count_template", 0) == 0
    return False


def is_non_xfa(entry: dict) -> bool:
    return entry.get("xfa_type", "none") == "none"


def is_without_reference(entry: dict) -> bool:
    return not entry.get("pdfrest_ref_available", False)


def classify_entry(entry: dict, mode: str) -> tuple[bool, str | None]:
    """Return (excluded: bool, reason: str | None)."""
    if mode == "all":
        return False, None

    # Adversarial
    if is_adversarial(entry):
        return True, "adversarial_filename"

    # Encrypted
    if is_encrypted(entry):
        return True, "encrypted"

    # Tiny file
    if is_tiny(entry):
        return True, f"file_too_small ({entry.get('file_size_bytes', 0)} bytes < {TINY_FILE_THRESHOLD_BYTES})"

    # Zero-page XFA
    if is_zero_page(entry):
        return True, "zero_page_xfa"

    # Oracle-faulty documents (parse errors prevent useful comparison)
    parse_error = entry.get("parse_error")
    if parse_error and mode != "all":
        return True, f"parse_error: {str(parse_error)[:120]}"

    if mode == "non-adversarial":
        return False, None

    # xfa-only and xfa-with-reference: additionally exclude non-XFA
    if is_non_xfa(entry):
        return True, "non_xfa"

    if mode == "xfa-only":
        return False, None

    # xfa-with-reference: also require pdfRest reference
    if is_without_reference(entry):
        return True, "no_pdfrest_reference"

    return False, None


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main() -> None:
    parser = argparse.ArgumentParser(
        description="Filter a corpus inventory according to exclusion criteria.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    parser.add_argument(
        "--inventory", required=True,
        help="Path to corpus_inventory.json",
    )
    parser.add_argument(
        "--mode", required=True, choices=MODES,
        help=(
            "Exclusion mode: "
            "'all' (no exclusions), "
            "'non-adversarial' (exclude adversarial/encrypted/tiny), "
            "'xfa-only' (non-adversarial + exclude non-XFA), "
            "'xfa-with-reference' (xfa-only + require pdfRest reference)"
        ),
    )
    parser.add_argument(
        "--output", required=True,
        help="Output path for filtered inventory JSON",
    )
    args = parser.parse_args()

    inv_path = Path(args.inventory)
    if not inv_path.exists():
        sys.exit(f"ERROR: inventory not found: {inv_path}")

    try:
        inventory: list[dict] = json.loads(inv_path.read_text())
    except Exception as e:
        sys.exit(f"ERROR: could not parse inventory: {e}")

    total = len(inventory)
    print(f"Input:  {total} documents  mode={args.mode}")

    # Classify each entry
    reason_counts: dict[str, int] = {}
    output_entries: list[dict] = []

    for entry in inventory:
        excluded, reason = classify_entry(entry, args.mode)
        enriched = dict(entry)
        enriched["excluded"] = excluded
        enriched["exclusion_reason"] = reason
        output_entries.append(enriched)

        if excluded and reason:
            # Bucket reason for summary (strip dynamic part)
            bucket = reason.split("(")[0].strip().split(":")[0].strip()
            reason_counts[bucket] = reason_counts.get(bucket, 0) + 1

    excluded_total = sum(1 for e in output_entries if e["excluded"])
    remaining = total - excluded_total

    # Write output
    out_path = Path(args.output)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(output_entries, indent=2))

    # Print summary
    print(f"\nResults:")
    print(f"  Total files:    {total}")
    print(f"  Excluded:       {excluded_total}")
    if reason_counts:
        print(f"  Exclusion reasons:")
        for reason, count in sorted(reason_counts.items(), key=lambda x: -x[1]):
            print(f"    {reason:<40} {count}")
    print(f"  Remaining:      {remaining}")
    print(f"\nWrote filtered inventory to: {out_path}")


if __name__ == "__main__":
    main()
