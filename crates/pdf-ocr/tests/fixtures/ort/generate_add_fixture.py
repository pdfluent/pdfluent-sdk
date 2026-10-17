#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Regenerate the tiny Add fixture for the shared ONNX Runtime layer (#641).

The fixture is a single-`Add` graph, a few hundred bytes: inputs `x` and `y`
(float[2,2]), output `z = x + y`. The `onnx-runtime` integration tests load it
through `pdf_ocr::runtime::session` and check the returned tensor, so the
shared layer proves it can run a model end to end without committing a real
weight.

Layout mirrors a model package: `tests/fixtures/ort/<name>/<file>` is used as
`<model_dir>/<name>/<file>`, with the spec's sha256 pinned in the test.

Generator: `onnx==1.19.0` exactly (asserted below). Usage:

    python3 -m venv /tmp/onnx-gen && /tmp/onnx-gen/bin/pip install "onnx==1.19.0"
    /tmp/onnx-gen/bin/python crates/pdf-ocr/tests/fixtures/ort/generate_add_fixture.py
"""
import pathlib
import sys

REQUIRED_ONNX = "1.19.0"

try:
    import onnx
except ModuleNotFoundError:
    sys.exit("needs the onnx package: pip install \"onnx==%s\"" % REQUIRED_ONNX)
if onnx.__version__ != REQUIRED_ONNX:
    sys.exit("needs onnx==%s, found %s" % (REQUIRED_ONNX, onnx.__version__))

from onnx import TensorProto, helper

HERE = pathlib.Path(__file__).resolve().parent
OUT = HERE / "add" / "add.onnx"

x = helper.make_tensor_value_info("x", TensorProto.FLOAT, [2, 2])
y = helper.make_tensor_value_info("y", TensorProto.FLOAT, [2, 2])
z = helper.make_tensor_value_info("z", TensorProto.FLOAT, [2, 2])
node = helper.make_node("Add", inputs=["x", "y"], outputs=["z"])
graph = helper.make_graph([node], "add-two-by-two", [x, y], [z])
# opset 17 / ir 8: old enough that every ORT this layer will ever pin reads
# it, new enough to be an ordinary export.
model = helper.make_model(
    graph,
    producer_name="pdfluent-fixture",
    opset_imports=[helper.make_opsetid("", 17)],
    ir_version=8,
)
onnx.checker.check_model(model)

OUT.parent.mkdir(parents=True, exist_ok=True)
OUT.write_bytes(model.SerializeToString())
print("wrote %s (%d bytes)" % (OUT, OUT.stat().st_size))
