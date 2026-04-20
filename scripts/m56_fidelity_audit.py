#!/usr/bin/env python3
from __future__ import annotations

import argparse
import csv
import json
import math
import re
import subprocess
import sys
import xml.etree.ElementTree as ET
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path

try:
    from PIL import Image, ImageStat
except ImportError as exc:  # pragma: no cover - runtime dependency
    raise SystemExit(f"Pillow is required: {exc}")

try:
    from pypdf import PdfReader
except ImportError as exc:  # pragma: no cover - runtime dependency
    raise SystemExit(f"pypdf is required: {exc}")


DPI = 150.0
BRIGHTNESS_THRESHOLD = 200.0
TIER_REQUIREMENTS = {1: 4, 2: 2, 3: 1}
FIELD_HEADERS = [
    "doc_name",
    "tier",
    "field_name",
    "field_type",
    "xfa_value",
    "output_state",
    "oracle_state",
    "classification",
]


@dataclass
class TierDoc:
    doc_name: str
    ssim: float
    tier: int


@dataclass
class BBox:
    x: float
    y: float
    w: float
    h: float
    page_width: float
    page_height: float
    source: str
    origin: str  # "bottom" for annots/tree, "top" for template


@dataclass
class FieldCandidate:
    doc_name: str
    base_name: str
    occ_idx: int
    field_name: str
    field_type: str
    xfa_value_raw: str
    bbox: BBox | None
    bbox_available: bool
    page1_visible: bool


def local_name(tag: str) -> str:
    return tag.split("}", 1)[1] if "}" in tag else tag


def resolve(obj):
    return obj.get_object() if hasattr(obj, "get_object") else obj


def parse_mm_or_pt(value: str | None) -> float | None:
    if value is None:
        return None
    raw = value.strip()
    if not raw:
        return None
    if raw.endswith("mm"):
        return float(raw[:-2]) * 72.0 / 25.4
    if raw.endswith("pt"):
        return float(raw[:-2])
    if raw.endswith("in"):
        return float(raw[:-2]) * 72.0
    if raw.endswith("cm"):
        return float(raw[:-2]) * 72.0 / 2.54
    try:
        return float(raw)
    except ValueError:
        return None


def parse_instance_name(name: str | None, fallback_idx: int) -> tuple[str | None, int]:
    if not name:
        return None, fallback_idx
    match = re.match(r"^(.*)\[(\d+)\]$", name)
    if match:
        return match.group(1), int(match.group(2))
    return name, fallback_idx


def xfa_value_kind(raw_value: str, field_type: str) -> str:
    value = raw_value.strip()
    lowered = value.casefold()
    if lowered in {"", "0", "off", "/off", "false"}:
        return "off"
    if field_type in {"checkbox", "radio"} and lowered in {"1", "/1", "/yes", "/on", "yes", "on", "true"}:
        return "on"
    return "value"


def display_xfa_value(raw_value: str, field_type: str) -> str:
    raw = raw_value.strip()
    if raw:
        return raw
    return "empty/off" if xfa_value_kind(raw_value, field_type) == "off" else "unavailable"


def normalize_text(text: str) -> str:
    lowered = text.casefold().strip()
    lowered = re.sub(r"\s+", " ", lowered)
    return lowered


def compact_text(text: str) -> str:
    return re.sub(r"[^0-9a-z]+", "", normalize_text(text))


def text_matches(expected: str, actual: str) -> bool:
    return bool(expected and actual and compact_text(expected) == compact_text(actual))


def tier_for_ssim(ssim: float) -> int:
    if ssim <= 0.910:
        return 1
    if ssim <= 0.940:
        return 2
    return 3


def load_minor_docs(baseline_json: Path) -> list[TierDoc]:
    payload = json.loads(baseline_json.read_text())
    rows = [
        TierDoc(doc_name=row["file"], ssim=float(row["ssim"]), tier=tier_for_ssim(float(row["ssim"])))
        for row in payload["results"]
        if row.get("status") == "minor_deviation"
    ]
    rows.sort(key=lambda row: (row.tier, row.ssim, row.doc_name))
    return rows


