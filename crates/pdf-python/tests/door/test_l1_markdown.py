# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Lane L1: SPEC sections 2, 3, 10, 12, 18, 23, 24. Flip markers as sections land."""

from contextlib import contextmanager

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
from pdfluent_extract.helpers import get_text_lines as gtl
from pdfluent_extract.helpers import legacy

pytestmark = [
    pytest.mark.skipif(SPEC_MISSING, reason=SKIP_REASON),
]


@contextmanager
def _page(fixture, number=0):
    pdfluent = pytest.importorskip(
        "pdfluent", reason="native wheel needed for page input"
    )
    with pdfluent.Document(str(FIX(fixture))) as doc:
        yield doc[number]


def _legacy():
    pe.use_layout(False)


def _pure(pid, func, probes="pure/probes.json"):
    args, kwargs, out = probe_args(probes, pid)
    got = func(*args, **kwargs)
    rel = out[len("results/") :] if out.startswith("results/") else out
    return got, _read_value(rel)


def test_g4_heading_styles():
    assert pe.to_markdown(str(FIX("headings-styles.pdf"))) == _read_text(
        "basics/md-headings.md"
    )


def test_g5_heading_styles_legacy():
    _legacy()
    assert pe.to_markdown(str(FIX("headings-styles.pdf"))) == _read_text(
        "legacy-basics/md-headings-styles.md"
    )


def test_g6_lists():
    assert pe.to_markdown(str(FIX("lists.pdf"))) == _read_text("basics/md-lists.md")


def test_g7_lists_legacy():
    _legacy()
    assert pe.to_markdown(str(FIX("lists.pdf"))) == _read_text(
        "legacy-basics/md-lists.md"
    )


def test_g8_codeblock():
    assert pe.to_markdown(str(FIX("code-block.pdf"))) == _read_text(
        "basics/md-codeblock.md"
    )


def test_g9_codeblock_legacy():
    _legacy()
    assert pe.to_markdown(str(FIX("code-block.pdf"))) == _read_text(
        "legacy-basics/md-code-block.md"
    )


def test_g10_codesnippet_both():
    assert pe.to_markdown(str(FIX("code-snippet.pdf"))) == _read_text(
        "basics/md-codesnippet.md"
    )
    _legacy()
    assert pe.to_markdown(str(FIX("code-snippet.pdf"))) == _read_text(
        "legacy-basics/md-code-snippet.md"
    )


def test_g11_twocol():
    assert pe.to_markdown(str(FIX("two-column.pdf"))) == _read_text(
        "basics/md-twocol.md"
    )


def test_g12_twocol_legacy():
    _legacy()
    assert pe.to_markdown(str(FIX("two-column.pdf"))) == _read_text(
        "legacy-basics/md-two-column.md"
    )


@pytest.mark.xfail(strict=True, reason="needs layout parse+render (L3)")
def test_g13_rotated_both():
    assert pe.to_markdown(str(FIX("rotated-text.pdf"))) == _read_text(
        "basics/md-rotated.md"
    )
    _legacy()
    assert pe.to_markdown(str(FIX("rotated-text.pdf"))) == _read_text(
        "legacy-basics/md-rotated-text.md"
    )


def test_g14_blank_page():
    assert pe.to_markdown(str(FIX("blank-page.pdf"))) == _read_text(
        "basics/md-blank.md"
    )
    assert pe.to_text(str(FIX("blank-page.pdf"))) == _read_text("misc2/blank-text.txt")
    _legacy()
    assert pe.to_markdown(str(FIX("blank-page.pdf"))) == _read_text(
        "legacy-basics/md-blank-page.md"
    )


def test_g15_repeated_equal():
    first = pe.to_markdown(str(FIX("multi-page.pdf")))
    second = pe.to_markdown(str(FIX("multi-page.pdf")))
    assert first == second == _read_text("smoke/path-str.md")


