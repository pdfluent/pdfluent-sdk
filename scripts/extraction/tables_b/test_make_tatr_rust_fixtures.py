#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The TATR Rust-fixture generator, with the standard library alone.

The crop geometry (10 px at 150 dpi, pixels first), the pure decode replica
against hand-computed softmax/argmax/geometry, the ONNX-against-meta
refusals, the fixture schema round trip and every misshapen record refused,
the table flattening, the decoded-cell fill over a synthetic multi-cell
crop, every committed fixture against the grid its raw outputs imply, the
one cell-mapping helper shared with the eval, and the committed synthetic
fixtures re-decoded through the vendored path on the stand-in Rect. The
torch half (`main`) is not runnable here: only its pre-torch refusal is
taken.
"""
from __future__ import annotations

import base64
import contextlib
import hashlib
import json
import math
import pathlib
import sys
import tempfile
import types

HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(HERE.parent))
sys.path.insert(0, str(HERE.parents[1]))
sys.path.insert(0, str(HERE.parents[1] / "learned"))
import eval_zeroshot as ev  # noqa: E402
import rect_standin  # noqa: E402
import make_tatr_rust_fixtures as gen  # noqa: E402

REPO = HERE.parents[3]
TATR_FIXTURES = REPO / "crates" / "pdf-engine" / "tests" / "fixtures" / "tatr"

FAILURES: list[str] = []


def check(name: str, got, want, why: str = "") -> None:
    if got != want:
        FAILURES.append(f"{name}: got {got!r}, want {want!r}"
                        + (f" -- {why}" if why else ""))


def check_close(name: str, got: float, want: float, tol: float = 1e-9,
                why: str = "") -> None:
    if abs(got - want) > tol:
        FAILURES.append(f"{name}: got {got!r}, want {want!r} (tol {tol})"
                        + (f" -- {why}" if why else ""))


def check_refuses(name: str, fn, *args, **kwargs) -> None:
    try:
        fn(*args, **kwargs)
    except SystemExit:
        return
    FAILURES.append(f"{name}: was accepted")


@contextlib.contextmanager
def with_fake_reference_library():
    """A `fitz`/`pymupdf` on sys.path that raises on import; state restored.

    The post-processing must never import it: any attempt fails the run
    with the fake's own error, while the stand-in path stays silent. The
    fake raises RuntimeError, not ImportError, so an old import-or-fallback
    path cannot swallow it back into green.
    """
    saved_modules = dict(sys.modules)
    saved_path = list(sys.path)
    with tempfile.TemporaryDirectory() as tmp:
        for name in ("fitz", "pymupdf"):
            pathlib.Path(tmp, f"{name}.py").write_text(
                "raise RuntimeError(\"a reference Rect library must never "
                "be imported\")\n", encoding="utf-8")
        sys.path.insert(0, tmp)
        # `rect_standin` stays cached -- it imports nothing itself, so only
        # the post-processing is re-imported, against the cached stand-in.
        for module in ("fitz", "pymupdf", "tatr_postprocess"):
            sys.modules.pop(module, None)
        try:
            yield
        finally:
            sys.path[:] = saved_path
            sys.modules.clear()
            sys.modules.update(saved_modules)


def valid_record(**over) -> dict:
    record = {
        "schema": gen.FIXTURE_SCHEMA,
        "crop": {"document": "d", "page_no": 0, "table": 0, "dpi": 150,
                 "set": "pt-val", "width_px": 100, "height_px": 100,
                 "crop_box_pt": [0.0, 0.0, 48.0, 48.0]},
        "logits": [0.0] * (gen.TATR_QUERIES * gen.TATR_CLASSES),
        "pred_boxes": [0.5] * (gen.TATR_QUERIES * 4),
        "words": [{"text": "a", "bbox": [10.0, 10.0, 20.0, 20.0]}],
        "cells": [{"row": 0, "col": 0, "row_span": 1, "col_span": 1,
                   "header": True, "bbox": [10.0, 10.0, 20.0, 20.0],
                   "text": "a"}],
        "details": {"confidence": 1.0, "detections": 125, "kept": 4,
                    "rect_backend": "stub"},
        "provenance": {"synthetic": True},
    }
    record.update(over)
    return record


def test_crop_geometry() -> None:
    pixels, box = gen.crop_geometry([100.0, 100.0, 300.0, 200.0], 612.0, 792.0)
    check("the 10 px pad at 150 dpi is 4.8 pt a side", pixels,
          (198, 1223, 635, 1452),
          "pixels first: the integer bounds are the geometry")
    check_close("the crop box reads back off the pixels (x0)", box[0], 95.04)
    check_close("the crop box reads back off the pixels (y1)", box[3], 204.96)
    pixels, _box = gen.crop_geometry([0.0, 0.0, 10.0, 10.0], 612.0, 792.0)
    check("the pad clamps to the page", pixels[0], 0)
    check("the pad clamps to the page top", pixels[1] >= 0, True)


def test_decode_outputs() -> None:
    logits = [[2.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0]]
    boxes = [[0.5, 0.5, 0.25, 0.5]]
    [obj] = gen.decode_outputs(logits, boxes, 100.0, 200.0)
    total = math.exp(2.0) + math.exp(1.0) + 5.0
    check("the foreground argmax wins the query", obj["label"], 0)
    check_close("the score is the softmax share", obj["score"],
                math.exp(2.0) / total)
    check_close("centre boxes go to corners, scaled (x0)", obj["bbox"][0],
                37.5)
    check_close("centre boxes go to corners, scaled (y0)", obj["bbox"][1],
                50.0)
    check_close("centre boxes go to corners, scaled (x1)", obj["bbox"][2],
                62.5)
    check_close("centre boxes go to corners, scaled (y1)", obj["bbox"][3],
                150.0)

    # No-object winning overall still decodes to a foreground argmax --
    # transformers takes the max over the first six, whatever the seventh
    # says -- with a tiny score the class thresholds drop later.
    [obj] = gen.decode_outputs([[0.0] * 6 + [10.0]], [boxes[0]], 100.0, 200.0)
    check("a no-object query still takes a foreground label", obj["label"], 0)
    check("with a near-zero score", obj["score"] < 1e-3, True)
    check("which the threshold drops",
          gen.decode_outputs([[0.0] * 6 + [10.0]], [boxes[0]], 100.0, 200.0,
                             threshold=0.5), [])

    # Query order survives: the vendored NMS sorts by score itself, and a
    # decode that reordered would hand it ties in a new order.
    rows = [[float(i)] + [0.0] * 6 for i in (1, 3, 2)]
    got = gen.decode_outputs(rows, [boxes[0]] * 3, 100.0, 200.0)
    scores = [o["score"] for o in got]
    check("detections stay in query order",
          scores[1] > scores[2] > scores[0], True)


def test_check_onnx_against_meta() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        onnx = pathlib.Path(tmp) / "t.onnx"
        onnx.write_bytes(b"the pinned bytes")
        digest = hashlib.sha256(b"the pinned bytes").hexdigest()
        meta = {"model": "tatr-v1.1-all", "onnx_sha256": digest,
                "inputs": [{"name": "pixel_values"}]}
        check("the pinned bytes pass", gen.check_onnx_against_meta(onnx, meta),
              digest)
        check_refuses("a mask input refuses", gen.check_onnx_against_meta,
                      onnx, {"inputs": [{"name": "pixel_values"},
                                        {"name": "pixel_mask"}],
                             "onnx_sha256": digest})
        check_refuses("a missing sha refuses", gen.check_onnx_against_meta,
                      onnx, {"inputs": [{"name": "pixel_values"}]})
        check_refuses("other bytes refuse", gen.check_onnx_against_meta,
                      onnx, {"inputs": [{"name": "pixel_values"}],
                             "onnx_sha256": "0" * 64})


def test_is_pinned_ort() -> None:
    check("1.23.2 passes", gen.is_pinned_ort("1.23.2"), True)
    for other in ("1.23.1", "1.24.0", "1.23.2\n", ""):
        check(f"{other!r} refuses", gen.is_pinned_ort(other), False)


def test_check_processor_normalization() -> None:
    gen.check_processor_normalization(gen.TATR_MEAN, gen.TATR_STD)
    check_refuses("a moved mean refuses",
                  gen.check_processor_normalization,
                  [0.5, 0.5, 0.5], gen.TATR_STD)
    check_refuses("a moved std refuses",
                  gen.check_processor_normalization,
                  gen.TATR_MEAN, [0.25, 0.25, 0.25])
    check_refuses("a short mean refuses",
                  gen.check_processor_normalization,
                  [0.485, 0.456], gen.TATR_STD)


def test_fixture_schema() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        out = pathlib.Path(tmp)
        path = gen.write_fixture(out, "ok", valid_record())
        back = json.loads(path.read_text(encoding="utf-8"))
        check("a valid fixture round-trips", back, valid_record())
        check("without pixels by default", "crop_png_base64" in back, False)
        plain = valid_record()
        e2e = gen.fixture_record(
            plain["schema"], plain["crop"], plain["logits"],
            plain["pred_boxes"], plain["words"], plain["cells"],
            plain["details"], plain["provenance"], b"\x89PNG", b"\x89PNG2")
        gen.write_fixture(out, "e2e", e2e)
        back = json.loads((out / "e2e.json").read_text(encoding="utf-8"))
        check("the e2e pixels ride base64",
              base64.b64decode(back["crop_png_base64"]), b"\x89PNG")
        check("both of them",
              base64.b64decode(back["resized_png_base64"]), b"\x89PNG2")

    bad_schema = valid_record(schema=999)
    check_refuses("an unknown schema refuses", gen.validate_fixture,
                  bad_schema)
    bad_logits = valid_record(logits=[0.0] * 10)
    check_refuses("short logits refuse", gen.validate_fixture, bad_logits)
    bad_boxes = valid_record(pred_boxes=[0.5] * 10)
    check_refuses("short boxes refuse", gen.validate_fixture, bad_boxes)
    bad_word = valid_record(words=[{"text": "a"}])
    check_refuses("a word without a box refuses", gen.validate_fixture,
                  bad_word)
    bad_cell = valid_record(cells=[{"row": 0}])
    check_refuses("a cell without its keys refuses", gen.validate_fixture,
                  bad_cell)
    bad_text = valid_record(cells=[dict(valid_record()["cells"][0],
                                        text=None)])
    check_refuses("a cell without text refuses", gen.validate_fixture,
                  bad_text)
    bad_png = valid_record(crop_png_base64="not base64!!")
    check_refuses("a non-base64 e2e payload refuses", gen.validate_fixture,
                  bad_png)
    with tempfile.TemporaryDirectory() as tmp:
        out = pathlib.Path(tmp)
        try:
            gen.write_fixture(out, "bad", bad_cell)
        except SystemExit:
            pass
        check("a refused fixture writes nothing",
              (out / "bad.json").exists(), False)


def test_provenance_manifest() -> None:
    check("an absolute scratch path records its basename",
          gen.provenance_manifest(
              "/tmp/session/scratchpad/p1/ftn-dev-resolved.json"),
          "ftn-dev-resolved.json")
    check("a relative run path records its basename",
          gen.provenance_manifest("../b-smoke/pt_resolved.json"),
          "pt_resolved.json")
    check("a bare name passes through",
          gen.provenance_manifest("pt_resolved.json"), "pt_resolved.json")
    ok = valid_record(provenance={"manifest": "ftn-dev-resolved.json",
                                  "manifest_sha256": "0" * 64})
    try:
        gen.validate_fixture(ok)
    except SystemExit as err:
        FAILURES.append(f"a bare manifest name was refused: {err}")
    for bad in ("/tmp/x/ftn-dev-resolved.json",
                "/var/tmp/work/pt_resolved.json",
                "/tmp/pt_resolved.json",
                "../b-smoke/pt_resolved.json",
                "~/work/pt_resolved.json",
                "..\\smoke\\pt_resolved.json",
                ""):
        check_refuses(f"a manifest path {bad!r} refuses", gen.validate_fixture,
                      valid_record(provenance={"manifest": bad,
                                               "manifest_sha256": "0" * 64}))
    for name in sorted(path.name for path in TATR_FIXTURES.glob("*.json")):
        record = json.loads((TATR_FIXTURES / name).read_text(
            encoding="utf-8"))
        manifest = (record.get("provenance") or {}).get("manifest")
        if manifest is None:
            continue
        check(f"{name} records no scratch path",
              manifest == gen.provenance_manifest(manifest)
              and "/" not in manifest and "\\" not in manifest, True)
        text = (TATR_FIXTURES / name).read_text(encoding="utf-8")
        check(f"{name} names no home or tmp dir",
              all(needle not in text
                  for needle in ("/private/", "/Users/", "/tmp/", "claude")),
              True)


def test_flatten_tables() -> None:
    pages = [{"tables": [{}, {}]}, {"tables": [{}]}, {"tables": []}]
    check("every table in manifest order",
          gen.flatten_tables(pages), [(0, 0), (0, 1), (1, 0)])


def test_run_onnx() -> None:
    class FakeArray:
        def __init__(self, shape):
            self.shape = shape

        def __getitem__(self, index):
            assert index == 0
            return self

    class FakeSession:
        def __init__(self, shapes):
            self.shapes = shapes

        def run(self, _names, _feed):
            return [FakeArray(shape) for shape in self.shapes]

    fake_np = types.ModuleType("numpy")
    fake_np.asarray = lambda array, dtype=None: array
    fake_np.float64 = object()
    saved = sys.modules.get("numpy")
    sys.modules["numpy"] = fake_np
    try:
        logits, boxes = gen.run_onnx(
            FakeSession([(1, 125, 7), (1, 125, 4)]), object())
        check("the head's shapes pass",
              (logits.shape, boxes.shape), ((1, 125, 7), (1, 125, 4)))
        check_refuses("other shapes refuse", gen.run_onnx,
                      FakeSession([(1, 100, 7), (1, 100, 4)]), object())
    finally:
        if saved is None:
            sys.modules.pop("numpy", None)
        else:
            sys.modules["numpy"] = saved


def test_fill_decoded_cells_multi_cell() -> None:
    """Six cells in, six texts out, each cell's own, in decode order.

    A 2x3 grid tiling a 300x200 crop at 72 dpi, where points are
    pixels: one word per cell, words already in PDF points as
    `words_in_box` leaves them. Red on the dict-comprehension fill,
    which kept one cell -- the last -- and zipped one text onto the
    first decoded cell.
    """
    fill = getattr(gen, "fill_decoded_cells", None)
    if not callable(fill):
        FAILURES.append("gen.fill_decoded_cells is missing: the main "
                        "path fills through nothing testable")
        return
    decoded = [
        {"row": 0, "col": 0, "row_span": 1, "col_span": 1,
         "header": True, "bbox_px": [0.0, 0.0, 100.0, 100.0]},
        {"row": 0, "col": 1, "row_span": 1, "col_span": 1,
         "header": True, "bbox_px": [100.0, 0.0, 200.0, 100.0]},
        {"row": 0, "col": 2, "row_span": 1, "col_span": 1,
         "header": True, "bbox_px": [200.0, 0.0, 300.0, 100.0]},
        {"row": 1, "col": 0, "row_span": 1, "col_span": 1,
         "header": False, "bbox_px": [0.0, 100.0, 100.0, 200.0]},
        {"row": 1, "col": 1, "row_span": 1, "col_span": 1,
         "header": False, "bbox_px": [100.0, 100.0, 200.0, 200.0]},
        {"row": 1, "col": 2, "row_span": 1, "col_span": 1,
         "header": False, "bbox_px": [200.0, 100.0, 300.0, 200.0]},
    ]
    words = [
        {"text": "A", "bbox": [40.0, 140.0, 60.0, 160.0]},
        {"text": "B", "bbox": [140.0, 140.0, 160.0, 160.0]},
        {"text": "C", "bbox": [240.0, 140.0, 260.0, 160.0]},
        {"text": "d", "bbox": [40.0, 40.0, 60.0, 60.0]},
        {"text": "e", "bbox": [140.0, 40.0, 160.0, 60.0]},
        {"text": "f", "bbox": [240.0, 40.0, 260.0, 60.0]},
    ]
    texts = fill(words, decoded, [0.0, 0.0, 300.0, 200.0], 72)
    check("six cells in, six texts out", len(texts), 6)
    check("each cell keeps its own text, in decode order", texts,
          ["A", "B", "C", "d", "e", "f"])
    check("the decode keeps its crop-pixel boxes for the fixture",
          [cell.get("bbox_px") is not None for cell in decoded],
          [True] * 6)
    check("and gains no PDF box beside them",
          ["bbox" in cell for cell in decoded], [False] * 6)


def test_same_mapping_helper_everywhere() -> None:
    """The eval and the generator map through ONE helper.

    `ev.cells_to_pdf` is the only place a `bbox_px` becomes a `bbox`:
    the eval's main loop and the generator's fill both call it, and no
    other pop or direct box call maps a cell beside it. A second
    spelling is how the generator filled one cell where the decode
    held many; this refuses the next one.
    """
    if not callable(getattr(ev, "cells_to_pdf", None)):
        FAILURES.append("ev.cells_to_pdf is missing: the two callers "
                        "cannot share what does not exist")
        return
    eval_src = (REPO / "scripts" / "extraction" / "tables_b"
                / "eval_zeroshot.py").read_text(encoding="utf-8")
    gen_src = pathlib.Path(gen.__file__).read_text(encoding="utf-8")
    check("the helper is defined once, in the eval",
          eval_src.count("def cells_to_pdf("), 1)
    check("the eval maps through it",
          "cells_to_pdf(predicted, crop_box, dpi)" in eval_src, True)
    check("the generator maps through it",
          "ev.cells_to_pdf(" in gen_src, True)
    check("through its fill helper",
          gen_src.count("fill_decoded_cells(") >= 2, True,
          "the def plus the main path's call")
    check("only the helper pops a crop-pixel box in the eval",
          eval_src.count('pop("bbox_px")'), 1)
    check("the generator pops no crop-pixel box beside it",
          gen_src.count('pop("bbox_px")'), 0)
    check("nothing beside the helper maps a box in the eval",
          eval_src.count("crop_px_to_pdf("), 2,
          "the def plus the helper's own call")
    check("the generator calls no other box mapping",
          gen_src.count("ev.crop_px_to_pdf("), 0)


def cells_of(record: dict, postprocess) -> tuple[list[dict], float]:
    """A fixture's logits/boxes/words through the vendored path."""
    import copy

    logits = record["logits"]
    boxes = record["pred_boxes"]
    rows = [logits[i * 7:(i + 1) * 7] for i in range(gen.TATR_QUERIES)]
    corners = [boxes[i * 4:(i + 1) * 4] for i in range(gen.TATR_QUERIES)]
    crop = record["crop"]
    objects = gen.decode_outputs(rows, corners, float(crop["width_px"]),
                                 float(crop["height_px"]))
    bboxes, scores, labels = postprocess.apply_class_thresholds(
        [o["bbox"] for o in objects], [o["label"] for o in objects],
        [o["score"] for o in objects], ev.TATR_LABELS, ev.TATR_THRESHOLDS)
    kept = [{"bbox": box, "label": label, "score": score}
            for box, label, score in zip(bboxes, labels, scores)]
    tokens = [{"bbox": list(word["bbox"]), "text": word["text"], "flags": 0,
               "span_num": at, "line_num": at, "block_num": 0}
              for at, word in enumerate(record["words"])]
    table = {"bbox": [0.0, 0.0, float(crop["width_px"]),
                      float(crop["height_px"])],
             "page_num": 0}
    _structures, cells, confidence = postprocess.objects_to_cells(
        copy.deepcopy(table), copy.deepcopy(kept), copy.deepcopy(tokens),
        ev.TATR_LABELS, ev.TATR_THRESHOLDS)
    decoded = ev.tatr_cells(cells)
    # The fill in the generator's own frame: words and cells through the
    # fixture's crop box at its dpi, the same call the generator makes, so
    # an edge-exact centre cannot flip between the two.
    to_pdf = lambda box: ev.crop_px_to_pdf(  # noqa: E731
        box, list(crop["crop_box_pt"]), int(crop["dpi"]))
    texts = ev.fill_cells(
        [dict(word, bbox=to_pdf(word["bbox"]))
         for word in record["words"]],
        [{"bbox": to_pdf(cell["bbox_px"])} for cell in decoded])
    return ([{"row": cell["row"], "col": cell["col"],
              "row_span": cell["row_span"], "col_span": cell["col_span"],
              "header": cell["header"], "bbox": cell["bbox_px"],
              "text": text}
             for cell, text in zip(decoded, texts)], float(confidence))