def write_tiers_csv(rows: list[TierDoc], tiers_csv: Path) -> None:
    tiers_csv.parent.mkdir(parents=True, exist_ok=True)
    with tiers_csv.open("w", newline="") as handle:
        writer = csv.writer(handle)
        writer.writerow(["doc_name", "ssim", "tier"])
        for row in rows:
            writer.writerow([row.doc_name, f"{row.ssim:.6f}", row.tier])


def extract_xfa_packets(reader: PdfReader) -> dict[str, bytes]:
    root = resolve(reader.trailer["/Root"])
    acroform = resolve(root.get("/AcroForm"))
    if not acroform:
        return {}
    xfa = resolve(acroform.get("/XFA"))
    if not xfa:
        return {}

    packets: dict[str, bytes] = {}
    if isinstance(xfa, list):
        for i in range(0, len(xfa), 2):
            if i + 1 >= len(xfa):
                break
            packet_name = str(xfa[i])
            packet_obj = resolve(xfa[i + 1])
            if hasattr(packet_obj, "get_data"):
                try:
                    packets[packet_name] = packet_obj.get_data()
                except Exception:
                    continue
    elif hasattr(xfa, "get_data"):
        try:
            packets["xfa"] = xfa.get_data()
        except Exception:
            return {}
    return packets


def find_xml_fragment(blob: bytes, tag_name: str) -> bytes | None:
    text = blob.decode("utf-8", errors="ignore")
    start = text.find(f"<{tag_name}")
    end_tag = f"</{tag_name}>"
    if start == -1:
        start = text.find(f"<xfa:{tag_name}")
        end_tag = f"</xfa:{tag_name}>"
    if start == -1:
        return None
    end = text.rfind(end_tag)
    if end == -1:
        return text[start:].encode("utf-8")
    end += len(end_tag)
    return text[start:end].encode("utf-8")


def parse_xml_root(raw_xml: bytes | None) -> ET.Element | None:
    if not raw_xml:
        return None
    try:
        text = raw_xml.decode("utf-8", errors="ignore")
        start = text.find("<")
        if start > 0:
            text = text[start:]
        return ET.fromstring(text)
    except ET.ParseError:
        return None


def get_template_and_datasets(reader: PdfReader) -> tuple[ET.Element | None, ET.Element | None]:
    packets = extract_xfa_packets(reader)
    template_blob = None
    datasets_blob = None
    for name, data in packets.items():
        lowered = name.casefold()
        if lowered == "template" or lowered.endswith("template"):
            template_blob = data
        elif lowered == "datasets" or lowered.endswith("datasets"):
            datasets_blob = data

    if template_blob is None or datasets_blob is None:
        joined = b"\n".join(packets.values())
        if template_blob is None:
            template_blob = find_xml_fragment(joined, "template")
        if datasets_blob is None:
            datasets_blob = find_xml_fragment(joined, "datasets")

    return parse_xml_root(template_blob), parse_xml_root(datasets_blob)


def parse_dataset_values(root: ET.Element | None) -> dict[str, list[str]]:
    values: dict[str, list[str]] = defaultdict(list)
    if root is None:
        return values

    def walk(node: ET.Element) -> None:
        children = [child for child in node if isinstance(child.tag, str)]
        if not children:
            name = local_name(node.tag)
            if name not in {"datasets", "data"}:
                values[name].append((node.text or "").strip())
            return
        for child in children:
            walk(child)

    walk(root)
    return values


def extract_field_inline_value(node: ET.Element) -> str:
    for child in node:
        if local_name(child.tag) != "value":
            continue
        for descendant in child.iter():
            tag = local_name(descendant.tag)
            if tag in {"text", "integer", "decimal", "boolean", "date", "time"}:
                return (descendant.text or "").strip()
    return ""


def detect_field_type(node: ET.Element) -> str:
    if local_name(node.tag) == "exclGroup":
        return "radio"
    for descendant in node.iter():
        tag = local_name(descendant.tag)
        if tag == "checkButton":
            return "checkbox"
        if tag == "choiceList":
            return "dropdown"
        if tag in {"textEdit", "numericEdit", "dateTimeEdit"}:
            return "text"
    return "unknown"


