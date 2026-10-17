# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Compatible module entry points (lane L5): every name this lane registers in _native.

The lane appends each new name to _COMPAT_NAMES; ``pdfluent.__init__``
re-exports the combined list in fixed lane order.
"""

from __future__ import annotations

_COMPAT_NAMES: list[str] = [
    # Helper classes.
    "PdfObj",
    "ElementPosition",
    # Validators.
    "CheckColor",
    "CheckMarkerArg",
    "CheckMorph",
    "CheckParent",
    "CheckQuad",
    "CheckRect",
    "ColorCode",
    "ConversionHeader",
    "ConversionTrailer",
    # Boxes.
    "EMPTY_IRECT",
    "EMPTY_QUAD",
    "EMPTY_RECT",
    "INFINITE_IRECT",
    "INFINITE_QUAD",
    "INFINITE_RECT",
    # Names and destinations.
    "PDF_NAME",
    "dest_is_valid",
    "dest_is_valid_page",
    "pdfobj_string",
    # Assertions.
    "ASSERT_PDF",
    "ENSURE_OPERATION",
    "RAISEPY",
    # Native-only doors (recorded raises).
    "hdist",
    "vdist",
    "compute_scissor",
    "detect_super_script",
    "pdf_lookup_page_loc",
    "string_in_names_list",
    "page_merge",
    "extra_FzDocument_insert_pdf",
    "strip_outline",
    "strip_outlines",
    # Strings.
    "find_string",
    "match_string",
    "format_g",
    "get_pdf_now",
    "get_pdf_str",
    "getTJstr",
    "make_escape",
    "make_story_elpos",
    "no_recommend_layout",
    "planish_line",
    "restore_aliases",
    # Environment and paper.
    "get_env_bool",
    "get_env_int",
    "paper_size",
    "paper_rect",
    "paper_sizes",
    # Colours.
    "sRGB_to_pdf",
    "sRGB_to_rgb",
    "colors_pdf_dict",
    "colors_wx_list",
    # Logging.
    "message",
    "message_warning",
    "log",
    "set_log",
    "set_messages",
    # Sequence helpers.
    "PySequence_Check",
    "PySequence_Size",
    "PyUnicode_DecodeRawUnicodeEscape",
    # Introspection.
    "dir_str",
    "exception_info",
    "args_match",
    # Document operations.
    "get_highlight_selection",
    "apply_pages",
    "annot_preprocess",
    "annot_postprocess",
    # Geometry utilities.
    "util_ensure_widget_calc",
    "util_concat_matrix",
    "util_hor_matrix",
    "util_include_point_in_rect",
    "util_intersect_rect",
    "util_invert_matrix",
    "util_is_point_in_rect",
    "util_make_irect",
    "util_make_rect",
    "util_measure_string",
    "util_point_in_quad",
    "util_round_rect",
    "util_sine_between",
    "util_transform_point",
    "util_transform_rect",
    "util_union_rect",
]
