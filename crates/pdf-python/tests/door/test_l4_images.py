# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Lane L4: SPEC sections 7, 8, 9. Flip markers as sections land."""

import json
import os
import struct

import pytest
from conftest import FIX, RES, SKIP_REASON, SPEC_MISSING, assert_data_equal, read_json
from conftest import read_text as _read_text

import pdfluent_extract as pe

pytestmark = [
    pytest.mark.skipif(SPEC_MISSING, reason=SKIP_REASON),
]

lane_l4 = pytest.mark.xfail(strict=True, reason="lane L4")


def _legacy():
    pe.use_layout(False)


def _written_names(folder):
    return sorted(
        name
        for name in os.listdir(folder)
        if os.path.isfile(os.path.join(folder, name))
    )


def _png_size(path):
    with open(path, "rb") as handle:
        data = handle.read(26)
    return struct.unpack(">II", data[16:24])


@lane_l4
def test_g66_layout_write_inline(tmp_path):
    dest = str(tmp_path) + "/"
    out = pe.to_markdown(str(FIX("images.pdf")), write_images=True, image_path=dest)
    names = _written_names(dest)
    assert names == ["images.pdf-0001-01.png", "images.pdf-0001-02.png"]
    first = out.index("![](%s%s)" % (dest, names[0]))
    second = out.index("![](%s%s)" % (dest, names[1]))
    assert out.index("Two pictures follow.") < first < second
    assert second < out.index("Caption under the pictures.")


@lane_l4
def test_g67_legacy_write_after_text(tmp_path):
    _legacy()
    dest = str(tmp_path) + "/"
    out = pe.to_markdown(str(FIX("images.pdf")), write_images=True, image_path=dest)
    names = _written_names(dest)
    assert names == ["images.pdf-0-0.png", "images.pdf-0-1.png"]
    refs = [out.index("![](%s%s)" % (dest, n)) for n in names]
    assert out.index("Caption under the pictures.") < min(refs)


@lane_l4
def test_g68_layout_names(tmp_path):
    dest = str(tmp_path) + "/"
    pe.to_markdown(str(FIX("images.pdf")), write_images=True, image_path=dest)
    assert _written_names(dest) == ["images.pdf-0001-01.png", "images.pdf-0001-02.png"]
    assert _written_names(os.path.join(RES("images/layout-w"))) == [
        "images.pdf-0001-01.png",
        "images.pdf-0001-02.png",
    ]


@lane_l4
def test_g69_legacy_names(tmp_path):
    _legacy()
    dest = str(tmp_path) + "/"
    pe.to_markdown(str(FIX("images.pdf")), write_images=True, image_path=dest)
    assert _written_names(dest) == ["images.pdf-0-0.png", "images.pdf-0-1.png"]


@lane_l4
def test_g70_legacy_filename(tmp_path):
    _legacy()
    dest = str(tmp_path) + "/"
    pe.to_markdown(
        str(FIX("images.pdf")),
        write_images=True,
        image_path=dest,
        filename="custom.pdf",
    )
    assert _written_names(dest) == ["custom.pdf-0-0.png", "custom.pdf-0-1.png"]


@lane_l4
def test_g71_jpg_format(tmp_path):
    dest = str(tmp_path) + "/"
    pe.to_markdown(
        str(FIX("images.pdf")), write_images=True, image_path=dest, image_format="jpg"
    )
    assert _written_names(dest) == ["images.pdf-0001-01.jpg", "images.pdf-0001-02.jpg"]
    _legacy()
    dest = str(tmp_path / "leg") + "/"
    os.mkdir(str(tmp_path / "leg"))
    pe.to_markdown(
        str(FIX("images.pdf")), write_images=True, image_path=dest, image_format="jpg"
    )
    assert _written_names(dest) == ["images.pdf-0-0.jpg", "images.pdf-0-1.jpg"]


@lane_l4
def test_g72_bogus_format_ignored():
    assert pe.to_markdown(str(FIX("images.pdf")), image_format="zzz") == _read_text(
        "layout-params/imgfmt-bogus.md"
    )


@lane_l4
def test_g73_embed_both():
    out = pe.to_markdown(str(FIX("images.pdf")), embed_images=True)
    assert out == _read_text("images/layout-embed.md")
    assert "![](data:image/png;base64," in out
    _legacy()
    out = pe.to_markdown(str(FIX("images.pdf")), embed_images=True)
    assert out == _read_text("images/legacy-embed.md")
    assert "![](data:image/png;base64," in out


@lane_l4
def test_g74_json_embed_raw():
    got = json.loads(pe.to_json(str(FIX("images.pdf")), embed_images=True))
    assert_data_equal(got, read_json("images/layout-json-embed.json"))
    for page in got["pages"]:
        for box in page["boxes"]:
            if box["boxclass"] == "picture":
                assert box["image"] and "data:" not in box["image"]