def test_g16_garbled_markers_both():
    out = pe.to_markdown(str(FIX("headings-styles.pdf")))
    assert out == _read_text("basics/md-headings.md")
    assert "\ufffd" in out and "```" in out
    _legacy()
    out = pe.to_markdown(str(FIX("headings-styles.pdf")))
    assert out == _read_text("legacy-basics/md-headings-styles.md")
    assert "\ufffd" in out and "```" in out


def test_g17_hdr_none_matches_default():
    _legacy()
    path = str(FIX("font-sizes.pdf"))
    assert pe.to_markdown(path, hdr_info=None) == _read_text(
        "legacy-params/hdr-none.md"
    )
    assert pe.to_markdown(path, hdr_info=legacy.IdentifyHeaders(path)) == _read_text(
        "legacy-params/hdr-none.md"
    )


def test_g18_submodule_needs_hdr():
    with pytest.raises(AttributeError):
        legacy.to_markdown(str(FIX("font-sizes.pdf")))


def test_g19_ident_from_path():
    _legacy()
    path = str(FIX("multi-page.pdf"))
    pdfluent = pytest.importorskip(
        "pdfluent", reason="native wheel needed for doc input"
    )
    with pdfluent.Document(path) as doc:
        out = legacy.to_markdown(doc, hdr_info=legacy.IdentifyHeaders(path))
    assert out == _read_text("smoke/rag-hdr-md.md")


def test_g20_ident_options_same():
    _legacy()
    path = str(FIX("font-sizes.pdf"))
    pdfluent = pytest.importorskip(
        "pdfluent", reason="native wheel needed for doc input"
    )

    with pdfluent.Document(path) as doc:
        tuned = legacy.to_markdown(
            doc,
            hdr_info=legacy.IdentifyHeaders(
                path, pages=[0], body_limit=10, max_levels=3
            ),
        )
    with pdfluent.Document(path) as doc:
        default = legacy.to_markdown(doc, hdr_info=legacy.IdentifyHeaders(path))
    assert tuned == _read_text("legacy-params/hdr-ident-pages.md")
    assert default == _read_text("legacy-params/hdr-ident-default.md")
    assert tuned == default


def test_g21_toc_from_path_refused():
    with pytest.raises(AttributeError):
        legacy.TocHeaders(str(FIX("toc-meta.pdf")))
    with pytest.raises(AttributeError):
        legacy.TocHeaders(str(FIX("multi-page.pdf")))


def test_g22_toc_from_doc():
    _legacy()
    pdfluent = pytest.importorskip(
        "pdfluent", reason="native wheel needed for doc input"
    )

    with pdfluent.Document(str(FIX("toc-meta.pdf"))) as doc:
        out = legacy.to_markdown(doc, hdr_info=legacy.TocHeaders(doc))
    assert out == _read_text("smoke/rag-toc-doc-md.md")
    with pdfluent.Document(str(FIX("multi-page.pdf"))) as doc:
        out = legacy.to_markdown(doc, hdr_info=legacy.TocHeaders(doc))
    assert out == _read_text("legacy-params/hdr-toc-multi.md")


def test_g23_detectors_agree_on_bookmarks():
    _legacy()
    path = str(FIX("toc-meta.pdf"))
    pdfluent = pytest.importorskip(
        "pdfluent", reason="native wheel needed for doc input"
    )

    with pdfluent.Document(path) as doc:
        ident_out = legacy.to_markdown(doc, hdr_info=legacy.IdentifyHeaders(path))
    with pdfluent.Document(path) as doc:
        toc_out = legacy.to_markdown(doc, hdr_info=legacy.TocHeaders(doc))
    assert ident_out == toc_out
    assert ident_out == _read_text("misc2/rag-ident-tocmeta.md")
    assert toc_out == _read_text("smoke/rag-toc-doc-md.md")


def test_g24_toc_silent_without_bookmarks():
    _legacy()
    pdfluent = pytest.importorskip(
        "pdfluent", reason="native wheel needed for doc input"
    )

    with pdfluent.Document(str(FIX("multi-page.pdf"))) as doc:
        toc_out = legacy.to_markdown(doc, hdr_info=legacy.TocHeaders(doc))
    assert toc_out == _read_text("legacy-params/hdr-toc-multi.md")
    assert "#" not in toc_out
    default_out = pe.to_markdown(str(FIX("multi-page.pdf")))
    assert default_out == _read_text("smoke/legacy.md")
    assert "#" in default_out


