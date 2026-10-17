# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Self-test for the D7 door-difference verdict (owner 2026-10-10).

The harness reports the three listed chunk tests as ``difference`` when
their only mismatch sits in page-box bboxes, and still fails them on any
new divergence. Each test below pins both directions, so removing the
mechanism (never difference) or the filter (every mismatch a difference)
fails it. Needs no spec checkout.
"""

import os

import pytest
from conftest import (
    DOOR_DIFFERENCES,
    DoorDifference,
    assert_data_equal,
    door_difference_reason,
)


def _chunks(bbox, text="t", cls="text", pos=None):
    box = {"index": 0, "class": cls, "bbox": list(bbox),
           "pos": list(pos) if pos else [0, 1]}
    return [{"metadata": {}, "toc_items": [], "page_boxes": [box],
             "text": text}]


def test_door_difference_marks_only_base14_boxes():
    listed = "test_g53_chunks_page_subset"
    want = _chunks([72, 80, 185, 93])
    # Approved: any bbox edge, alone or together, is a difference.
    for bbox in ([72, 92, 185, 93], [72, 80, 175, 93],
                 [72, 92, 175, 92], [70, 80, 185, 93]):
        reason = door_difference_reason(listed, _chunks(bbox), want)
        assert reason == "difference (owner 10-10): base-14 ink metrics", bbox
    # New divergences on a listed test still fail (no reason).
    assert door_difference_reason(
        listed, _chunks([72, 80, 185, 93], text="other"), want) is None
    assert door_difference_reason(
        listed, _chunks([72, 80, 185, 93], cls="caption"), want) is None
    assert door_difference_reason(
        listed, _chunks([72, 80, 185, 93], pos=[0, 2]), want) is None
    assert door_difference_reason(
        listed, _chunks([72, 80, 185, 93]) + _chunks([72, 80, 185, 93]),
        want) is None
    # Unlisted tests and exact matches never report a difference.
    assert door_difference_reason(
        "test_g50_layout_chunk_keys", _chunks([72, 92, 185, 93]),
        want) is None
    assert door_difference_reason(listed, want, want) is None
    # The approved set is exactly the three D7 chunk tests.
    assert set(DOOR_DIFFERENCES) == {
        "test_g53_chunks_page_subset",
        "test_g54_layout_metadata",
        "test_g56_toc_per_page",
    }
    for reason in DOOR_DIFFERENCES.values():
        assert "(owner 10-10)" in reason, reason


def test_door_difference_end_to_end_still_fails_new_divergence(monkeypatch):
    want = _chunks([72, 80, 185, 93])
    monkeypatch.setenv("PYTEST_CURRENT_TEST",
                       "test_l3_chunks.py::test_g54_layout_metadata (call)")
    with pytest.raises(DoorDifference) as exc:
        assert_data_equal(_chunks([72, 92, 175, 92]), want)
    assert "difference (owner 10-10): base-14 ink metrics" in str(exc.value)
    with pytest.raises(AssertionError) as exc:
        assert_data_equal(_chunks([72, 80, 185, 93], text="new text"), want)
    assert type(exc.value) is AssertionError
    monkeypatch.setenv("PYTEST_CURRENT_TEST",
                       "test_l3_chunks.py::test_g50_layout_chunk_keys (call)")
    with pytest.raises(AssertionError) as exc:
        assert_data_equal(_chunks([72, 92, 175, 92]), want)
    assert type(exc.value) is AssertionError