def parse_template_fields(root: ET.Element | None) -> list[FieldCandidate]:
    if root is None:
        return []

    counts: dict[str, int] = defaultdict(int)
    fields: list[FieldCandidate] = []

    for node in root.iter():
        tag = local_name(node.tag)
        if tag not in {"field", "exclGroup"}:
            continue
        name = node.attrib.get("name")
        if not name:
            continue
        occ_idx = counts[name]
        counts[name] += 1
        field_type = detect_field_type(node)
        fields.append(
            FieldCandidate(
                doc_name="",
                base_name=name,
                occ_idx=occ_idx,
                field_name=f"{name}[{occ_idx}]",
                field_type=field_type,
                xfa_value_raw=extract_field_inline_value(node),
                bbox=None,
                bbox_available=False,
                page1_visible=False,
            )
        )
    return fields


def get_page1_annotation_boxes(reader: PdfReader) -> dict[tuple[str, int], BBox]:
    page = reader.pages[0]
    page_height = float(page.mediabox.height)
    page_width = float(page.mediabox.width)
    annots = resolve(page.get("/Annots")) or []

    boxes: dict[tuple[str, int], BBox] = {}
    auto_counts: dict[str, int] = defaultdict(int)
    for annot_ref in annots:
        annot = resolve(annot_ref)
        if str(annot.get("/Subtype")) != "/Widget":
            continue
        parent = resolve(annot.get("/Parent"))
        raw_name = annot.get("/T") or (parent.get("/T") if parent else None)
        base_name, explicit_idx = parse_instance_name(str(raw_name) if raw_name is not None else None, auto_counts[""])
        if base_name is None:
            continue
        if explicit_idx is None:
            explicit_idx = auto_counts[base_name]
        auto_counts[base_name] = max(auto_counts[base_name], explicit_idx + 1)

        rect = annot.get("/Rect")
        if not rect or len(rect) != 4:
            continue
        x1, y1, x2, y2 = [float(value) for value in rect]
        boxes[(base_name, explicit_idx)] = BBox(
            x=x1,
            y=y1,
            w=max(0.0, x2 - x1),
            h=max(0.0, y2 - y1),
            page_width=page_width,
            page_height=page_height,
            source="annot",
            origin="bottom",
        )
    return boxes


