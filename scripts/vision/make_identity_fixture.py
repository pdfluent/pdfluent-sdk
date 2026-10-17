#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Build the tiny identity ONNX fixture for the vision-layer tests (#636).

Writes crates/pdf-engine/tests/fixtures/vision_identity.onnx: a single
Identity node, float input [1, 'C', 'H', 'W'], so it runs on any raster
(grayscale or RGB) regardless of page size. A few hundred bytes, far under
the 50 KB budget. Regenerate with:

    python3 scripts/vision/make_identity_fixture.py

Requires the `onnx` Python package (pip install onnx). The committed .onnx
file is what the tests read; this script is the reproducible source.
"""

from pathlib import Path
import sys


def main() -> int:
    try:
        import onnx
        from onnx import TensorProto, helper
    except ImportError:
        print("error: the `onnx` package is required (pip install onnx)", file=sys.stderr)
        return 2

    root = Path(__file__).resolve().parents[2]
    out = root / "crates" / "pdf-engine" / "tests" / "fixtures" / "vision_identity.onnx"

    # N=1 fixed; C, H, W symbolic so one fixture fits grayscale or RGB
    # rasters of any page size.
    x = helper.make_tensor_value_info("input", TensorProto.FLOAT, [1, "C", "H", "W"])
    y = helper.make_tensor_value_info("output", TensorProto.FLOAT, [1, "C", "H", "W"])
    node = helper.make_node("Identity", inputs=["input"], outputs=["output"])
    graph = helper.make_graph([node], "vision_identity", [x], [y])
    opset = helper.make_operatorsetid("", 14)
    model = helper.make_model(
        graph,
        producer_name="pdfluent-vision-fixture",
        opset_imports=[opset],
        ir_version=8,
    )
    onnx.checker.check_model(model)

    out.parent.mkdir(parents=True, exist_ok=True)
    onnx.save(model, str(out))
    size = out.stat().st_size
    print(f"wrote {out.relative_to(root)} ({size} bytes)")
    if size >= 50 * 1024:
        print(f"error: fixture exceeds the 50 KB budget ({size} bytes)", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
