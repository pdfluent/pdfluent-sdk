#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""`Rect` for the vendored TATR post-processing: the stdlib stand-in we ship.

`tatr_postprocess.py` imports `Rect` from this file and uses four things
of it: the constructor from a bbox or empty, `intersect`, `include_rect`,
`get_area`, and indexing/iteration. This stand-in is the ONLY Rect the
tooling knows: nothing here imports or prefers any outside Rect library.
The runs record `rect_backend` "stub" beside their TATR numbers, so later
runs stay comparable with the recorded ones.

The semantics the vendored calls need, read off them rather than assumed:

- `Rect()` is the EMPTY rect, the infinite inverse: the first `include_rect`
  becomes its argument rather than a union with the origin. The publisher's
  `row_column_bbox` accumulates from it, so a `(0, 0, 0, 0)` here would drag
  every table's rows to the page top.
- `intersect` returns a NEW rect and never touches its receiver: two of its
  call sites reuse the receiver across a loop, which would shrink it to
  nothing if the call mutated.
- `include_rect` mutates AND returns the receiver: one call site assigns it,
  the loops rely on the mutation.
- the area of an empty or inverted rect is 0, never negative or infinite: the
  overlap fractions divide by it.
"""
from __future__ import annotations

_INF = float("inf")


class Rect:
    """The four operations the vendored file uses, with the calls' semantics."""

    __slots__ = ("x0", "y0", "x1", "y1")

    def __init__(self, *args) -> None:
        if not args:
            self.x0, self.y0, self.x1, self.y1 = _INF, _INF, -_INF, -_INF
        elif len(args) == 1:
            x0, y0, x1, y1 = _coords(args[0])
            self.x0, self.y0, self.x1, self.y1 = x0, y0, x1, y1
        elif len(args) == 4:
            self.x0, self.y0, self.x1, self.y1 = (float(v) for v in args)
        else:
            raise ValueError(f"Rect takes 0, 1 or 4 arguments, not {len(args)}")

    def __getitem__(self, index: int) -> float:
        return (self.x0, self.y0, self.x1, self.y1)[index]

    def __iter__(self):
        yield self.x0
        yield self.y0
        yield self.x1
        yield self.y1

    def __len__(self) -> int:
        return 4

    def __repr__(self) -> str:  # pragma: no cover - debugging only
        return f"Rect({self.x0}, {self.y0}, {self.x1}, {self.y1})"

    @property
    def is_empty(self) -> bool:
        return self.x0 >= self.x1 or self.y0 >= self.y1

    def get_area(self) -> float:
        """The area, 0 when the rect is empty or inverted."""
        if self.is_empty:
            return 0.0
        return (self.x1 - self.x0) * (self.y1 - self.y0)

    def intersect(self, other) -> "Rect":
        """A NEW rect: the overlap of this one and `other`."""
        x0, y0, x1, y1 = _coords(other)
        return Rect(max(self.x0, x0), max(self.y0, y0),
                    min(self.x1, x1), min(self.y1, y1))

    def include_rect(self, other) -> "Rect":
        """Grow this rect to cover `other`, and return this rect.

        An empty rect BECOMES its argument: it holds nothing yet, and a union
        with the infinite inverse would stay infinite.
        """
        x0, y0, x1, y1 = _coords(other)
        if self.x0 == _INF and self.y0 == _INF:
            self.x0, self.y0, self.x1, self.y1 = x0, y0, x1, y1
        else:
            self.x0 = min(self.x0, x0)
            self.y0 = min(self.y0, y0)
            self.x1 = max(self.x1, x1)
            self.y1 = max(self.y1, y1)
        return self


def _coords(box) -> tuple[float, float, float, float]:
    if isinstance(box, Rect):
        return box.x0, box.y0, box.x1, box.y1
    x0, y0, x1, y1 = box
    return float(x0), float(y0), float(x1), float(y1)


def ensure() -> str:
    """Name the Rect backend the runs record: always `"stub"`.

    The stand-in above is the only Rect -- no outside library is imported
    or preferred anywhere -- so there is nothing to choose. The eval
    records the answer beside its TATR numbers, and `"stub"` keeps later
    runs comparable with the recorded ones.
    """
    return "stub"