def test_g25_sizes_no_markers_legacy():
    _legacy()
    out = pe.to_markdown(str(FIX("font-sizes.pdf")))
    assert out == _read_text("legacy-basics/md-font-sizes.md")
    assert "#" not in out


def test_g26_sizes_headed_layout():
    assert pe.to_markdown(str(FIX("font-sizes.pdf"))) == _read_text(
        "basics/md-fontsizes.md"
    )


@pytest.mark.xfail(strict=True, reason="needs layout parse+render (L3)")
def test_g27_scan_heading_plain():
    out = pe.to_markdown(str(FIX("scanned-words.pdf")))
    assert out == _read_text("ocrwords/words-default-md.md")
    assert out.startswith("# ") and "**" not in out.splitlines()[0]


def test_g94_ignore_code_legacy():
    _legacy()
    assert pe.to_markdown(str(FIX("code-block.pdf")), ignore_code=True) == _read_text(
        "legacy-params/ignorecode-t.md"
    )


def test_g95_ignore_code_layout():
    assert pe.to_markdown(str(FIX("code-block.pdf")), ignore_code=True) == _read_text(
        "layout-params/ignorecode-t.md"
    )


def test_g96_ignore_code_text():
    assert pe.to_text(str(FIX("code-block.pdf")), ignore_code=True) == _read_text(
        "layout-params/txt-ignorecode-t.txt"
    )


@pytest.mark.xfail(strict=True, reason="needs layout render (L3/L5)")
def test_g111_heads_kept_default():
    assert pe.to_markdown(str(FIX("header-footer.pdf"))) == _read_text(
        "basics/md-headerfooter.md"
    )
    assert pe.to_text(str(FIX("header-footer.pdf"))) == _read_text(
        "layout-params/txthdr-all.txt"
    )


@pytest.mark.xfail(strict=True, reason="needs layout render (L3/L5)")
def test_g112_head_foot_flags():
    path = str(FIX("header-footer.pdf"))
    assert pe.to_markdown(path, header=False, footer=False) == _read_text(
        "layout-params/hdr-FF.md"
    )
    assert pe.to_markdown(path, header=True, footer=False) == _read_text(
        "layout-params/hdr-hF.md"
    )
    assert pe.to_markdown(path, header=False, footer=True) == _read_text(
        "layout-params/hdr-fT.md"
    )
    assert pe.to_text(path, header=False, footer=False) == _read_text(
        "layout-params/txthdr-none.txt"
    )


def test_g113_header_string_tolerated():
    assert pe.to_markdown(str(FIX("multi-page.pdf")), header="yes") == _read_text(
        "errors/header-str.md"
    )


@pytest.mark.xfail(strict=True, reason="needs layout parse+render (L3)")
def test_g114_compat_no_heads():
    out = pe.to_markdown(str(FIX("compat-frame.pdf")), header=False, footer=False)
    assert out == _read_text("layout-params/hdr-compat-none.md")
    assert "<!-- Start of picture text -->" in out and "<br>" in out


def test_g163_hidden_dropped_default():
    _legacy()
    assert pe.to_markdown(
        str(FIX("invisible-text.pdf")), ignore_alpha=False
    ) == _read_text("legacy-params/ignorealpha-f.md")


def test_g164_hidden_kept_ignore_alpha():
    _legacy()
    assert pe.to_markdown(
        str(FIX("invisible-text.pdf")), ignore_alpha=True
    ) == _read_text("legacy-params/ignorealpha-t.md")


@pytest.mark.xfail(strict=True, reason="needs layout render (L3/L5)")
def test_g165_hidden_dropped_layout():
    import json

    assert pe.to_markdown(str(FIX("invisible-text.pdf"))) == _read_text(
        "misc2/invis-layout-md.md"
    )
    assert pe.to_text(str(FIX("invisible-text.pdf"))) == _read_text(
        "misc2/invis-layout-text.txt"
    )
    assert_data_equal(
        json.loads(pe.to_json(str(FIX("invisible-text.pdf")))),
        read_json("misc2/invis-layout-json.json"),
    )