def rounded(cells: list[dict]) -> list[dict]:
    return [dict(cell, bbox=[round(v, 9) for v in cell["bbox"]])
            for cell in cells]


def test_synthetic_fixtures() -> None:
    names = sorted(path.name for path in TATR_FIXTURES.glob("syn_*.json"))
    check("synthetic fixtures are committed", len(names) >= 2, True,
          "the parity test needs them before T5's terminal run lands")
    rect_standin.ensure()
    import tatr_postprocess as postprocess

    for name in names:
        record = json.loads((TATR_FIXTURES / name).read_text(
            encoding="utf-8"))
        try:
            gen.validate_fixture(record)
        except SystemExit as err:
            FAILURES.append(f"{name} is misshapen: {err}")
            continue
        got, confidence = cells_of(record, postprocess)
        check(f"{name} re-decodes to its cells", rounded(got),
              rounded(record["cells"]),
              "hand-edited cells would green a lying parity test")
        check_close(f"{name} re-decodes to its confidence", confidence,
                    record["details"]["confidence"], tol=1e-9)
    with with_fake_reference_library():
        try:
            import tatr_postprocess as guarded
        except RuntimeError as err:
            FAILURES.append("the post-processing imported a reference "
                            f"library: {err}")
            return
        check("Rect resolves to the stand-in",
              guarded.Rect is rect_standin.Rect, True)
        for name in names:
            record = json.loads((TATR_FIXTURES / name).read_text(
                encoding="utf-8"))
            got, _confidence = cells_of(record, guarded)
            check(f"{name} decodes with a reference library present",
                  rounded(got), rounded(record["cells"]))
    rect_standin.ensure()
    import tatr_postprocess as postprocess_again  # noqa: F401


