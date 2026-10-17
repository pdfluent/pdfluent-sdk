#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Reference outputs for the Rust PIL-style bilinear resampler (#641).

The layout model preprocesses with PIL BILINEAR (HF RTDetrImageProcessor,
resample=2, reducing_gap=None), which antialiases on downscale by widening
the triangle support instead of sampling a plain 2x2 neighbourhood. The Rust
resampler in `crates/pdf-engine/src/layout.rs` implements that filter, and
its unit tests embed the outputs below.

Generator: `Pillow==10.2.0` exactly (asserted). Usage:

    python3 crates/pdf-engine/tests/fixtures/ort_layout/pil_resize_reference.py
"""
import sys

REQUIRED_PILLOW = "10.2.0"

import PIL
from PIL import Image

if PIL.__version__ != REQUIRED_PILLOW:
    sys.exit(f"want Pillow=={REQUIRED_PILLOW}, have {PIL.__version__}")


def show(name, w, h, px, nw, nh):
    out = Image.frombytes("RGB", (w, h), bytes(px)).resize(
        (nw, nh), resample=Image.BILINEAR
    )
    print(f"{name}: {w}x{h} -> {nw}x{nh}")
    print(f"  in : {px}")
    print(f"  out: {list(out.tobytes())}")


# Downscale with a smooth ramp: the edge pixels (46, 146) only land there
# when the clipped support renormalises, as PIL does.
ramp4 = []
for y in range(4):
    for x in range(4):
        ramp4 += [x * 64, y * 64, 128]
show("ramp4", 4, 4, ramp4, 2, 2)

# Downscale with alternating pixels: a plain 2x2 bilinear reads 0 or 255
# here depending on alignment, PIL's widened support reads ~128.
checker4 = []
for y in range(4):
    for x in range(4):
        v = 255 if (x + y) % 2 == 0 else 0
        checker4 += [v, v, v]
show("checker4", 4, 4, checker4, 2, 2)

# A hard edge downscaled: the 36 and 223 only land there with support 2.
edge8 = [0, 0, 0] * 2 + [255, 255, 255] * 6
show("edge8", 8, 1, edge8, 4, 1)

# Upscale: support 1, plain triangle between neighbours.
ramp3 = []
for y in range(3):
    for x in range(3):
        ramp3 += [x * 100, y * 100, 50]
show("ramp3up", 3, 3, ramp3, 5, 5)