def test_g166_glyphs_one_space():
    _legacy()
    path = str(FIX("headings-styles.pdf"))
    out_t = pe.to_markdown(path, use_glyphs=True)
    out_f = pe.to_markdown(path, use_glyphs=False)
    assert out_t == _read_text("legacy-params/useglyphs-t.md")
    assert out_f == _read_text("legacy-params/useglyphs-f.md")
    assert out_t.replace(" ", "") == out_f.replace(" ", "")


@pytest.mark.xfail(strict=True, reason="span metrics/bidi/char_flags unwired")
def test_g167_raw_lines_textpage():
    with _page("multi-page.pdf") as page:
        got = legacy.get_raw_lines(textpage=page)
    want = read_json("unit/grl-tp.json")
    assert len(got) == len(want)
    for (bbox, spans), (_, w_spans) in zip(got, want):
        assert bbox is not None
        assert_data_equal(spans, w_spans)


def test_g168_blocks_shapes_refused():
    with pytest.raises(TypeError):
        legacy.get_raw_lines(blocks=[(1, 2, 3, 4, "hi", 0, 0)])
    with pytest.raises(TypeError):
        legacy.get_raw_lines(blocks={"blocks": []})
    with pytest.raises(TypeError):
        legacy.get_raw_lines(blocks=[{"type": 0, "lines": []}])


def test_g198_section_and_title():
    got, want = _pure("sec-hdr", dl.section_hdr_to_md)
    assert got == want
    got, want = _pure("title", dl.title_to_md)
    assert got == want


def test_g199_prose_markdown_variants():
    got, want = _pure("text-md", dl.text_to_md)
    assert got == want
    got, want = _pure("fall-md", dl.fallback_text_to_md)
    assert got == want and "|" in got and "<!--" in got
    got, want = _pure("pic-md", dl.picture_text_to_md)
    assert got == want and "<br>" in got and "<!--" in got


def test_g200_prose_text_variants():
    got, want = _pure("text-text", dl.text_to_text)
    assert got == want
    got, want = _pure("fall-text", dl.fallback_text_to_text)
    assert got == want and "+---" in got
    got, want = _pure("pic-text", dl.picture_text_to_text)
    assert got == want and "-----" in got


def test_g201_code_variants():
    got, want = _pure("code-md", dl.code_block_to_md)
    assert got == want
    got, want = _pure("code-text", dl.code_block_to_text)
    assert got == want


@pytest.mark.xfail(strict=True, reason="recorded indent (3sp) contradicts test (4sp)")
def test_g202_list_levels():
    cases = [
        ("list-md0", "pure/probes.json", 0),
        ("list-md1", "final/probes.json", 0),
        ("list-md1", "pure/probes.json", 4),
        ("list-md3", "final/probes.json", 6),
    ]
    for pid, probes, indent in cases:
        got, want = _pure(pid, dl.list_item_to_md, probes)
        assert got == want
        assert len(got) - len(got.lstrip(" ")) == indent


def test_g203_list_text():
    got, want = _pure("list-text", dl.list_item_to_text)
    assert got == want and got.startswith("•  ")


def test_g204_styled_and_plain():
    for pid in ("styled", "styled-code"):
        got, want = _pure(pid, dl.get_styled_text)
        assert_data_equal(got, want)
        assert len(got) == 2
    for pid in ("plain", "plain-code"):
        got, want = _pure(pid, dl.get_plain_text)
        assert got == want and got.endswith(" ")


def test_g205_monospaced():
    got, want = _pure("mono-code", dl.is_monospaced)
    assert got is True and want is True
    got, want = _pure("mono-prose", dl.is_monospaced)
    assert got is False and want is False