def test_committed_fixtures_match_their_grid() -> None:
    """Every committed fixture holds the cells its grid implies.

    Each fixture's own cells tile their grid with no holes -- and a
    single cell is only ever a 1x1 grid -- while the raw outputs
    re-decode through the vendored path to the same count and grid. A
    fixture with one cell where the decode held many fails the
    re-decode leg.
    """
    names = sorted(path.name for path in TATR_FIXTURES.glob("*.json"))
    check("fixtures are committed", len(names) >= 3, True)
    rect_standin.ensure()
    import tatr_postprocess as postprocess

    for name in names:
        record = json.loads((TATR_FIXTURES / name).read_text(
            encoding="utf-8"))
        cells = record.get("cells") or []
        check(f"{name} holds cells", len(cells) > 0, True)
        if not cells:
            continue
        rows = max(c["row"] + c["row_span"] for c in cells)
        cols = max(c["col"] + c["col_span"] for c in cells)
        if len(cells) == 1:
            check(f"{name}: one cell is a 1x1 grid", (rows, cols),
                  (1, 1))
        covered = set()
        for cell in cells:
            for row in range(cell["row"],
                             cell["row"] + cell["row_span"]):
                for col in range(cell["col"],
                                 cell["col"] + cell["col_span"]):
                    covered.add((row, col))
        holes = [(row, col) for row in range(rows)
                 for col in range(cols)
                 if (row, col) not in covered]
        check(f"{name} tiles its grid with no holes", holes, [])
        origins = [(c["row"], c["col"]) for c in cells]
        check(f"{name} holds one cell per origin", len(set(origins)),
              len(cells))
        got, _confidence = cells_of(record, postprocess)
        check(f"{name} re-decodes to as many cells", len(got),
              len(cells))
        if len(got) == len(cells):
            want_grid = sorted((c["row"], c["col"], c["row_span"],
                                c["col_span"], c["header"])
                               for c in cells)
            got_grid = sorted((c["row"], c["col"], c["row_span"],
                               c["col_span"], c["header"])
                              for c in got)
            check(f"{name} re-decodes to its grid", got_grid,
                  want_grid)


