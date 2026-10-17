#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Does this interpreter train LightGBM on several threads, and get it right? (#598)

Trains the same deterministic model on four threads and on one, predicts with
each on as many threads, and requires the margins to be identical. A process
with two OpenMP runtimes crashes here (no traceback, exit 139) instead of in the
middle of a lane's training run; one that is loaded wrongly but survives is
caught by the comparison. Refuses KMP_DUPLICATE_LIB_OK, which would hide both.
"""
from __future__ import annotations

import os
import sys


def main() -> int:
    if os.environ.get("KMP_DUPLICATE_LIB_OK"):
        print("smoke_threads: KMP_DUPLICATE_LIB_OK is set; this proves nothing with it. Unset it.",
              file=sys.stderr)
        return 1
    import lightgbm as lgb
    import numpy as np

    x = np.random.RandomState(598).rand(20000, 24)
    y = (x[:, 0] + x[:, 1] > 1.0).astype(int)
    base = {"objective": "binary", "verbosity": -1, "seed": 598, "deterministic": True,
            "force_row_wise": True}
    many = lgb.train(dict(base, num_threads=4), lgb.Dataset(x, label=y), num_boost_round=30)
    one = lgb.train(dict(base, num_threads=1), lgb.Dataset(x, label=y), num_boost_round=30)
    a = many.predict(x, raw_score=True, num_threads=4)
    b = one.predict(x, raw_score=True, num_threads=1)
    diff = float(np.abs(a - b).max())
    if diff != 0.0:
        print(f"smoke_threads: FAIL -- four threads and one disagree by {diff}", file=sys.stderr)
        return 1
    print(f"smoke_threads: OK -- lightgbm {lgb.__version__}, numpy {np.__version__}: "
          "four threads and one give identical margins")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
