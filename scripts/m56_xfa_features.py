#!/usr/bin/env python3
"""GL-CA-02: XFA feature extraction and category assignment for M#56."""

from __future__ import annotations

import json
import re
import xml.etree.ElementTree as ET
from pathlib import Path
from typing import Optional

import pypdf

from m56_common import (
    BENCHMARKS_DIR,
    TMP_CONTEXT_PATH,
    bool_str,
    minor_deviation_docs,
    parse_args,
    read_csv_rows,
    resolve_source_pdf,
    write_csv,
)


RAW_PATH = BENCHMARKS_DIR / "m56_pixel_energy_raw.csv"
OUTPUT_PATH = BENCHMARKS_DIR / "m56_category_assignments.csv"

FEATURE_CHECKBOX_RE = re.compile(r'fieldType="Check"|<exclGroup')
FEATURE_POSITIONED_TEXT_RE = re.compile(r'layout="positioned"[^>]*w="\d')
DRAW_TEXT_RE = re.compile(r"<draw[^>]*>.*?<value>.*?<text", re.DOTALL)
FEATURE_TABLE_RE = re.compile(r'layout="table"|layout="row"|layout="col"')
FEATURE_BORDER_RE = re.compile(r'<line\b|presence="visible"|<rectangle\b')

MEASURE_RE = re.compile(r"^\s*([+-]?\d+(?:\.\d+)?)\s*(pt|in|mm|cm|px)?\s*$", re.I)
UNITS = {
    "pt": 1.0,
    "in": 72.0,
    "mm": 72.0 / 25.4,
    "cm": 72.0 / 2.54,
    "px": 0.75,
    None: 1.0,
}


def local_name(tag: str) -> str:
    return tag.rsplit("}", 1)[-1]


def parse_measure(value: Optional[str]) -> Optional[float]:
    if not value:
        return None
    match = MEASURE_RE.match(value)
    if not match:
        return None
    number = float(match.group(1))
    unit = match.group(2).lower() if match.group(2) else None
    return number * UNITS.get(unit, 1.0)


def extract_xfa_xml(pdf_path: Path) -> str:
    reader = pypdf.PdfReader(str(pdf_path))
    root = reader.trailer["/Root"]
    acro_form = root.get("/AcroForm")
    if acro_form is None:
        return ""

    xfa = acro_form.get("/XFA")
    if xfa is None:
        return ""

    if hasattr(xfa, "get_data"):
        data = xfa.get_data()
        return data.decode("utf-8", errors="ignore")

    parts: list[str] = []
    for item in xfa:
        obj = item.get_object() if hasattr(item, "get_object") else item
        if hasattr(obj, "get_data"):
            data = obj.get_data()
            parts.append(data.decode("utf-8", errors="ignore"))
    return "".join(parts)


def xfa_features(xfa_xml: str) -> dict[str, bool]:
    return {
        "has_checkbox": FEATURE_CHECKBOX_RE.search(xfa_xml) is not None,
        "has_draw_text_positioned": bool(
            FEATURE_POSITIONED_TEXT_RE.search(xfa_xml) and DRAW_TEXT_RE.search(xfa_xml)
        ),
        "has_table": FEATURE_TABLE_RE.search(xfa_xml) is not None,
        "has_border_lines": FEATURE_BORDER_RE.search(xfa_xml) is not None,
    }


def has_draw_text(elem: ET.Element) -> bool:
    for child in elem.iter():
        if local_name(child.tag) == "text":
            return True
    return False


def classify_element(elem: ET.Element, positioned_context: bool) -> Optional[str]:
    name = local_name(elem.tag)
    field_type = elem.attrib.get("fieldType", "")
    layout = elem.attrib.get("layout", "")

    if name == "exclGroup":
        return "checkbox"
    if name == "field" and field_type == "Check":
        return "checkbox"
    if name == "draw" and positioned_context and has_draw_text(elem):
        return "text"
    if name in {"line", "rectangle"}:
        return "border"
    if name == "subform" and layout in {"table", "row", "col"}:
        return "border"
    if name == "subform":
        return "background"
    return None


def extract_page_dimensions(root: ET.Element) -> tuple[float, float]:
    for elem in root.iter():
        if local_name(elem.tag) == "pageArea":
            width = parse_measure(elem.attrib.get("w"))
            height = parse_measure(elem.attrib.get("h"))
            if width and height:
                return (width, height)
    return (612.0, 792.0)


def overlaps(a: tuple[float, float, float, float], b: tuple[float, float, float, float]) -> bool:
    ax0, ay0, aw, ah = a
    bx0, by0, bw, bh = b
    ax1, ay1 = ax0 + aw, ay0 + ah
    bx1, by1 = bx0 + bw, by0 + bh
    return ax0 < bx1 and ax1 > bx0 and ay0 < by1 and ay1 > by0