@lane_l4
def test_g75_json_write_paths(tmp_path):
    dest = str(tmp_path) + "/"
    got = json.loads(
        pe.to_json(str(FIX("images.pdf")), write_images=True, image_path=dest)
    )
    want = read_json("images/layout-json-write.json")
    for page in want["pages"]:
        for box in page["boxes"]:
            if box["boxclass"] == "picture" and box["image"]:
                name = box["image"].rsplit("/", 1)[-1]
                box["image"] = dest + name
    assert_data_equal(got, want)


@lane_l4
def test_g76_json_no_images_null():
    got = json.loads(pe.to_json(str(FIX("images.pdf"))))
    assert_data_equal(got, read_json("basics/json-images.json"))
    for page in got["pages"]:
        for box in page["boxes"]:
            if box["boxclass"] == "picture":
                assert box["image"] is None


def test_g77_dpi_no_effect(tmp_path):
    low = str(tmp_path / "lo") + "/"
    high = str(tmp_path / "hi") + "/"
    os.mkdir(str(tmp_path / "lo"))
    os.mkdir(str(tmp_path / "hi"))
    pe.to_markdown(
        str(FIX("images.pdf")), write_images=True, image_path=low, image_dpi=72
    )
    pe.to_markdown(
        str(FIX("images.pdf")), write_images=True, image_path=high, image_dpi=300
    )
    assert _written_names(low) == _written_names(high)
    for name in _written_names(low):
        with open(os.path.join(low, name), "rb") as a, open(
            os.path.join(high, name), "rb"
        ) as b:
            assert a.read() == b.read()
        with open(os.path.join(RES("images/layout-dpi72"), name), "rb") as rec:
            with open(os.path.join(low, name), "rb") as ours:
                assert ours.read() == rec.read()


@lane_l4
def test_g78_legacy_ignore_wins(tmp_path):
    _legacy()
    dest = str(tmp_path) + "/"
    out = pe.to_markdown(
        str(FIX("images.pdf")), ignore_images=True, write_images=True, image_path=dest
    )
    assert out == _read_text("images/legacy-ignore-write.md")
    assert _written_names(dest) == [] and "![]( " not in out and ".png)" not in out


@lane_l4
def test_g79_limit_zero_keeps_all(tmp_path):
    _legacy()
    dest = str(tmp_path) + "/"
    pe.to_markdown(
        str(FIX("tiny-images.pdf")),
        write_images=True,
        image_path=dest,
        image_size_limit=0,
    )
    assert _written_names(dest) == [
        "tiny-images.pdf-0-0.png",
        "tiny-images.pdf-0-1.png",
    ]


@lane_l4
def test_g80_limit_default_drops_small(tmp_path):
    _legacy()
    dest = str(tmp_path) + "/"
    pe.to_markdown(str(FIX("tiny-images.pdf")), write_images=True, image_path=dest)
    assert _written_names(dest) == ["tiny-images.pdf-0-0.png"]


def test_g81_limit_high_keeps_none(tmp_path):
    _legacy()
    for limit in (0.5, 0.99):
        dest = str(tmp_path / str(limit)) + "/"
        os.mkdir(str(tmp_path / str(limit)))
        pe.to_markdown(
            str(FIX("tiny-images.pdf")),
            write_images=True,
            image_path=dest,
            image_size_limit=limit,
        )
        assert _written_names(dest) == []
    with pytest.raises(ValueError):
        pe.to_markdown(
            str(FIX("tiny-images.pdf")),
            write_images=True,
            image_path=str(tmp_path) + "/",
            image_size_limit=1,
        )


@lane_l4
def test_g82_shape_sweep(tmp_path):
    _legacy()
    expectations = {0.02: 2, 0.03: 1, 0.034: 0, 0.1: 0}
    for limit, count in expectations.items():
        dest = str(tmp_path / str(limit)) + "/"
        os.mkdir(str(tmp_path / str(limit)))
        pe.to_markdown(
            str(FIX("shape-images.pdf")),
            write_images=True,
            image_path=dest,
            image_size_limit=limit,
        )
        assert len(_written_names(dest)) == count
    assert _png_size(
        os.path.join(str(tmp_path / "0.02"), "shape-images.pdf-0-0.png")
    ) == (
        834,
        42,
    )
    assert _png_size(
        os.path.join(str(tmp_path / "0.02"), "shape-images.pdf-0-1.png")
    ) == (
        42,
        834,
    )
    assert _png_size(
        os.path.join(str(tmp_path / "0.03"), "shape-images.pdf-0-0.png")
    ) == (
        42,
        834,
    )


