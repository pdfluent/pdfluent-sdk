# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Lane L5: SPEC sections 11, 13. Flip markers as sections land."""

import json

import pytest
from conftest import (
    FIX,
    SKIP_REASON,
    SPEC_MISSING,
    assert_data_equal,
    probe_args,
    read_json,
)
from conftest import read_text as _read_text
from conftest import read_value as _read_value

import pdfluent_extract as pe
from pdfluent_extract.helpers import document_layout as dl
from pdfluent_extract.helpers import utils as du

pytestmark = pytest.mark.skipif(SPEC_MISSING, reason=SKIP_REASON)

TABLES = (
    "ruled-table.pdf",
    "unruled-table.pdf",
    "headerless-table.pdf",
    "shaded-table.pdf",
    "spanning-table.pdf",
)
STRATS = ("lines_strict", "lines", "text")


def _legacy():
    pe.use_layout(False)


def _strat_out(fixture, strat):
    stem = fixture[: -len(".pdf")]
    return _read_text("legacy-tables/strat-%s-%s.md" % (strat, stem))


def test_g97_ruled_all_strategies():
    _legacy()
    path = str(FIX("ruled-table.pdf"))
    for strat in STRATS:
        out = pe.to_markdown(path, table_strategy=strat)
        assert out == _strat_out("ruled-table.pdf", strat)
        assert "|---|---|" in out


def test_g98_unruled_strategies():
    _legacy()
    path = str(FIX("unruled-table.pdf"))
    for strat in STRATS:
        assert pe.to_markdown(path, table_strategy=strat) == _strat_out(
            "unruled-table.pdf", strat
        )


def test_g99_headerless_strategies():
    _legacy()
    path = str(FIX("headerless-table.pdf"))
    for strat in STRATS:
        assert pe.to_markdown(path, table_strategy=strat) == _strat_out(
            "headerless-table.pdf", strat
        )


def test_g100_shaded_strategies():
    _legacy()
    path = str(FIX("shaded-table.pdf"))
    assert pe.to_markdown(path, table_strategy="lines_strict") == ""
    for strat in STRATS:
        assert pe.to_markdown(path, table_strategy=strat) == _strat_out(
            "shaded-table.pdf", strat
        )


def test_g101_spanning_strategies():
    _legacy()
    path = str(FIX("spanning-table.pdf"))
    for strat in STRATS:
        assert pe.to_markdown(path, table_strategy=strat) == _strat_out(
            "spanning-table.pdf", strat
        )


def test_g102_bogus_strategy_refused():
    _legacy()
    for fixture in TABLES:
        with pytest.raises(AttributeError):
            pe.to_markdown(str(FIX(fixture)), table_strategy="zzz-nope")


def test_g103_bogus_output_refused():
    _legacy()
    path = str(FIX("ruled-table.pdf"))
    for output in ("text", "grid", "zzz-nope"):
        with pytest.raises(ValueError):
            pe.to_markdown(path, table_output=output)


def test_g104_layout_tables_found():
    for fixture, stem in (
        ("ruled-table.pdf", "md-ruled-table.md"),
        ("unruled-table.pdf", "md-unruled-table.md"),
        ("headerless-table.pdf", "md-headerless-table.md"),
    ):
        assert pe.to_markdown(str(FIX(fixture))) == _read_text("jsonsweep/" + stem)


def test_g105_layout_shaded_runs_together():
    out = pe.to_markdown(str(FIX("shaded-table.pdf")))
    assert out == _read_text("jsonsweep/md-shaded-table.md")
    assert "|" not in out


def test_g106_layout_spanning_flattened():
    assert pe.to_markdown(str(FIX("spanning-table.pdf"))) == _read_text(
        "jsonsweep/md-spanning-table.md"
    )


def test_g107_strategy_ignored_layout():
    assert pe.to_markdown(
        str(FIX("ruled-table.pdf")), table_strategy="lines"
    ) == _read_text("layout-params/strategy-lines.md")


def test_g108_band_tables_layout():
    assert pe.to_markdown(str(FIX("graphics-limit-pages.pdf"))) == _read_text(
        "misc2/glp-layout.md"
    )


def test_g109_html_edge_noops():
    path = str(FIX("ruled-table.pdf"))
    default = pe.to_markdown(path)
    assert pe.to_markdown(path, render_html_tables=True) == default
    assert pe.to_markdown(path, render_html_tables=False) == default
    assert pe.to_markdown(path, edge_threshold=0.5) == default
    assert pe.to_markdown(path, render_html_tables=True) == _read_text(
        "misc2/render-html-t.md"
    )
    assert pe.to_markdown(path, render_html_tables=False) == _read_text(
        "misc2/render-html-f.md"
    )
    assert pe.to_markdown(path, edge_threshold=0.5) == _read_text("misc2/edge-thr.md")


def test_g110_table_helper():
    args, kwargs, out = probe_args("pure/probes.json", "cells-md")
    got = du.table_to_markdown(*args, **kwargs)
    rel = out[len("results/") :]
    assert got == _read_value(rel)
    assert "**" not in got


def test_g115_text_strips_markers():
    out = pe.to_text(str(FIX("headings-styles.pdf")))
    assert out == _read_text("basics/text-headings.txt")
    assert "#" not in out and "**" not in out and "_" not in out and "`" not in out