def tile_bbox(tile_r: int, tile_c: int) -> tuple[float, float, float, float]:
    return (tile_c / 10.0, tile_r / 10.0, 0.1, 0.1)


def classify_tile(root: ET.Element, tile_r: int, tile_c: int) -> str:
    page_w, page_h = extract_page_dimensions(root)
    target = tile_bbox(tile_r, tile_c)
    found_types: list[str] = []
    container_count = 0
    coord_nodes = 0

    def walk(elem: ET.Element, base_x: float, base_y: float, positioned: bool) -> None:
        nonlocal container_count, coord_nodes

        name = local_name(elem.tag)
        layout = elem.attrib.get("layout", "")
        elem_x = parse_measure(elem.attrib.get("x")) or 0.0
        elem_y = parse_measure(elem.attrib.get("y")) or 0.0
        elem_w = parse_measure(elem.attrib.get("w"))
        elem_h = parse_measure(elem.attrib.get("h"))

        abs_x = base_x + elem_x
        abs_y = base_y + elem_y
        child_positioned = positioned or (name == "subform" and layout == "positioned" and elem_w is not None)

        if elem_w is not None and elem_h is not None:
            coord_nodes += 1
            bbox = (abs_x / page_w, abs_y / page_h, elem_w / page_w, elem_h / page_h)
            if overlaps(target, bbox):
                kind = classify_element(elem, child_positioned)
                if kind == "background":
                    container_count += 1
                elif kind:
                    found_types.append(kind)

        for child in list(elem):
            walk(child, abs_x, abs_y, child_positioned)

    walk(root, 0.0, 0.0, False)

    if coord_nodes == 0:
        return "mixed"
    unique = set(found_types)
    if unique:
        if len(unique) == 1:
            return next(iter(unique))
        return "mixed"
    if container_count > 0:
        return "background"
    return "mixed"


def resolve_mixed_category(
    rank2_type: str,
    rank3_type: str,
    features: dict[str, bool],
) -> tuple[str, str, str]:
    if rank2_type != rank3_type or rank2_type == "mixed":
        return (
            "mixed",
            "CAT-X",
            "Decision rule 6 triggered: rank-1 tile was mixed and rank-2/rank-3 did not yield a majority signal.",
        )

    dominant = rank2_type
    if dominant == "checkbox" and features["has_checkbox"]:
        return (
            dominant,
            "CAT-6",
            "Decision rule 6 triggered: rank-1 tile was mixed and rank-2/rank-3 resolved the dominant signal to checkbox.",
        )
    if dominant == "text" and features["has_draw_text_positioned"]:
        return (
            dominant,
            "CAT-3",
            "Decision rule 6 triggered: rank-1 tile was mixed and rank-2/rank-3 resolved the dominant signal to text.",
        )
    if dominant == "border" and (features["has_table"] or features["has_border_lines"]):
        return (
            dominant,
            "CAT-7",
            "Decision rule 6 triggered: rank-1 tile was mixed and rank-2/rank-3 resolved the dominant signal to border.",
        )
    if dominant == "text" and not features["has_draw_text_positioned"]:
        return (
            dominant,
            "CAT-8",
            "Decision rule 6 triggered: rank-1 tile was mixed and rank-2/rank-3 resolved the dominant signal to text without positioned draw support.",
        )
    if dominant == "background":
        return (
            dominant,
            "CAT-9",
            "Decision rule 6 triggered: rank-1 tile was mixed and rank-2/rank-3 resolved the dominant signal to background.",
        )
    return (
        dominant,
        "CAT-X",
        "Decision rule 6 triggered: rank-1 tile was mixed but the resolved majority type lacked the required supporting XFA features.",
    )