@lane_l4
def test_g83_kept_index_restart(tmp_path):
    _legacy()
    dest = str(tmp_path) + "/"
    pe.to_markdown(str(FIX("tiny-images.pdf")), write_images=True, image_path=dest)
    assert _written_names(dest) == ["tiny-images.pdf-0-0.png"]
    dest = str(tmp_path / "shape") + "/"
    os.mkdir(str(tmp_path / "shape"))
    pe.to_markdown(
        str(FIX("shape-images.pdf")),
        write_images=True,
        image_path=dest,
        image_size_limit=0.03,
    )
    assert _written_names(dest) == ["shape-images.pdf-0-0.png"]


@lane_l4
def test_g84_png_pixel_scale(tmp_path):
    _legacy()
    dest = str(tmp_path) + "/"
    pe.to_markdown(
        str(FIX("shape-images.pdf")),
        write_images=True,
        image_path=dest,
        image_size_limit=0.02,
    )
    assert _png_size(os.path.join(dest, "shape-images.pdf-0-0.png")) == (834, 42)
    assert _png_size(os.path.join(dest, "shape-images.pdf-0-1.png")) == (42, 834)


def test_g85_graphics_caption_legacy():
    _legacy()
    path = str(FIX("vector-graphics.pdf"))
    assert pe.to_markdown(path, ignore_graphics=True) == _read_text(
        "legacy-params/ignore-graphics-t.md"
    )
    assert pe.to_markdown(path, ignore_graphics=False) == _read_text(
        "legacy-params/ignore-graphics-f.md"
    )


@lane_l4
def test_g86_graphics_gap_layout():
    assert pe.to_markdown(str(FIX("vector-graphics.pdf"))) == _read_text(
        "misc2/vec-layout.md"
    )


@lane_l4
def test_g87_graphics_limit_ignored():
    _legacy()
    vec = str(FIX("vector-graphics.pdf"))
    vec_outs = [pe.to_markdown(vec, graphics_limit=n) for n in (0, 10, 100, None)]
    assert vec_outs[0] == _read_text("legacy-params/graphicslimit-0-vec.md")
    assert vec_outs[1] == _read_text("legacy-params/graphicslimit-10-vec.md")
    assert vec_outs[2] == _read_text("legacy-params/graphicslimit-100-vec.md")
    assert vec_outs[3] == _read_text("legacy-params/graphicslimit-none.md")
    assert vec_outs[0] == vec_outs[1] == vec_outs[2] == vec_outs[3]
    glp = str(FIX("graphics-limit-pages.pdf"))
    glp_outs = [pe.to_markdown(glp, graphics_limit=n) for n in (0, 10, 100)]
    assert glp_outs[0] == _read_text("legacy-params/graphicslimit-0-glp.md")
    assert glp_outs[1] == _read_text("legacy-params/graphicslimit-10-glp.md")
    assert glp_outs[2] == _read_text("legacy-params/graphicslimit-100-glp.md")
    assert glp_outs[0] == glp_outs[1] == glp_outs[2]


def test_g88_graphics_limit_negative():
    _legacy()
    assert pe.to_markdown(
        str(FIX("vector-graphics.pdf")), graphics_limit=-1
    ) == _read_text("errors/leg-graphics-neg.md")


def test_g89_bg_color_no_effect():
    _legacy()
    path = str(FIX("shaded-table.pdf"))
    assert pe.to_markdown(path, detect_bg_color=True) == ""
    assert pe.to_markdown(path, detect_bg_color=False) == ""
    assert pe.to_markdown(path, detect_bg_color=True) == _read_text(
        "legacy-params/detectbg-t.md"
    )
    assert pe.to_markdown(path, detect_bg_color=False) == _read_text(
        "legacy-params/detectbg-f.md"
    )


def test_g90_over_picture_dropped():
    _legacy()
    assert pe.to_markdown(
        str(FIX("text-over-image.pdf")), force_text=True
    ) == _read_text("legacy-params/forcetext-True-toi.md")


def test_g91_force_text_required():
    _legacy()
    with pytest.raises(ValueError):
        pe.to_markdown(str(FIX("text-over-image.pdf")), force_text=False)
    with pytest.raises(ValueError):
        pe.to_markdown(str(FIX("scanned-page.pdf")), force_text=False)


@lane_l4
def test_g92_force_text_ignored_layout():
    path = str(FIX("text-over-image.pdf"))
    out_t = pe.to_markdown(path, force_text=True)
    out_f = pe.to_markdown(path, force_text=False)
    assert out_t == _read_text("layout-params/forcetext-t.md")
    assert out_f == _read_text("layout-params/forcetext-f.md")
    assert out_t == out_f


def test_g93_scan_page_marks():
    assert pe.to_markdown(str(FIX("scanned-page.pdf"))) == "\n\n"
    assert pe.to_markdown(str(FIX("scanned-page.pdf"))) == _read_text(
        "layout-params/forcetext-scan-t.md"
    )
    _legacy()
    assert pe.to_markdown(str(FIX("scanned-page.pdf"))) == ""
    assert pe.to_markdown(str(FIX("scanned-page.pdf"))) == _read_text(
        "legacy-params/forcetext-True-scan.md"
    )