def get_page1_tree_boxes(binary: Path, pdf_path: Path) -> dict[tuple[str, int], BBox]:
    proc = subprocess.run(
        [str(binary), "debug-xfa", "--format", "json", str(pdf_path)],
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        return {}

    try:
        payload = json.loads(proc.stdout)
    except json.JSONDecodeError:
        return {}

    pages = payload.get("pages") or []
    if not pages:
        return {}
    page = pages[0]

    boxes: dict[tuple[str, int], BBox] = {}
    counts: dict[str, int] = defaultdict(int)

    def walk(node: dict) -> None:
        if node.get("type") == "Widget":
            name = node.get("fieldName")
            if name:
                idx = counts[name]
                counts[name] += 1
                boxes[(name, idx)] = BBox(
                    x=float(node["x"]),
                    y=float(node["y"]),
                    w=float(node["width"]),
                    h=float(node["height"]),
                    page_width=float(page["width"]),
                    page_height=float(page["height"]),
                    source="tree",
                    origin="bottom",
                )
        for child in node.get("children") or []:
            walk(child)

    for child in page.get("children") or []:
        walk(child)
    return boxes


def enrich_fields(
    doc_name: str,
    template_fields: list[FieldCandidate],
    dataset_values: dict[str, list[str]],
    annot_boxes: dict[tuple[str, int], BBox],
    tree_boxes: dict[tuple[str, int], BBox],
) -> list[FieldCandidate]:
    enriched: list[FieldCandidate] = []
    for field in template_fields:
        xfa_value = field.xfa_value_raw
        dataset_list = dataset_values.get(field.base_name) or []
        if field.occ_idx < len(dataset_list):
            xfa_value = dataset_list[field.occ_idx]

        bbox = annot_boxes.get((field.base_name, field.occ_idx))
        if bbox is None:
            bbox = tree_boxes.get((field.base_name, field.occ_idx))

        enriched.append(
            FieldCandidate(
                doc_name=doc_name,
                base_name=field.base_name,
                occ_idx=field.occ_idx,
                field_name=field.field_name,
                field_type=field.field_type,
                xfa_value_raw=xfa_value,
                bbox=bbox,
                bbox_available=bbox is not None,
                page1_visible=bbox is not None,
            )
        )
    return enriched


def priority_buckets(fields: list[FieldCandidate], require_bbox: bool) -> list[FieldCandidate]:
    page_filtered = [field for field in fields if field.bbox_available is require_bbox]
    checkbox = [field for field in page_filtered if field.field_type == "checkbox"]
    radio = [field for field in page_filtered if field.field_type == "radio"]
    text_with_value = [
        field
        for field in page_filtered
        if field.field_type == "text" and xfa_value_kind(field.xfa_value_raw, field.field_type) == "value"
    ]
    dropdown = [field for field in page_filtered if field.field_type == "dropdown"]
    others = [
        field
        for field in page_filtered
        if field not in checkbox and field not in radio and field not in text_with_value and field not in dropdown
    ]
    return checkbox + radio + text_with_value + dropdown + others


def select_fields(fields: list[FieldCandidate], tier: int) -> list[FieldCandidate]:
    needed = TIER_REQUIREMENTS[tier]
    chosen: list[FieldCandidate] = []
    seen: set[tuple[str, int]] = set()
    for field in priority_buckets(fields, require_bbox=True) + priority_buckets(fields, require_bbox=False):
        key = (field.base_name, field.occ_idx)
        if key in seen:
            continue
        seen.add(key)
        chosen.append(field)
        if len(chosen) >= needed:
            return chosen
    return chosen


def bbox_to_pixels(bbox: BBox, image_width: int, image_height: int) -> tuple[int, int, int, int] | None:
    scale = DPI / 72.0
    if bbox.origin == "bottom":
        left = bbox.x * scale
        top = (bbox.page_height - bbox.y - bbox.h) * scale
        right = (bbox.x + bbox.w) * scale
        bottom = (bbox.page_height - bbox.y) * scale
    else:
        left = bbox.x * scale
        top = bbox.y * scale
        right = (bbox.x + bbox.w) * scale
        bottom = (bbox.y + bbox.h) * scale

    crop = [math.floor(left), math.floor(top), math.ceil(right), math.ceil(bottom)]
    crop[0] = max(0, min(image_width, crop[0]))
    crop[1] = max(0, min(image_height, crop[1]))
    crop[2] = max(0, min(image_width, crop[2]))
    crop[3] = max(0, min(image_height, crop[3]))
    if crop[2] <= crop[0] or crop[3] <= crop[1]:
        return None
    return crop[0], crop[1], crop[2], crop[3]


def inner_bbox_pixels(field: FieldCandidate, image_width: int, image_height: int) -> tuple[int, int, int, int] | None:
    if field.bbox is None:
        return None
    pixels = bbox_to_pixels(field.bbox, image_width, image_height)
    if pixels is None:
        return None
    left, top, right, bottom = pixels
    width = right - left
    height = bottom - top
    if field.field_type in {"checkbox", "radio"}:
        inset = max(1, int(min(width, height) * 0.24))
    else:
        inset = max(0, int(min(width, height) * 0.04))
    left += inset
    top += inset
    right -= inset
    bottom -= inset
    if right <= left or bottom <= top:
        return pixels
    return left, top, right, bottom


def mean_brightness(image: Image.Image, crop_box: tuple[int, int, int, int]) -> float:
    crop = image.crop(crop_box).convert("L")
    return float(ImageStat.Stat(crop).mean[0])


def render_doc(binary: Path, doc_path: Path, work_dir: Path) -> tuple[Path, Path]:
    stem = doc_path.stem
    flat_pdf = work_dir / f"{stem}.pdf"
    png_prefix = work_dir / stem
    png_path = work_dir / f"{stem}-1.png"

    subprocess.run(
        [str(binary), "flatten", "--output", str(flat_pdf), str(doc_path)],
        check=True,
        capture_output=True,
        text=True,
    )
    subprocess.run(
        ["pdftoppm", "-r", "150", "-png", "-f", "1", "-l", "1", str(flat_pdf), str(png_prefix)],
        check=True,
        capture_output=True,
        text=True,
    )
    return flat_pdf, png_path


def run_ocr(crop_path: Path) -> str:
    proc = subprocess.run(
        ["tesseract", str(crop_path), "stdout", "--psm", "6"],
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        return ""
    return re.sub(r"\s+", " ", proc.stdout).strip()


def classify_field(
    field: FieldCandidate,
    output_state: str,
    oracle_state: str,
    ocr_text: str,
) -> str:
    if not field.bbox_available:
        return "UNKNOWN"

    xfa_kind = xfa_value_kind(field.xfa_value_raw, field.field_type)
    output_on = output_state == "on"
    oracle_on = oracle_state == "on"

    if field.field_type in {"text", "dropdown"} and xfa_kind == "value":
        if ocr_text and text_matches(field.xfa_value_raw, ocr_text):
            return "MATCH"
        if ocr_text and not text_matches(field.xfa_value_raw, ocr_text):
            return "WRONG"
        if not output_on and oracle_on:
            return "MISSING"
        return "UNKNOWN"

    if xfa_kind == "off":
        if not output_on and not oracle_on:
            return "MATCH"
        if output_on and not oracle_on:
            return "INVERTED"
        if output_on and oracle_on:
            return "EXTRA"
        return "UNKNOWN"

    if not output_on and oracle_on:
        return "MISSING"
    if output_on and oracle_on:
        return "MATCH"
    return "UNKNOWN"


def doc_rows(
    binary: Path,
    doc_path: Path,
    oracle_png: Path,
    tier_doc: TierDoc,
    work_dir: Path,
    crop_dir: Path,
) -> list[dict[str, str]]:
    try:
        reader = PdfReader(str(doc_path), strict=False)
    except Exception:
        return placeholder_rows(tier_doc, "source_unreadable")

    template_root, datasets_root = get_template_and_datasets(reader)
    annot_boxes = get_page1_annotation_boxes(reader)
    tree_boxes = get_page1_tree_boxes(binary, doc_path)

    if template_root is None and datasets_root is None:
        return placeholder_rows(tier_doc, "no_xfa_packets")

    template_fields = parse_template_fields(template_root)
    dataset_values = parse_dataset_values(datasets_root)
    fields = enrich_fields(tier_doc.doc_name, template_fields, dataset_values, annot_boxes, tree_boxes)
    selected = select_fields(fields, tier_doc.tier)
    if not selected:
        return placeholder_rows(tier_doc, "no_field_candidates")

    try:
        _, output_png = render_doc(binary, doc_path, work_dir)
        output_img = Image.open(output_png)
        oracle_img = Image.open(oracle_png)
    except Exception:
        return [
            {
                "doc_name": tier_doc.doc_name,
                "tier": str(tier_doc.tier),
                "field_name": field.field_name,
                "field_type": field.field_type,
                "xfa_value": display_xfa_value(field.xfa_value_raw, field.field_type),
                "output_state": "unknown",
                "oracle_state": "unknown",
                "classification": "UNKNOWN",
            }
            for field in selected
        ]

    rows: list[dict[str, str]] = []
    crop_dir.mkdir(parents=True, exist_ok=True)
    for field in selected:
        xfa_display = display_xfa_value(field.xfa_value_raw, field.field_type)
        if not field.bbox_available:
            rows.append(
                {
                    "doc_name": tier_doc.doc_name,
                    "tier": str(tier_doc.tier),
                    "field_name": field.field_name,
                    "field_type": field.field_type,
                    "xfa_value": xfa_display,
                    "output_state": "unknown",
                    "oracle_state": "unknown",
                    "classification": "UNKNOWN",
                }
            )
            continue

        crop_box = inner_bbox_pixels(field, output_img.width, output_img.height)
        if crop_box is None:
            rows.append(
                {
                    "doc_name": tier_doc.doc_name,
                    "tier": str(tier_doc.tier),
                    "field_name": field.field_name,
                    "field_type": field.field_type,
                    "xfa_value": xfa_display,
                    "output_state": "unknown",
                    "oracle_state": "unknown",
                    "classification": "UNKNOWN",
                }
            )
            continue

        output_mean = mean_brightness(output_img, crop_box)
        oracle_mean = mean_brightness(oracle_img, crop_box)
        output_state = "on" if output_mean < BRIGHTNESS_THRESHOLD else "off"
        oracle_state = "on" if oracle_mean < BRIGHTNESS_THRESHOLD else "off"

        ocr_text = ""
        output_state_value = output_state
        if field.field_type in {"text", "dropdown"} and xfa_value_kind(field.xfa_value_raw, field.field_type) == "value":
            crop_path = crop_dir / f"{tier_doc.doc_name}__{field.field_name}.png"
            output_img.crop(crop_box).save(crop_path)
            ocr_text = run_ocr(crop_path)
            if ocr_text:
                output_state_value = ocr_text

        classification = classify_field(field, output_state, oracle_state, ocr_text)
        rows.append(
            {
                "doc_name": tier_doc.doc_name,
                "tier": str(tier_doc.tier),
                "field_name": field.field_name,
                "field_type": field.field_type,
                "xfa_value": xfa_display,
                "output_state": output_state_value,
                "oracle_state": oracle_state,
                "classification": classification,
            }
        )

    while len(rows) < TIER_REQUIREMENTS[tier_doc.tier]:
        rows.append(
            {
                "doc_name": tier_doc.doc_name,
                "tier": str(tier_doc.tier),
                "field_name": f"fallback_unknown[{len(rows)}]",
                "field_type": "unknown",
                "xfa_value": "unavailable",
                "output_state": "unknown",
                "oracle_state": "unknown",
                "classification": "UNKNOWN",
            }
        )
    return rows


def placeholder_rows(tier_doc: TierDoc, reason: str) -> list[dict[str, str]]:
    return [
        {
            "doc_name": tier_doc.doc_name,
            "tier": str(tier_doc.tier),
            "field_name": f"{reason}[{idx}]",
            "field_type": "unknown",
            "xfa_value": "unavailable",
            "output_state": "unknown",
            "oracle_state": "unknown",
            "classification": "UNKNOWN",
        }
        for idx in range(TIER_REQUIREMENTS[tier_doc.tier])
    ]


def write_raw_csv(rows: list[dict[str, str]], raw_csv: Path) -> None:
    raw_csv.parent.mkdir(parents=True, exist_ok=True)
    with raw_csv.open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=FIELD_HEADERS)
        writer.writeheader()
        writer.writerows(rows)


def main() -> int:
    parser = argparse.ArgumentParser(description="M#56 broad data-fidelity audit")
    parser.add_argument("--baseline-json", default="benchmarks/enterprise-baseline-ENT05-final.json")
    parser.add_argument("--tiers-csv", default="benchmarks/m56_fidelity_tiers.csv")
    parser.add_argument("--raw-csv", default="benchmarks/m56_fidelity_raw.csv")
    parser.add_argument("--corpus", required=True)
    parser.add_argument("--oracle", required=True)
    parser.add_argument("--binary", required=True)
    parser.add_argument("--work-dir", default=".tmp/m56_assets/work")
    parser.add_argument("--crop-dir", default=".tmp/m56_assets/crops")
    parser.add_argument("--limit", type=int, default=0)
    args = parser.parse_args()

    baseline_json = Path(args.baseline_json)
    tiers_csv = Path(args.tiers_csv)
    raw_csv = Path(args.raw_csv)
    corpus_dir = Path(args.corpus)
    oracle_dir = Path(args.oracle)
    binary = Path(args.binary)
    work_dir = Path(args.work_dir)
    crop_dir = Path(args.crop_dir)
    work_dir.mkdir(parents=True, exist_ok=True)
    crop_dir.mkdir(parents=True, exist_ok=True)

    tier_docs = load_minor_docs(baseline_json)
    write_tiers_csv(tier_docs, tiers_csv)
    if args.limit:
        tier_docs = tier_docs[: args.limit]

    all_rows: list[dict[str, str]] = []
    for tier_doc in tier_docs:
        doc_path = corpus_dir / tier_doc.doc_name
        oracle_png = oracle_dir / f"{Path(tier_doc.doc_name).stem}.png"
        rows = doc_rows(binary, doc_path, oracle_png, tier_doc, work_dir, crop_dir)
        all_rows.extend(rows)
        print(
            f"{tier_doc.doc_name}: tier={tier_doc.tier} rows={len(rows)} "
            f"unknown={sum(row['classification'] == 'UNKNOWN' for row in rows)}"
        )

    write_raw_csv(all_rows, raw_csv)
    print(f"Wrote {tiers_csv}")
    print(f"Wrote {raw_csv} ({len(all_rows)} rows)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