def decision_tree(
    rank1_type: str,
    rank2_type: str,
    rank3_type: str,
    features: dict[str, bool],
    top3_combined: float,
) -> dict[str, str]:
    confidence = "low"
    dominant = rank1_type

    if rank1_type == "checkbox" and features["has_checkbox"]:
        category = "CAT-6"
        confidence = "high"
        rationale = "Decision rule 1 triggered: dominant tile contains checkbox content and has_checkbox is true."
        fired_step = "1"
    elif rank1_type == "text" and features["has_draw_text_positioned"]:
        category = "CAT-3"
        confidence = "high"
        rationale = "Decision rule 2 triggered: dominant tile contains positioned draw text and has_draw_text_positioned is true."
        fired_step = "2"
    elif rank1_type == "border" and (features["has_table"] or features["has_border_lines"]):
        category = "CAT-7"
        confidence = "high"
        rationale = "Decision rule 3 triggered: dominant tile contains border content and has_table or has_border_lines is true."
        fired_step = "3"
    elif rank1_type == "text" and not features["has_draw_text_positioned"]:
        category = "CAT-8"
        confidence = "medium"
        rationale = "Decision rule 4 triggered: dominant tile is text but has_draw_text_positioned is false."
        fired_step = "4"
    elif rank1_type == "background":
        category = "CAT-9"
        confidence = "medium"
        rationale = "Decision rule 5 triggered: dominant tile classification resolved to background."
        fired_step = "5"
    elif rank1_type == "mixed":
        dominant, category, rationale = resolve_mixed_category(rank2_type, rank3_type, features)
        confidence = {
            "CAT-6": "high",
            "CAT-3": "high",
            "CAT-7": "high",
            "CAT-8": "medium",
            "CAT-9": "medium",
            "CAT-X": "low",
        }[category]
        fired_step = "6"
    else:
        category = "CAT-X"
        confidence = "low"
        rationale = "Decision rule 7 triggered: dominant tile type did not match any category rule."
        fired_step = "7"

    if top3_combined < 8.0:
        confidence = "low"

    return {
        "dominant_tile_type": dominant,
        "primary_category": category,
        "confidence": confidence,
        "fired_step": fired_step,
        "category_rationale": rationale,
    }


def grouped_raw_rows() -> dict[str, list[dict[str, str]]]:
    rows = read_csv_rows(RAW_PATH)
    grouped: dict[str, list[dict[str, str]]] = {}
    for row in rows:
        grouped.setdefault(row["doc_name"], []).append(row)
    for doc_rows in grouped.values():
        doc_rows.sort(key=lambda row: int(row["rank"]))
    return grouped


def main() -> None:
    parse_args("Compute M56 XFA features and category assignments.")
    raw_rows = grouped_raw_rows()
    baseline_docs = {row["doc_name"]: row["ssim"] for row in minor_deviation_docs()}
    output_rows = []
    context: dict[str, dict] = {}

    for doc_name in sorted(baseline_docs):
        doc_rows = raw_rows.get(doc_name)
        if not doc_rows or len(doc_rows) != 3:
            raise RuntimeError(f"Expected exactly 3 raw rows for {doc_name}")

        source_pdf = resolve_source_pdf(doc_name)
        xfa_xml = extract_xfa_xml(source_pdf)
        features = xfa_features(xfa_xml)

        try:
            root = ET.fromstring(xfa_xml)
        except ET.ParseError:
            root = ET.fromstring("<empty/>")

        rank_types = []
        for row in doc_rows:
            rank_types.append(
                classify_tile(root, int(row["tile_r"]), int(row["tile_c"]))
            )

        top3_combined = sum(float(row["energy_pct"]) for row in doc_rows)
        assignment = decision_tree(
            rank_types[0],
            rank_types[1],
            rank_types[2],
            features,
            top3_combined,
        )

        output_rows.append(
            {
                "doc_name": doc_name,
                "ssim": f"{baseline_docs[doc_name]:.6f}",
                "has_checkbox": bool_str(features["has_checkbox"]),
                "has_draw_text_positioned": bool_str(features["has_draw_text_positioned"]),
                "has_table": bool_str(features["has_table"]),
                "has_border_lines": bool_str(features["has_border_lines"]),
                "dominant_tile_type": assignment["dominant_tile_type"],
                "primary_category": assignment["primary_category"],
                "confidence": assignment["confidence"],
            }
        )

        context[doc_name] = {
            "source_pdf": str(source_pdf),
            "rank_tile_types": rank_types,
            "top3_combined_pct": round(top3_combined, 4),
            "category_rationale": assignment["category_rationale"],
            "fired_step": assignment["fired_step"],
            "features": features,
        }

    if len(output_rows) != 64:
        raise RuntimeError(f"Expected 64 category rows, found {len(output_rows)}")

    write_csv(
        OUTPUT_PATH,
        [
            "doc_name",
            "ssim",
            "has_checkbox",
            "has_draw_text_positioned",
            "has_table",
            "has_border_lines",
            "dominant_tile_type",
            "primary_category",
            "confidence",
        ],
        output_rows,
    )
    TMP_CONTEXT_PATH.write_text(json.dumps(context, indent=2))
    print(json.dumps({"output": str(OUTPUT_PATH), "rows": len(output_rows), "context": str(TMP_CONTEXT_PATH)}))


if __name__ == "__main__":
    main()
