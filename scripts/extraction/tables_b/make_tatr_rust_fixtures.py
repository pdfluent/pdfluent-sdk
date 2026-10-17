#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""TATR Rust-parity fixtures: raw ONNX outputs plus the Python-decoded cells.

The P2 bar is parity with Python: the Rust post-processing, fed raw ONNX
outputs, must decode the same cells as this script's reference path on the
same crops. T5 runs this in a terminal (torch + onnxruntime 1.23.2, the P1
venv); the fixtures land as small JSON under
`crates/pdf-engine/tests/fixtures/tatr/`, thirty crops -- fifteen PT-val,
fifteen FTN-clean -- and the Rust parity test reads them with no model and
no dylib. No PDFs, no rig, no ParseBench data travel: each fixture is the
raw outputs, the crop words and the decoded cells.

The reference path per crop is `eval_zeroshot.py`'s TATR path (P1's
revision), called -- not copied: the page renders through
`eval_zeroshot.render_page`, the words come from the same `feature_dump`
batch, the crop is MaxResize(1000)'d in bilinear and tensored by the hub
processor with `do_resize=False` (P1's `TatrAdapter.preprocess`: the hub
config's lone `longest_edge` is rejected when transformers itself resizes),
the ONNX runs the P1 export, the decode is the hub processor's own
`post_process_object_detection`, the structure is the vendored
`tatr_postprocess.objects_to_cells` at the eval's thresholds, and the cells
are `eval_zeroshot.tatr_cells` with `fill_cells` texts. The one deliberate
difference from the eval is the crop margin: the TATR reference pads 10 px
at the run's render resolution, and the Rust pre-processing mirrors that
geometry, so the crops here carry the 10 px pad at 150 dpi rather than the
eval's own 2 pt.

`decode_outputs` is the one replica: a pure-Python reading of
`post_process_object_detection` (transformers 4.57.6,
`models/detr/image_processing_detr.py`, softmax over all seven classes,
argmax over the six foreground ones, centre boxes to corners scaled to the
crop, keep scores strictly above the threshold). The real runs do NOT use
it -- they call the processor itself, so a drift between the replica and
the reference can never hide inside a fixture. It exists so the synthetic
self-check fixtures (and the test) can run the vendored path with the
standard library alone.
"""
from __future__ import annotations

import argparse
import base64
import hashlib
import json
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(HERE.parent))
sys.path.insert(0, str(HERE.parents[1]))
sys.path.insert(0, str(HERE.parents[1] / "learned"))
import eval_zeroshot as ev  # noqa: E402
import rect_standin  # noqa: E402

# The fixture schema this script writes and the Rust parity test reads.
# Bumped when a field is added, removed or re-meaninged -- never silently.
FIXTURE_SCHEMA = 1

# The TATR reference geometry the Rust pre-processing mirrors: the truth box
# plus 10 px at the run's render resolution, at 150 dpi, longest side 1000.
TATR_PAD_PX = 10
TATR_DPI = 150
TATR_MAX_SIZE = 1000

# Queries and classes of the exported head: 125 DETR queries, six foreground
# classes plus the no-object the head adds.
TATR_QUERIES = 125
TATR_CLASSES = 7

# The runtime the export was proven on (P1): anything else refuses.
ORT_VERSION = "1.23.2"

# Crops per set, and how many of the PT crops carry their pixels for the
# Rust end-to-end test: the crop PNG (so the e2e renders nothing through
# another renderer) AND the MaxResize(1000)'d PNG the processor tensored
# (so the e2e's strict 1e-3 compares against the exact pixels the fixture
# logits came from, while the Rust bilinear is held separately to an
# approximate pixel bar against the same pair).
PER_SET = 15
E2E_CROPS = 3

# The hub preprocessor_config's normalisation (v1.1-all, as P1's
# export_meta records): ImageNet mean/std. The processor's own values are
# asserted against these before a fixture is written, so a hub drift
# refuses rather than silently re-meaninging every committed logit.
TATR_MEAN = [0.485, 0.456, 0.406]
TATR_STD = [0.229, 0.224, 0.225]


def crop_geometry(truth_box: list[float], width: float, height: float,
                  dpi: int = TATR_DPI) -> tuple[tuple[int, int, int, int],
                                               list[float]]:
    """A truth box (PDF points, y up) as crop pixels and back.

    The box grows by the TATR 10 px at the run's dpi, clamped to the page;
    then -- pixels first, points after, exactly as the eval -- the integer
    pixel bounds ARE the geometry every conversion goes through, and the
    crop box in points is read back off them.
    """
    pad = TATR_PAD_PX * 72.0 / dpi
    wanted = ev.pad_box(truth_box, pad, width, height)
    scale = dpi / 72.0
    left = int(wanted[0] * scale)
    top = int((height - wanted[3]) * scale)
    right = int(wanted[2] * scale + 0.999)
    bottom = int((height - wanted[1]) * scale + 0.999)
    crop_box = [left / scale, height - bottom / scale,
                right / scale, height - top / scale]
    return (left, top, right, bottom), crop_box


def decode_outputs(logits: list[list[float]], boxes: list[list[float]],
                   width_px: float, height_px: float,
                   threshold: float = 0.0) -> list[dict]:
    """Raw TATR outputs as kept detections, the transformers reading.

    `logits` is one row of seven per query, `boxes` one normalised
    (cx, cy, w, h) row per query. Softmax over all seven classes, then the
    best of the six foreground classes wins each query; boxes go centre to
    corners and scale to the crop; detections scoring strictly above the
    threshold are kept, in query order. With the eval's 0.0 every query is
    kept -- the class thresholds fall later, in the vendored path.
    """
    import math

    objects = []
    for row, box in zip(logits, boxes):
        top = max(row)
        exps = [math.exp(v - top) for v in row]
        total = sum(exps)
        probs = [v / total for v in exps]
        label = max(range(len(probs) - 1), key=probs.__getitem__)
        score = probs[label]
        if score > threshold:
            cx, cy, w, h = box
            objects.append({
                "bbox": [(cx - w / 2.0) * width_px,
                         (cy - h / 2.0) * height_px,
                         (cx + w / 2.0) * width_px,
                         (cy + h / 2.0) * height_px],
                "label": label,
                "score": score,
            })
    return objects


def check_onnx_against_meta(onnx_path: pathlib.Path, meta: dict) -> str:
    """The ONNX file P1's meta vouches for, or a refusal.

    Refuses when the meta is not for the no-mask dynamic export (one input,
    `pixel_values` alone -- the reference passes no mask), when it carries
    no sha, and when the file's bytes hash differently. Returns the digest.
    """
    inputs = meta.get("inputs")
    names = [entry.get("name") for entry in inputs] \
        if isinstance(inputs, list) else None
    if names != ["pixel_values"]:
        raise SystemExit(
            f"REFUSED: {meta.get('model', '?')} meta names inputs {names}, "
            f"want ['pixel_values'] alone -- fixtures come from the "
            f"no-mask dynamic export only")
    want = meta.get("onnx_sha256") or ""
    if len(want) != 64 or any(c not in "0123456789abcdefABCDEF"
                              for c in want):
        raise SystemExit(
            "REFUSED: the export meta carries no onnx_sha256 to check "
            "the file against")
    digest = hashlib.sha256(onnx_path.read_bytes()).hexdigest()
    if digest != want.lower():
        raise SystemExit(
            f"REFUSED: {onnx_path} hashes {digest}, the meta pins "
            f"{want.lower()} -- fixtures come from the pinned bytes only")
    return digest


def is_pinned_ort(version: str) -> bool:
    """The exact runtime the export was proven on, and nothing else."""
    return version == ORT_VERSION


def check_processor_normalization(mean, std) -> None:
    """The processor's mean/std are the hub's, or nothing is written.

    The Rust side normalises with the same constants: a processor that
    drifted would mint logits no Rust input could reproduce, so it
    refuses here, naming both.
    """
    mean = [float(v) for v in mean]
    std = [float(v) for v in std]
    if len(mean) != 3 or len(std) != 3 or \
            any(abs(a - b) > 1e-12 for a, b in zip(mean + std,
                                                  TATR_MEAN + TATR_STD)):
        raise SystemExit(
            f"REFUSED: the processor normalises with mean {mean} std "
            f"{std}, want mean {TATR_MEAN} std {TATR_STD} -- the hub "
            f"config moved, and the Rust constants with it or not at all")


def fixture_record(schema: int, crop: dict, logits: list[float],
                   boxes: list[float], words: list[dict], cells: list[dict],
                   details: dict, provenance: dict,
                   crop_png: bytes | None = None,
                   resized_png: bytes | None = None) -> dict:
    """One fixture as the Rust parity test reads it.

    Boxes and word bboxes stay in crop pixels -- the mapping to points is
    the Rust side's to prove. `crop_png` and `resized_png` ride along only
    on the e2e crops: the crop, and the MaxResize(1000)'d pixels the
    processor tensored.
    """
    record = {
        "schema": schema,
        "crop": crop,
        "logits": logits,
        "pred_boxes": boxes,
        "words": words,
        "cells": cells,
        "details": details,
        "provenance": provenance,
    }
    if crop_png is not None:
        record["crop_png_base64"] = base64.b64encode(crop_png).decode("ascii")
    if resized_png is not None:
        record["resized_png_base64"] = base64.b64encode(
            resized_png).decode("ascii")
    return record


def validate_fixture(record: dict) -> None:
    """A fixture that lies about its shape is refused, not decoded.

    The Rust test holds the same contract: schema it knows, 125x7 logits,
    125x4 boxes, words with text and a box, cells with a grid position, a
    span, a header flag, a box and filled text.
    """
    if record.get("schema") != FIXTURE_SCHEMA:
        raise SystemExit(
            f"REFUSED: fixture schema {record.get('schema')!r}, "
            f"want {FIXTURE_SCHEMA}")
    logits = record.get("logits")
    if not isinstance(logits, list) or \
            len(logits) != TATR_QUERIES * TATR_CLASSES:
        have = len(logits) if isinstance(logits, list) else type(logits)
        raise SystemExit(
            f"REFUSED: fixture logits holds {have} values, want "
            f"{TATR_QUERIES * TATR_CLASSES}")
    boxes = record.get("pred_boxes")
    if not isinstance(boxes, list) or len(boxes) != TATR_QUERIES * 4:
        have = len(boxes) if isinstance(boxes, list) else type(boxes)
        raise SystemExit(
            f"REFUSED: fixture pred_boxes holds {have} values, want "
            f"{TATR_QUERIES * 4}")
    for word in record.get("words") or []:
        if not isinstance(word.get("text"), str) or \
                len(word.get("bbox") or []) != 4:
            raise SystemExit(
                f"REFUSED: a fixture word carries no text and box: {word!r}")
    for cell in record.get("cells") or []:
        for key in ("row", "col", "row_span", "col_span", "header",
                    "bbox", "text"):
            if key not in cell:
                raise SystemExit(
                    f"REFUSED: a fixture cell carries no {key!r}: {cell!r}")
        if len(cell["bbox"]) != 4 or not isinstance(cell["text"], str):
            raise SystemExit(
                f"REFUSED: a fixture cell's box or text is misshapen: "
                f"{cell!r}")
    for key in ("crop_png_base64", "resized_png_base64"):
        if key in record:
            try:
                base64.b64decode(record[key], validate=True)
            except (ValueError, TypeError):
                raise SystemExit(
                    f"REFUSED: fixture {key} is not base64")
    provenance = record.get("provenance") or {}
    manifest = provenance.get("manifest")
    if manifest is not None:
        if not isinstance(manifest, str) or not manifest \
                or manifest != provenance_manifest(manifest) \
                or "\\" in manifest \
                or manifest.startswith("~") or manifest.startswith("."):
            raise SystemExit(
                f"REFUSED: fixture provenance.manifest {manifest!r} is "
                f"not a bare file name -- the generator's scratch path "
                f"must never enter a fixture")


def provenance_manifest(dev_path: str) -> str:
    """The manifest name a fixture may record: the basename, never a path.

    The generator runs in a scratch directory whose absolute path names the
    operator's machine; that path must never enter the repo. The basename
    plus `manifest_sha256` still names the manifest exactly.
    """
    return pathlib.PurePath(dev_path).name


def write_fixture(out_dir: pathlib.Path, name: str, record: dict) -> pathlib.Path:
    """A validated fixture to disk, `<name>.json`."""
    validate_fixture(record)
    path = out_dir / f"{name}.json"
    path.write_text(json.dumps(record, ensure_ascii=False), encoding="utf-8")
    return path


def flatten_tables(pages: list[dict]) -> list[tuple[int, int]]:
    """Every (page entry, table) pair in manifest order."""
    pairs = []
    for page_at, entry in enumerate(pages):
        for table_at in range(len(entry.get("tables") or [])):
            pairs.append((page_at, table_at))
    return pairs


def run_onnx(session, pixel_values):
    """One preprocessed crop through the P1 export: logits, boxes, refused
    unless the shapes are the head's own (1x125x7, 1x125x4)."""
    import numpy as np

    logits, boxes = session.run(None, {"pixel_values": pixel_values})
    logits = np.asarray(logits, dtype=np.float64)
    boxes = np.asarray(boxes, dtype=np.float64)
    if logits.shape != (1, TATR_QUERIES, TATR_CLASSES) or \
            boxes.shape != (1, TATR_QUERIES, 4):
        raise SystemExit(
            f"REFUSED: the ONNX returns {logits.shape} + {boxes.shape}, "
            f"want (1, {TATR_QUERIES}, {TATR_CLASSES}) + "
            f"(1, {TATR_QUERIES}, 4)")
    return logits[0], boxes[0]


def fill_decoded_cells(in_crop: list[dict], decoded: list[dict],
                       crop_box: list[float],
                       dpi: int = TATR_DPI) -> list[str]:
    """Each decoded cell's text, in decode order: N cells in, N out.

    Decoded cells carry `bbox_px` (crop pixels) while the word fill
    reads `bbox` (PDF points), so the cells map through the eval's own
    `cells_to_pdf` -- the one place a crop-pixel box becomes a PDF
    box -- over copies: the fixture keeps `bbox_px` beside the text,
    and the mapping pops the key it reads. `in_crop` words are
    already in PDF points, as `words_in_box` leaves them.
    """
    to_fill = [dict(cell, bbox_px=list(cell["bbox_px"]))
               for cell in decoded]
    return ev.fill_cells(in_crop, ev.cells_to_pdf(to_fill, crop_box, dpi))


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--pt-dev", required=True,
                    help="the resolved PT-val manifest")
    ap.add_argument("--ftn-dev", required=True,
                    help="the resolved FTN-clean manifest")
    ap.add_argument("--pdf-dir", required=True,
                    help="the one-page dev PDFs of both manifests")
    ap.add_argument("--feature-dump", required=True)
    ap.add_argument("--weights", required=True,
                    help="the TATR weights dir: only the processor config "
                         "is read, the torch model never loads")
    ap.add_argument("--onnx", required=True,
                    help="P1's exported ONNX file")
    ap.add_argument("--export-meta", required=True,
                    help="P1's export_meta.json: the sha the ONNX must match")
    ap.add_argument("--mutool", default="mutool")
    ap.add_argument("--per-set", type=int, default=PER_SET)
    ap.add_argument("--out", required=True)
    args = ap.parse_args(argv)

    if args.per_set <= 0:
        raise SystemExit(f"--per-set {args.per_set} names no crops")

    import onnxruntime
    from transformers import AutoImageProcessor

    if not is_pinned_ort(onnxruntime.__version__):
        raise SystemExit(
            f"REFUSED: onnxruntime {onnxruntime.__version__}, want "
            f"{ORT_VERSION} -- the runtime the export was proven on")
    meta = json.loads(pathlib.Path(args.export_meta).read_text(
        encoding="utf-8"))
    onnx_path = pathlib.Path(args.onnx)
    digest = check_onnx_against_meta(onnx_path, meta)
    session = onnxruntime.InferenceSession(
        str(onnx_path), providers=["CPUExecutionProvider"])
    processor = AutoImageProcessor.from_pretrained(args.weights)
    check_processor_normalization(processor.image_mean, processor.image_std)
    backend = rect_standin.ensure()
    import tatr_postprocess as postprocess

    from PIL import Image
    import numpy as np
    import torch

    out = pathlib.Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    pdf_dir = pathlib.Path(args.pdf_dir)
    eval_sha = hashlib.sha256(
        (HERE.parent / "eval_zeroshot.py").read_bytes()).hexdigest()

    sets = (("pt-val", args.pt_dev), ("ftn-clean", args.ftn_dev))
    e2e_left = E2E_CROPS
    for set_name, dev_path in sets:
        manifest = ev.read_resolved(pathlib.Path(dev_path))
        manifest_sha = hashlib.sha256(
            pathlib.Path(dev_path).read_bytes()).hexdigest()
        pdfs = [pdf_dir / entry["pdf"] for entry in manifest["pages"]]
        missing = [str(p) for p in pdfs if not p.is_file()]
        if missing:
            raise SystemExit(f"{len(missing)} manifest PDF(s) are not in "
                             f"{pdf_dir}, first {missing[0]}")
        records = ev.load_words(args.feature_dump, pdfs, 4)
        made = 0
        skipped: list[str] = []
        with __import__("tempfile").TemporaryDirectory() as tmp:
            for page_at, table_at in flatten_tables(manifest["pages"]):
                if made >= args.per_set:
                    break
                entry = manifest["pages"][page_at]
                table = entry["tables"][table_at]
                pdf = pdf_dir / entry["pdf"]
                read_page = ev.read_page_of(entry, pdf, args.mutool)
                record = records[str(pdf)]
                width = float(record["width"])
                height = float(record["height"])
                if abs(height - float(entry["page_height"])) \
                        > ev.HEIGHT_SLACK:
                    skipped.append(f"{entry['document']} p{read_page} "
                                   f"t{table_at}: page height moved")
                    continue
                truth_box = ev.page_frame(
                    [float(v) for v in table["bbox"]], height)
                pixels, crop_box = crop_geometry(truth_box, width, height)
                png = pathlib.Path(tmp) / "page.png"
                ev.render_page(args.mutool, pdf, read_page, TATR_DPI, png)
                image = Image.open(png).convert("RGB")
                crop = image.crop(pixels)
                crop_w, crop_h = crop.size
                in_crop = ev.words_in_box(record.get("words") or [], crop_box)
                # P1's `TatrAdapter.preprocess`, verbatim in effect: the
                # reference's MaxResize at the reference size, in bilinear
                # (`round` halves like theirs), then the processor with
                # `do_resize=False` so its lone-longest_edge config never
                # runs -- it only tensorizes and normalizes.
                resize_scale = TATR_MAX_SIZE / max(crop_w, crop_h)
                resized = crop.resize(
                    (int(round(resize_scale * crop_w)),
                     int(round(resize_scale * crop_h))), Image.BILINEAR)
                if max(resized.size) != TATR_MAX_SIZE:
                    raise SystemExit(
                        f"REFUSED: the crop resized to longest side "
                        f"{max(resized.size)}, want {TATR_MAX_SIZE}")
                values = processor(
                    images=resized, return_tensors="pt",
                    do_resize=False)["pixel_values"].numpy()
                logits, boxes = run_onnx(session, values)
                fake = __import__("types").SimpleNamespace(
                    logits=torch.from_numpy(logits).unsqueeze(0),
                    pred_boxes=torch.from_numpy(boxes).unsqueeze(0))
                [result] = processor.post_process_object_detection(
                    fake, target_sizes=[(crop_h, crop_w)], threshold=0.0)
                objects = [{"bbox": [float(v) for v in box],
                            "label": int(label), "score": float(score)}
                           for box, label, score in zip(
                               result["boxes"].tolist(),
                               result["labels"].tolist(),
                               result["scores"].tolist())]
                scale = TATR_DPI / 72.0
                tokens = [{"bbox": [(w["bbox"][0] - crop_box[0]) * scale,
                                    (crop_box[3] - w["bbox"][3]) * scale,
                                    (w["bbox"][2] - crop_box[0]) * scale,
                                    (crop_box[3] - w["bbox"][1]) * scale],
                           "text": w["text"], "flags": 0, "span_num": at,
                           "line_num": at, "block_num": 0}
                          for at, w in enumerate(in_crop)]
                bboxes, scores, labels = \
                    postprocess.apply_class_thresholds(
                        [o["bbox"] for o in objects],
                        [o["label"] for o in objects],
                        [o["score"] for o in objects],
                        ev.TATR_LABELS, ev.TATR_THRESHOLDS)
                kept = [{"bbox": box, "label": label, "score": score}
                        for box, label, score in zip(bboxes, labels, scores)]
                table_obj = {"bbox": [0.0, 0.0, float(crop_w),
                                      float(crop_h)],
                             "page_num": 0}
                _structures, cells, confidence = \
                    postprocess.objects_to_cells(
                        table_obj, kept, tokens, ev.TATR_LABELS,
                        ev.TATR_THRESHOLDS)
                decoded = ev.tatr_cells(cells)
                texts = fill_decoded_cells(in_crop, decoded, crop_box,
                                           TATR_DPI)
                fixture_cells = [{
                    "row": cell["row"], "col": cell["col"],
                    "row_span": cell["row_span"],
                    "col_span": cell["col_span"],
                    "header": cell["header"], "bbox": cell["bbox_px"],
                    "text": text}
                    for cell, text in zip(decoded, texts)]
                words = [{"text": w["text"],
                          "bbox": [float(v) for v in token["bbox"]]}
                         for w, token in zip(in_crop, tokens)]
                crop_png = None
                resized_png = None
                if set_name == "pt-val" and e2e_left > 0:
                    import io as _io
                    buffer = _io.BytesIO()
                    crop.save(buffer, format="PNG")
                    crop_png = buffer.getvalue()
                    buffer = _io.BytesIO()
                    resized.save(buffer, format="PNG")
                    resized_png = buffer.getvalue()
                    e2e_left -= 1
                name = f"{'pt' if set_name == 'pt-val' else 'ftn'}_{made:02d}"
                write_fixture(out, name, fixture_record(
                    FIXTURE_SCHEMA,
                    {"document": entry["document"],
                     "page_no": entry["page_no"], "table": table_at,
                     "dpi": TATR_DPI, "set": set_name,
                     "width_px": crop_w, "height_px": crop_h,
                     "crop_box_pt": crop_box},
                    [float(v) for v in logits.flatten().tolist()],
                    [float(v) for v in boxes.flatten().tolist()],
                    words, fixture_cells,
                    {"confidence": float(confidence),
                     "detections": len(objects), "kept": len(kept),
                     "rect_backend": backend},
                    {"manifest": provenance_manifest(dev_path),
                     "manifest_sha256": manifest_sha,
                     "eval_zeroshot_sha256": eval_sha,
                     "onnx_sha256": digest,
                     "export_policy": meta.get("policy"),
                     "export_opset": meta.get("opset"),
                     "onnxruntime": onnxruntime.__version__,
                     "rect_backend": backend,
                     "image_mean": list(processor.image_mean),
                     "image_std": list(processor.image_std)},
                    crop_png, resized_png))
                made += 1
        print(f"{set_name}: {made} fixtures"
              + (f", skipped {len(skipped)} ({'; '.join(skipped)})"
                 if skipped else ""))
        if made < args.per_set:
            raise SystemExit(
                f"REFUSED: {set_name} gave {made} crops, want "
                f"{args.per_set} -- the parity set is {args.per_set} a side")
    if e2e_left:
        raise SystemExit(
            f"REFUSED: {E2E_CROPS - e2e_left} of {E2E_CROPS} e2e crops "
            f"carried pixels")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