def test_main_refuses_before_torch() -> None:
    try:
        gen.main(["--pt-dev", "a", "--ftn-dev", "b", "--pdf-dir", "c",
                  "--feature-dump", "d", "--weights", "e", "--onnx", "f",
                  "--export-meta", "g", "--out", "h", "--per-set", "0"])
    except SystemExit as err:
        check("an empty set refuses before any import",
              "names no crops" in str(err), True)
    else:
        FAILURES.append("an empty set was accepted")
    check("no torch entered for a refusal",
          any(name.split(".")[0] in ("torch", "onnxruntime", "PIL",
                                     "transformers")
              for name in sys.modules), False)


def main() -> int:
    test_crop_geometry()
    test_decode_outputs()
    test_check_onnx_against_meta()
    test_is_pinned_ort()
    test_check_processor_normalization()
    test_fixture_schema()
    test_provenance_manifest()
    test_flatten_tables()
    test_run_onnx()
    test_fill_decoded_cells_multi_cell()
    test_same_mapping_helper_everywhere()
    test_synthetic_fixtures()
    test_committed_fixtures_match_their_grid()
    test_main_refuses_before_torch()
    if FAILURES:
        print(f"[test-make-tatr-rust-fixtures] {len(FAILURES)} check(s) "
              f"failed:", file=sys.stderr)
        for line in FAILURES:
            print(f"  - {line}", file=sys.stderr)
        return 1
    print("OK make_tatr_rust_fixtures: geometry, decode, meta refusals, "
          "schema, provenance basenames, flattening, the head's shapes, "
          "the multi-cell fill, the one mapping helper, synthetic "
          "fixtures re-decoded on the stand-in Rect, every fixture "
          "against its grid, and the pre-torch refusal")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