def test_g116_text_unfolds_code():
    assert pe.to_text(str(FIX("code-block.pdf"))) == _read_text(
        "basics/text-codeblock.txt"
    )
    assert pe.to_text(str(FIX("code-snippet.pdf"))) == _read_text("misc2/snip-text.txt")


def test_g117_text_bullets():
    assert pe.to_text(str(FIX("lists.pdf"))) == _read_text("basics/text-lists.txt")
    assert pe.to_text(str(FIX("sample.epub"))) == _read_text("nonpdf/epub-text.txt")


def test_g118_text_grid_default():
    out = pe.to_text(str(FIX("ruled-table.pdf")))
    assert out == _read_text("basics/text-ruled.txt")
    assert "+----------+" in out


def test_g119_text_grid_aliases():
    path = str(FIX("ruled-table.pdf"))
    grid = pe.to_text(path, table_format="grid")
    markdown = pe.to_text(path, table_format="markdown")
    bogus = pe.to_text(path, table_format="zzz")
    assert grid == _read_text("layout-params/tablefmt-grid.txt")
    assert markdown == _read_text("layout-params/tablefmt-markdown.txt")
    assert bogus == _read_text("layout-params/tablefmt-zzz.txt")
    assert grid == markdown == bogus


def test_g120_text_grid_frames():
    path = str(FIX("ruled-table.pdf"))
    assert pe.to_text(path, table_format="plain") == _read_text(
        "layout-params/tablefmt-plain.txt"
    )
    assert pe.to_text(path, table_format="psql") == _read_text(
        "layout-params/tablefmt-psql.txt"
    )
    assert pe.to_text(path, table_format="simple") == _read_text(
        "layout-params/tablefmt-simple.txt"
    )


def test_g121_text_widths_ignored():
    path = str(FIX("ruled-table.pdf"))
    narrow = pe.to_text(path, table_max_width=10, table_min_col_width=2)
    wide = pe.to_text(path, table_max_width=500, table_min_col_width=40)
    assert narrow == _read_text("layout-params/tablewidth-narrow.txt")
    assert wide == _read_text("layout-params/tablewidth-wide.txt")
    assert narrow == wide


def test_g122_legacy_text_json_refuse():
    sample = pe.to_text(str(FIX("multi-page.pdf")))
    assert isinstance(sample, str) and len(sample) > 0
    json.loads(pe.to_json(str(FIX("multi-page.pdf"))))
    _legacy()
    with pytest.raises(NotImplementedError):
        pe.to_text(str(FIX("multi-page.pdf")))
    with pytest.raises(NotImplementedError):
        pe.to_json(str(FIX("multi-page.pdf")))


def test_g123_footnotes_quoted(tmp_path):
    args, kwargs, out = probe_args("pure/probes.json", "foot-md")
    got = dl.footnote_to_md(*args, **kwargs)
    assert got == _read_value(out[len("results/") :])
    assert got.startswith("> ")
    args, kwargs, out = probe_args("pure/probes.json", "foot-text")
    got = dl.footnote_to_text(*args, **kwargs)
    assert got == _read_value(out[len("results/") :])
    assert got.startswith("> ")
    dest = str(tmp_path) + "/"
    out = pe.to_markdown(
        str(FIX("chunk-classes.pdf")), write_images=True, image_path=dest
    )
    recorded = _read_text("images/layout-write-cc.md")
    assert [ln for ln in out.splitlines() if ln.startswith("> ")] == [
        ln for ln in recorded.splitlines() if ln.startswith("> ")
    ]


def test_g124_compat_text_markers():
    out = pe.to_text(str(FIX("compat-frame.pdf")))
    assert out == _read_text("misc2/compat-text.txt")
    assert "----- Start of picture text -----" in out
    assert "----- End of picture text -----" in out


def test_l5_extract_cells_contract():
    blocks = [
        {"bbox": [0, 0, 10, 10], "text": "hello"},
        {"bbox": [20, 0, 30, 10], "text": "away"},
    ]
    assert du.extract_cells(blocks, [0, 0, 10, 10]) == "hello"
    assert du.extract_cells(blocks, [0, 0, 10, 10], markdown=True) == "hello<br>"
    assert du.extract_cells(blocks, [100, 100, 110, 110]) == ""


def test_l5_table_details_contract():
    tab = {
        "bbox": [0, 0, 30, 10],
        "rows": 1,
        "columns": 2,
        "cells": [[[0, 0, 10, 10], [20, 0, 30, 10]]],
    }
    blocks = [
        {"bbox": [0, 0, 10, 10], "text": "a"},
        {"bbox": [20, 0, 30, 10], "text": "b"},
    ]
    details = dl.get_table_details(tab, blocks)
    assert details.extract == [["a", "b"]]
    assert details.markdown == "|**a**|**b**|\n|---|---|\n\n"
    assert (details.row_count, details.col_count) == (1, 2)
    assert tuple(details.bbox) == (0, 0, 30, 10)
    plain = dl.get_table_details(dict(tab, header=False), blocks)
    assert plain.markdown == "|a|b|\n|---|---|\n\n"
    assert plain.extract == [["a", "b"]]