def test_g206_bullets():
    cases = (
        ("bullet1", True),
        ("bullet2", False),
        ("bullet3", False),
        ("bullet4", True),
    )
    for pid, pid_want in cases:
        got, want = _pure(pid, legacy.startswith_bullet)
        assert got is pid_want and want is pid_want


def test_g207_white():
    for pid, pid_want in (("white1", True), ("white2", False), ("white3", True)):
        got, want = _pure(pid, legacy.is_white)
        assert got is pid_want and want is pid_want


def test_g208_pua():
    got, want = _pure("pua-explicit", dl.omit_if_pua_char, "final/probes2.json")
    assert got == "" and want == ""
    got, want = _pure("pua-mid", dl.omit_if_pua_char, "final/probes2.json")
    assert got == want == "A\ue000B"


def test_g209_mdpath():
    from pdfluent_extract.helpers import utils as _du

    got, want = _pure("mdpath", _du.md_path)
    assert_data_equal(got, want)
    assert got == ["results/x/doc.pdf", "results/x/doc.pdf"]


def test_g210_overlap_helpers():
    import pytest as _pt

    from pdfluent_extract.helpers import utils as _du

    got, _ = _pure("iou", _du.iou)
    assert got == _pt.approx(1 / 7)
    got, want = _pure("bboxin1", _du.bbox_in_bbox)
    assert got is True and want is True
    got, want = _pure("bboxin2", _du.bbox_in_bbox)
    assert got is False and want is False
    got, want = _pure("disj1", _du.are_disjoint)
    assert got is True and want is True
    got, want = _pure("disj2", _du.are_disjoint)
    assert got is False and want is False
    got, want = _pure("almost3", _du.almost_in_bbox, "final/probes.json")
    assert got is False and want is False


def test_g211_partial_overlap_true():
    from pdfluent_extract.helpers import utils as _du

    got, want = _pure("almost4", _du.almost_in_bbox, "final/probes.json")
    assert got is True and want is True


def test_g212_empty_boxes():
    from pdfluent_extract.helpers import utils as _du

    with pytest.raises(TypeError):
        _du.bbox_is_empty(None)
    got, want = _pure("bempty1", _du.bbox_is_empty)
    assert got is True and want is True
    got, want = _pure("bempty2", _du.bbox_is_empty)
    assert got is False and want is False


def test_g213_join_rects():
    from pdfluent_extract.helpers import utils as _du

    args, _, _ = probe_args("pure/probes.json", "join")
    got = _du.join_rects(*args)
    want = [
        read_json("final/join-x0.json"),
        read_json("final/join-y0.json"),
        read_json("final/join-x1.json"),
        read_json("final/join-y1.json"),
    ]
    assert_data_equal([got[0], got[1], got[2], got[3]], want)


def test_g214_expand_bbox():
    from pdfluent_extract.helpers import utils as _du

    got, want = _pure("expand", _du.expand_bbox_by_points)
    assert_data_equal(got, want)
    assert got == [0, 0, 9, 9]


def test_g215_wrap_table():
    got, want = _pure("wrap", dl.wrap_table_for_tabulate)
    assert_data_equal(got, want)
    got, want = _pure("wrap2", dl.wrap_table_for_tabulate)
    assert_data_equal(got, want)


def test_g216_text_lines_default():
    with _page("multi-page.pdf") as page:
        assert gtl.get_text_lines(page) == _read_text("unit/gtl-default.json")


def test_g217_text_lines_ruled():
    with _page("ruled-table.pdf") as page:
        assert gtl.get_text_lines(page) == _read_text("final/gtl-ruled-def.json")
    with _page("ruled-table.pdf") as page:
        assert gtl.get_text_lines(page, sep="|") == _read_text("unit/gtl-sep.json")


def test_g218_text_lines_tolerance():
    with _page("multi-page.pdf") as page:
        assert gtl.get_text_lines(page, tolerance=10) == _read_text("unit/gtl-tol.json")
    with _page("multi-page.pdf") as page:
        default = gtl.get_text_lines(page)
    with _page("multi-page.pdf") as page:
        assert gtl.get_text_lines(page, tolerance=10) == default
