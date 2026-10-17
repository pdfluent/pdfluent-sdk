//! Recorded module constants: flags, codes, names, and tables.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Every value below is pinned data from the recorded `l2-constants-all`
//! expectation: names stay verbatim (hence the case-pattern allowance),
//! grouped by purpose. Groups that map onto a lane type use it
//! ([`Colorspace`](super::pixmap::Colorspace) presets, the
//! [`Matrix`](super::geometry::Matrix) identity); the rest are plain
//! integers, floats, strings, and slices.
#![allow(non_upper_case_globals)]

use super::geometry::Matrix;
use super::pixmap::Colorspace;

// Geometry bounds and the identity matrix (5).
/// Small float threshold.
pub const EPSILON: f64 = 1e-05;
/// Small float threshold.
pub const FLT_EPSILON: f64 = 1e-05;
/// Infinite-rectangle bound.
pub const FZ_MAX_INF_RECT: i32 = 2147483520;
/// Infinite-rectangle bound.
pub const FZ_MIN_INF_RECT: i32 = -2147483648;
/// Identity matrix.
pub const Identity: Matrix = Matrix {
    a: 1.0,
    b: 0.0,
    c: 0.0,
    d: 1.0,
    e: 0.0,
    f: 0.0,
};

// Text extraction flags (42).
/// Text extraction flag.
pub const TEXT_ACCURATE_ASCENDERS: i32 = 262144;
/// Text extraction flag.
pub const TEXT_ACCURATE_BBOXES: i32 = 512;
/// Text extraction flag.
pub const TEXT_ACCURATE_SIDE_BEARINGS: i32 = 524288;
/// Text extraction flag.
pub const TEXT_ALIGN_CENTER: i32 = 1;
/// Text extraction flag.
pub const TEXT_ALIGN_JUSTIFY: i32 = 3;
/// Text extraction flag.
pub const TEXT_ALIGN_LEFT: i32 = 0;
/// Text extraction flag.
pub const TEXT_ALIGN_RIGHT: i32 = 2;
/// Text extraction flag.
pub const TEXT_CID_FOR_UNKNOWN_UNICODE: i32 = 128;
/// Text extraction flag.
pub const TEXT_CLIP: i32 = 64;
/// Text extraction flag.
pub const TEXT_CLIP_RECT: i32 = 131072;
/// Text extraction flag.
pub const TEXT_COLLECT_STRUCTURE: i32 = 256;
/// Text extraction flag.
pub const TEXT_COLLECT_STYLES: i32 = 32768;
/// Text extraction flag.
pub const TEXT_COLLECT_VECTORS: i32 = 1024;
/// Text extraction flag.
pub const TEXT_DEHYPHENATE: i32 = 16;
/// Text extraction flag.
pub const TEXT_ENCODING_CYRILLIC: i32 = 2;
/// Text extraction flag.
pub const TEXT_ENCODING_GREEK: i32 = 1;
/// Text extraction flag.
pub const TEXT_ENCODING_LATIN: i32 = 0;
/// Text extraction flag.
pub const TEXT_FONT_BOLD: i32 = 16;
/// Text extraction flag.
pub const TEXT_FONT_ITALIC: i32 = 2;
/// Text extraction flag.
pub const TEXT_FONT_MONOSPACED: i32 = 8;
/// Text extraction flag.
pub const TEXT_FONT_SERIFED: i32 = 4;
/// Text extraction flag.
pub const TEXT_FONT_SUPERSCRIPT: i32 = 1;
/// Text extraction flag.
pub const TEXT_FUZZY_VECTORS: i32 = 2097152;
/// Text extraction flag.
pub const TEXT_IGNORE_ACTUALTEXT: i32 = 2048;
/// Text extraction flag.
pub const TEXT_INHIBIT_SPACES: i32 = 8;
/// Text extraction flag.
pub const TEXT_LAZY_VECTORS: i32 = 1048576;
/// Text extraction flag.
pub const TEXT_MEDIABOX_CLIP: i32 = 64;
/// Text extraction flag.
pub const TEXT_OUTPUT_HTML: i32 = 1;
/// Text extraction flag.
pub const TEXT_OUTPUT_JSON: i32 = 2;
/// Text extraction flag.
pub const TEXT_OUTPUT_TEXT: i32 = 0;
/// Text extraction flag.
pub const TEXT_OUTPUT_XHTML: i32 = 4;
/// Text extraction flag.
pub const TEXT_OUTPUT_XML: i32 = 3;
/// Text extraction flag.
pub const TEXT_PARAGRAPH_BREAK: i32 = 8192;
/// Text extraction flag.
pub const TEXT_PRESERVE_IMAGES: i32 = 4;
/// Text extraction flag.
pub const TEXT_PRESERVE_LIGATURES: i32 = 1;
/// Text extraction flag.
pub const TEXT_PRESERVE_SPANS: i32 = 32;
/// Text extraction flag.
pub const TEXT_PRESERVE_WHITESPACE: i32 = 2;
/// Text extraction flag.
pub const TEXT_SEGMENT: i32 = 4096;
/// Text extraction flag.
pub const TEXT_STEXT_SEGMENT: i32 = 4096;
/// Text extraction flag.
pub const TEXT_TABLE_HUNT: i32 = 16384;
/// Text extraction flag.
pub const TEXT_USE_CID_FOR_UNKNOWN_UNICODE: i32 = 128;
/// Text extraction flag.
pub const TEXT_USE_GID_FOR_UNKNOWN_UNICODE: i32 = 65536;

// Text flag presets (9).
/// Text flag preset.
pub const TEXTFLAGS_BLOCKS: i32 = 195;
/// Text flag preset.
pub const TEXTFLAGS_DICT: i32 = 199;
/// Text flag preset.
pub const TEXTFLAGS_HTML: i32 = 199;
/// Text flag preset.
pub const TEXTFLAGS_RAWDICT: i32 = 199;
/// Text flag preset.
pub const TEXTFLAGS_SEARCH: i32 = 210;
/// Text flag preset.
pub const TEXTFLAGS_TEXT: i32 = 195;
/// Text flag preset.
pub const TEXTFLAGS_WORDS: i32 = 195;
/// Text flag preset.
pub const TEXTFLAGS_XHTML: i32 = 199;
/// Text flag preset.
pub const TEXTFLAGS_XML: i32 = 195;

// Structured-text dict keys (53).
/// Structured-text key.
pub const dictkey_align: &str = "align";
/// Structured-text key.
pub const dictkey_asc: &str = "ascender";
/// Structured-text key.
pub const dictkey_bbox: &str = "bbox";
/// Structured-text key.
pub const dictkey_bidi: &str = "bidi";
/// Structured-text key.
pub const dictkey_blocks: &str = "blocks";
/// Structured-text key.
pub const dictkey_bpc: &str = "bpc";
/// Structured-text key.
pub const dictkey_c: &str = "c";
/// Structured-text key.
pub const dictkey_char_flags: &str = "char_flags";
/// Structured-text key.
pub const dictkey_chars: &str = "chars";
/// Structured-text key.
pub const dictkey_color: &str = "color";
/// Structured-text key.
pub const dictkey_colorspace: &str = "colorspace";
/// Structured-text key.
pub const dictkey_content: &str = "content";
/// Structured-text key.
pub const dictkey_creationDate: &str = "creationDate";
/// Structured-text key.
pub const dictkey_cs_name: &str = "cs-name";
/// Structured-text key.
pub const dictkey_da: &str = "da";
/// Structured-text key.
pub const dictkey_dashes: &str = "dashes";
/// Structured-text key.
pub const dictkey_desc: &str = "descender";
/// Structured-text key.
pub const dictkey_descr: &str = "description";
/// Structured-text key.
pub const dictkey_dir: &str = "dir";
/// Structured-text key.
pub const dictkey_effect: &str = "effect";
/// Structured-text key.
pub const dictkey_ext: &str = "ext";
/// Structured-text key.
pub const dictkey_filename: &str = "filename";
/// Structured-text key.
pub const dictkey_fill: &str = "fill";
/// Structured-text key.
pub const dictkey_flags: &str = "flags";
/// Structured-text key.
pub const dictkey_font: &str = "font";
/// Structured-text key.
pub const dictkey_glyph: &str = "glyph";
/// Structured-text key.
pub const dictkey_height: &str = "height";
/// Structured-text key.
pub const dictkey_id: &str = "id";
/// Structured-text key.
pub const dictkey_image: &str = "image";
/// Structured-text key.
pub const dictkey_items: &str = "items";
/// Structured-text key.
pub const dictkey_length: &str = "length";
/// Structured-text key.
pub const dictkey_lines: &str = "lines";
/// Structured-text key.
pub const dictkey_matrix: &str = "transform";
/// Structured-text key.
pub const dictkey_modDate: &str = "modDate";
/// Structured-text key.
pub const dictkey_name: &str = "name";
/// Structured-text key.
pub const dictkey_number: &str = "number";
/// Structured-text key.
pub const dictkey_origin: &str = "origin";
/// Structured-text key.
pub const dictkey_rect: &str = "rect";
/// Structured-text key.
pub const dictkey_size: &str = "size";
/// Structured-text key.
pub const dictkey_smask: &str = "smask";
/// Structured-text key.
pub const dictkey_spans: &str = "spans";
/// Structured-text key.
pub const dictkey_stroke: &str = "stroke";
/// Structured-text key.
pub const dictkey_style: &str = "style";
/// Structured-text key.
pub const dictkey_subject: &str = "subject";
/// Structured-text key.
pub const dictkey_text: &str = "text";
/// Structured-text key.
pub const dictkey_title: &str = "title";
/// Structured-text key.
pub const dictkey_type: &str = "type";
/// Structured-text key.
pub const dictkey_ufilename: &str = "ufilename";
/// Structured-text key.
pub const dictkey_width: &str = "width";
/// Structured-text key.
pub const dictkey_wmode: &str = "wmode";
/// Structured-text key.
pub const dictkey_xref: &str = "xref";
/// Structured-text key.
pub const dictkey_xres: &str = "xres";
/// Structured-text key.
pub const dictkey_yres: &str = "yres";

// Structure-tree marks (8).
/// Structure-tree mark.
pub const PDF_STRUCT_BROKEN: i32 = 2;
/// Structure-tree mark.
pub const PDF_STRUCT_FIXED: i32 = 4;
/// Structure-tree mark.
pub const PDF_STRUCT_HAS_ATTRIBUTES: i32 = 8;
/// Structure-tree mark.
pub const PDF_STRUCT_HAS_CYCLE: i32 = 64;
/// Structure-tree mark.
pub const PDF_STRUCT_HAS_TABLE_ATTRIBUTES: i32 = 16;
/// Structure-tree mark.
pub const PDF_STRUCT_HAS_TABLE_SPAN_ATTRIBUTES: i32 = 32;
/// Structure-tree mark.
pub const PDF_STRUCT_NOT_PRESENT: i32 = 0;
/// Structure-tree mark.
pub const PDF_STRUCT_PRESENT: i32 = 1;

// Link kinds and flags (13).
/// Link kind or flag.
pub const LINK_FLAG_B_VALID: i32 = 8;
/// Link kind or flag.
pub const LINK_FLAG_FIT_H: i32 = 16;
/// Link kind or flag.
pub const LINK_FLAG_FIT_V: i32 = 32;
/// Link kind or flag.
pub const LINK_FLAG_L_VALID: i32 = 1;
/// Link kind or flag.
pub const LINK_FLAG_R_IS_ZOOM: i32 = 64;
/// Link kind or flag.
pub const LINK_FLAG_R_VALID: i32 = 4;
/// Link kind or flag.
pub const LINK_FLAG_T_VALID: i32 = 2;
/// Link kind or flag.
pub const LINK_GOTO: i32 = 1;
/// Link kind or flag.
pub const LINK_GOTOR: i32 = 5;
/// Link kind or flag.
pub const LINK_LAUNCH: i32 = 3;
/// Link kind or flag.
pub const LINK_NAMED: i32 = 4;
/// Link kind or flag.
pub const LINK_NONE: i32 = 0;
/// Link kind or flag.
pub const LINK_URI: i32 = 2;

// Encryption methods (7).
/// Encryption method.
pub const PDF_ENCRYPT_AES_128: i32 = 4;
/// Encryption method.
pub const PDF_ENCRYPT_AES_256: i32 = 5;
/// Encryption method.
pub const PDF_ENCRYPT_KEEP: i32 = 0;
/// Encryption method.
pub const PDF_ENCRYPT_NONE: i32 = 1;
/// Encryption method.
pub const PDF_ENCRYPT_RC4_128: i32 = 3;
/// Encryption method.
pub const PDF_ENCRYPT_RC4_40: i32 = 2;
/// Encryption method.
pub const PDF_ENCRYPT_UNKNOWN: i32 = 6;

// Permission bits (8).
/// Permission bit.
pub const PDF_PERM_ACCESSIBILITY: i32 = 512;
/// Permission bit.
pub const PDF_PERM_ANNOTATE: i32 = 32;
/// Permission bit.
pub const PDF_PERM_ASSEMBLE: i32 = 1024;
/// Permission bit.
pub const PDF_PERM_COPY: i32 = 16;
/// Permission bit.
pub const PDF_PERM_FORM: i32 = 256;
/// Permission bit.
pub const PDF_PERM_MODIFY: i32 = 8;
/// Permission bit.
pub const PDF_PERM_PRINT: i32 = 4;
/// Permission bit.
pub const PDF_PERM_PRINT_HQ: i32 = 2048;

// Page label styles (6).
/// Page label style.
pub const PDF_PAGE_LABEL_ALPHA_LC: &str = "a";
/// Page label style.
pub const PDF_PAGE_LABEL_ALPHA_UC: &str = "A";
/// Page label style.
pub const PDF_PAGE_LABEL_DECIMAL: &str = "D";
/// Page label style.
pub const PDF_PAGE_LABEL_NONE: i32 = 0;
/// Page label style.
pub const PDF_PAGE_LABEL_ROMAN_LC: &str = "r";
/// Page label style.
pub const PDF_PAGE_LABEL_ROMAN_UC: &str = "R";

// Signatures (18).
/// Signature value.
pub const PDF_SIGNATURE_DEFAULT_APPEARANCE: i32 = 63;
/// Signature value.
pub const PDF_SIGNATURE_ERROR_DIGEST_FAILURE: i32 = 3;
/// Signature value.
pub const PDF_SIGNATURE_ERROR_NOT_SIGNED: i32 = 7;
/// Signature value.
pub const PDF_SIGNATURE_ERROR_NOT_TRUSTED: i32 = 6;
/// Signature value.
pub const PDF_SIGNATURE_ERROR_NO_CERTIFICATE: i32 = 2;
/// Signature value.
pub const PDF_SIGNATURE_ERROR_NO_SIGNATURES: i32 = 1;
/// Signature value.
pub const PDF_SIGNATURE_ERROR_OKAY: i32 = 0;
/// Signature value.
pub const PDF_SIGNATURE_ERROR_SELF_SIGNED: i32 = 4;
/// Signature value.
pub const PDF_SIGNATURE_ERROR_SELF_SIGNED_IN_CHAIN: i32 = 5;
/// Signature value.
pub const PDF_SIGNATURE_ERROR_UNKNOWN: i32 = 8;
/// Signature value.
pub const PDF_SIGNATURE_SHOW_DATE: i32 = 4;
/// Signature value.
pub const PDF_SIGNATURE_SHOW_DN: i32 = 2;
/// Signature value.
pub const PDF_SIGNATURE_SHOW_GRAPHIC_NAME: i32 = 16;
/// Signature value.
pub const PDF_SIGNATURE_SHOW_LABELS: i32 = 1;
/// Signature value.
pub const PDF_SIGNATURE_SHOW_LOGO: i32 = 32;
/// Signature value.
pub const PDF_SIGNATURE_SHOW_TEXT_NAME: i32 = 8;
/// Signature value.
pub const SigFlag_AppendOnly: i32 = 2;
/// Signature value.
pub const SigFlag_SignaturesExist: i32 = 1;

// Alerts and document events (19).
/// Alert or event value.
pub const PDF_ALERT_BUTTON_CANCEL: i32 = 2;
/// Alert or event value.
pub const PDF_ALERT_BUTTON_GROUP_OK: i32 = 0;
/// Alert or event value.
pub const PDF_ALERT_BUTTON_GROUP_OK_CANCEL: i32 = 1;
/// Alert or event value.
pub const PDF_ALERT_BUTTON_GROUP_YES_NO: i32 = 2;
/// Alert or event value.
pub const PDF_ALERT_BUTTON_GROUP_YES_NO_CANCEL: i32 = 3;
/// Alert or event value.
pub const PDF_ALERT_BUTTON_NO: i32 = 3;
/// Alert or event value.
pub const PDF_ALERT_BUTTON_NONE: i32 = 0;
/// Alert or event value.
pub const PDF_ALERT_BUTTON_OK: i32 = 1;
/// Alert or event value.
pub const PDF_ALERT_BUTTON_YES: i32 = 4;
/// Alert or event value.
pub const PDF_ALERT_ICON_ERROR: i32 = 0;
/// Alert or event value.
pub const PDF_ALERT_ICON_QUESTION: i32 = 2;
/// Alert or event value.
pub const PDF_ALERT_ICON_STATUS: i32 = 3;
/// Alert or event value.
pub const PDF_ALERT_ICON_WARNING: i32 = 1;
/// Alert or event value.
pub const PDF_DOCUMENT_EVENT_ALERT: i32 = 0;
/// Alert or event value.
pub const PDF_DOCUMENT_EVENT_EXEC_MENU_ITEM: i32 = 5;
/// Alert or event value.
pub const PDF_DOCUMENT_EVENT_LAUNCH_URL: i32 = 2;
/// Alert or event value.
pub const PDF_DOCUMENT_EVENT_MAIL_DOC: i32 = 3;
/// Alert or event value.
pub const PDF_DOCUMENT_EVENT_PRINT: i32 = 1;
/// Alert or event value.
pub const PDF_DOCUMENT_EVENT_SUBMIT: i32 = 4;

// Layers and optional content (6).
/// Layer value.
pub const PDF_LAYER_UI_CHECKBOX: i32 = 1;
/// Layer value.
pub const PDF_LAYER_UI_LABEL: i32 = 0;
/// Layer value.
pub const PDF_LAYER_UI_RADIOBOX: i32 = 2;
/// Layer value.
pub const PDF_OC_OFF: i32 = 2;
/// Layer value.
pub const PDF_OC_ON: i32 = 0;
/// Layer value.
pub const PDF_OC_TOGGLE: i32 = 1;

// Electronic invoicing marks (8).
/// E-invoicing mark.
pub const PDF_NOT_ZUGFERD: i32 = 0;
/// E-invoicing mark.
pub const PDF_ZUGFERD_BASIC: i32 = 2;
/// E-invoicing mark.
pub const PDF_ZUGFERD_BASIC_WL: i32 = 4;
/// E-invoicing mark.
pub const PDF_ZUGFERD_COMFORT: i32 = 1;
/// E-invoicing mark.
pub const PDF_ZUGFERD_EXTENDED: i32 = 3;
/// E-invoicing mark.
pub const PDF_ZUGFERD_MINIMUM: i32 = 5;
/// E-invoicing mark.
pub const PDF_ZUGFERD_UNKNOWN: i32 = 7;
/// E-invoicing mark.
pub const PDF_ZUGFERD_XRECHNUNG: i32 = 6;

// Document cleanup (4).
/// Cleanup value.
pub const PDF_CLEAN_STRUCTURE_DROP: i32 = 0;
/// Cleanup value.
pub const PDF_CLEAN_STRUCTURE_KEEP: i32 = 1;
/// Cleanup value.
pub const PDF_CLEAN_VECTORIZE_NO: i32 = 0;
/// Cleanup value.
pub const PDF_CLEAN_VECTORIZE_YES: i32 = 1;

// Colour-space presets (6).
/// Colour-space preset.
pub const CS_CMYK: i32 = 3;
/// Colour-space preset.
pub const CS_GRAY: i32 = 2;
/// Colour-space preset.
pub const CS_RGB: i32 = 1;
/// Colour-space preset.
pub const csCMYK: Colorspace = Colorspace::Cmyk;
/// Colour-space preset.
pub const csGRAY: Colorspace = Colorspace::Gray;
/// Colour-space preset.
pub const csRGB: Colorspace = Colorspace::Rgb;

// Image recompression (8).
/// Recompression value.
pub const FZ_RECOMPRESS_FAX: i32 = 5;
/// Recompression value.
pub const FZ_RECOMPRESS_J2K: i32 = 4;
/// Recompression value.
pub const FZ_RECOMPRESS_JPEG: i32 = 3;
/// Recompression value.
pub const FZ_RECOMPRESS_LOSSLESS: i32 = 2;
/// Recompression value.
pub const FZ_RECOMPRESS_NEVER: i32 = 0;
/// Recompression value.
pub const FZ_RECOMPRESS_SAME: i32 = 1;
/// Recompression value.
pub const FZ_SUBSAMPLE_AVERAGE: i32 = 0;
/// Recompression value.
pub const FZ_SUBSAMPLE_BICUBIC: i32 = 1;

// Annotation kinds, flags and ends (63).
/// Annotation value.
pub const PDF_ANNOT_3D: i32 = 26;
/// Annotation value.
pub const PDF_ANNOT_CARET: i32 = 14;
/// Annotation value.
pub const PDF_ANNOT_CIRCLE: i32 = 5;
/// Annotation value.
pub const PDF_ANNOT_FILE_ATTACHMENT: i32 = 17;
/// Annotation value.
pub const PDF_ANNOT_FREE_TEXT: i32 = 2;
/// Annotation value.
pub const PDF_ANNOT_HIGHLIGHT: i32 = 8;
/// Annotation value.
pub const PDF_ANNOT_INK: i32 = 15;
/// Annotation value.
pub const PDF_ANNOT_IS_HIDDEN: i32 = 2;
/// Annotation value.
pub const PDF_ANNOT_IS_INVISIBLE: i32 = 1;
/// Annotation value.
pub const PDF_ANNOT_IS_LOCKED: i32 = 128;
/// Annotation value.
pub const PDF_ANNOT_IS_LOCKED_CONTENTS: i32 = 512;
/// Annotation value.
pub const PDF_ANNOT_IS_NO_ROTATE: i32 = 16;
/// Annotation value.
pub const PDF_ANNOT_IS_NO_VIEW: i32 = 32;
/// Annotation value.
pub const PDF_ANNOT_IS_NO_ZOOM: i32 = 8;
/// Annotation value.
pub const PDF_ANNOT_IS_PRINT: i32 = 4;
/// Annotation value.
pub const PDF_ANNOT_IS_READ_ONLY: i32 = 64;
/// Annotation value.
pub const PDF_ANNOT_IS_TOGGLE_NO_VIEW: i32 = 256;
/// Annotation value.
pub const PDF_ANNOT_IT_DEFAULT: i32 = 0;
/// Annotation value.
pub const PDF_ANNOT_IT_FREETEXT_CALLOUT: i32 = 1;
/// Annotation value.
pub const PDF_ANNOT_IT_FREETEXT_TYPEWRITER: i32 = 2;
/// Annotation value.
pub const PDF_ANNOT_IT_LINE_ARROW: i32 = 3;
/// Annotation value.
pub const PDF_ANNOT_IT_LINE_DIMENSION: i32 = 4;
/// Annotation value.
pub const PDF_ANNOT_IT_POLYGON_CLOUD: i32 = 6;
/// Annotation value.
pub const PDF_ANNOT_IT_POLYGON_DIMENSION: i32 = 7;
/// Annotation value.
pub const PDF_ANNOT_IT_POLYLINE_DIMENSION: i32 = 5;
/// Annotation value.
pub const PDF_ANNOT_IT_STAMP_IMAGE: i32 = 8;
/// Annotation value.
pub const PDF_ANNOT_IT_STAMP_SNAPSHOT: i32 = 9;
/// Annotation value.
pub const PDF_ANNOT_IT_UNKNOWN: i32 = 255;
/// Annotation value.
pub const PDF_ANNOT_LE_BUTT: i32 = 6;
/// Annotation value.
pub const PDF_ANNOT_LE_CIRCLE: i32 = 2;
/// Annotation value.
pub const PDF_ANNOT_LE_CLOSED_ARROW: i32 = 5;
/// Annotation value.
pub const PDF_ANNOT_LE_DIAMOND: i32 = 3;
/// Annotation value.
pub const PDF_ANNOT_LE_NONE: i32 = 0;
/// Annotation value.
pub const PDF_ANNOT_LE_OPEN_ARROW: i32 = 4;
/// Annotation value.
pub const PDF_ANNOT_LE_R_CLOSED_ARROW: i32 = 8;
/// Annotation value.
pub const PDF_ANNOT_LE_R_OPEN_ARROW: i32 = 7;
/// Annotation value.
pub const PDF_ANNOT_LE_SLASH: i32 = 9;
/// Annotation value.
pub const PDF_ANNOT_LE_SQUARE: i32 = 1;
/// Annotation value.
pub const PDF_ANNOT_LINE: i32 = 3;
/// Annotation value.
pub const PDF_ANNOT_LINK: i32 = 1;
/// Annotation value.
pub const PDF_ANNOT_MOVIE: i32 = 19;
/// Annotation value.
pub const PDF_ANNOT_POLYGON: i32 = 6;
/// Annotation value.
pub const PDF_ANNOT_POLY_LINE: i32 = 7;
/// Annotation value.
pub const PDF_ANNOT_POPUP: i32 = 16;
/// Annotation value.
pub const PDF_ANNOT_PRINTER_MARK: i32 = 23;
/// Annotation value.
pub const PDF_ANNOT_PROJECTION: i32 = 27;
/// Annotation value.
pub const PDF_ANNOT_Q_CENTER: i32 = 1;
/// Annotation value.
pub const PDF_ANNOT_Q_LEFT: i32 = 0;
/// Annotation value.
pub const PDF_ANNOT_Q_RIGHT: i32 = 2;
/// Annotation value.
pub const PDF_ANNOT_REDACT: i32 = 12;
/// Annotation value.
pub const PDF_ANNOT_RICH_MEDIA: i32 = 20;
/// Annotation value.
pub const PDF_ANNOT_SCREEN: i32 = 22;
/// Annotation value.
pub const PDF_ANNOT_SOUND: i32 = 18;
/// Annotation value.
pub const PDF_ANNOT_SQUARE: i32 = 4;
/// Annotation value.
pub const PDF_ANNOT_SQUIGGLY: i32 = 10;
/// Annotation value.
pub const PDF_ANNOT_STAMP: i32 = 13;
/// Annotation value.
pub const PDF_ANNOT_STRIKE_OUT: i32 = 11;
/// Annotation value.
pub const PDF_ANNOT_TEXT: i32 = 0;
/// Annotation value.
pub const PDF_ANNOT_TRAP_NET: i32 = 24;
/// Annotation value.
pub const PDF_ANNOT_UNDERLINE: i32 = 9;
/// Annotation value.
pub const PDF_ANNOT_UNKNOWN: i32 = -1;
/// Annotation value.
pub const PDF_ANNOT_WATERMARK: i32 = 25;
/// Annotation value.
pub const PDF_ANNOT_WIDGET: i32 = 21;

// Annotation borders (7).
/// Annotation border value.
pub const PDF_BORDER_EFFECT_CLOUDY: i32 = 1;
/// Annotation border value.
pub const PDF_BORDER_EFFECT_NONE: i32 = 0;
/// Annotation border value.
pub const PDF_BORDER_STYLE_BEVELED: i32 = 2;
/// Annotation border value.
pub const PDF_BORDER_STYLE_DASHED: i32 = 1;
/// Annotation border value.
pub const PDF_BORDER_STYLE_INSET: i32 = 3;
/// Annotation border value.
pub const PDF_BORDER_STYLE_SOLID: i32 = 0;
/// Annotation border value.
pub const PDF_BORDER_STYLE_UNDERLINE: i32 = 4;

// Widget and field flags (33).
/// Widget or field value.
pub const PDF_BTN_FIELD_IS_NO_TOGGLE_TO_OFF: i32 = 16384;
/// Widget or field value.
pub const PDF_BTN_FIELD_IS_PUSHBUTTON: i32 = 65536;
/// Widget or field value.
pub const PDF_BTN_FIELD_IS_RADIO: i32 = 32768;
/// Widget or field value.
pub const PDF_BTN_FIELD_IS_RADIOS_IN_UNISON: i32 = 33554432;
/// Widget or field value.
pub const PDF_CH_FIELD_IS_COMBO: i32 = 131072;
/// Widget or field value.
pub const PDF_CH_FIELD_IS_COMMIT_ON_SEL_CHANGE: i32 = 33554432;
/// Widget or field value.
pub const PDF_CH_FIELD_IS_DO_NOT_SPELL_CHECK: i32 = 4194304;
/// Widget or field value.
pub const PDF_CH_FIELD_IS_EDIT: i32 = 262144;
/// Widget or field value.
pub const PDF_CH_FIELD_IS_MULTI_SELECT: i32 = 2097152;
/// Widget or field value.
pub const PDF_CH_FIELD_IS_SORT: i32 = 524288;
/// Widget or field value.
pub const PDF_FIELD_IS_NO_EXPORT: i32 = 4;
/// Widget or field value.
pub const PDF_FIELD_IS_READ_ONLY: i32 = 1;
/// Widget or field value.
pub const PDF_FIELD_IS_REQUIRED: i32 = 2;
/// Widget or field value.
pub const PDF_TX_FIELD_IS_COMB: i32 = 16777216;
/// Widget or field value.
pub const PDF_TX_FIELD_IS_DO_NOT_SCROLL: i32 = 8388608;
/// Widget or field value.
pub const PDF_TX_FIELD_IS_DO_NOT_SPELL_CHECK: i32 = 4194304;
/// Widget or field value.
pub const PDF_TX_FIELD_IS_FILE_SELECT: i32 = 1048576;
/// Widget or field value.
pub const PDF_TX_FIELD_IS_MULTILINE: i32 = 4096;
/// Widget or field value.
pub const PDF_TX_FIELD_IS_PASSWORD: i32 = 8192;
/// Widget or field value.
pub const PDF_TX_FIELD_IS_RICH_TEXT: i32 = 33554432;
/// Widget or field value.
pub const PDF_WIDGET_TX_FORMAT_DATE: i32 = 3;
/// Widget or field value.
pub const PDF_WIDGET_TX_FORMAT_NONE: i32 = 0;
/// Widget or field value.
pub const PDF_WIDGET_TX_FORMAT_NUMBER: i32 = 1;
/// Widget or field value.
pub const PDF_WIDGET_TX_FORMAT_SPECIAL: i32 = 2;
/// Widget or field value.
pub const PDF_WIDGET_TX_FORMAT_TIME: i32 = 4;
/// Widget or field value.
pub const PDF_WIDGET_TYPE_BUTTON: i32 = 1;
/// Widget or field value.
pub const PDF_WIDGET_TYPE_CHECKBOX: i32 = 2;
/// Widget or field value.
pub const PDF_WIDGET_TYPE_COMBOBOX: i32 = 3;
/// Widget or field value.
pub const PDF_WIDGET_TYPE_LISTBOX: i32 = 4;
/// Widget or field value.
pub const PDF_WIDGET_TYPE_RADIOBUTTON: i32 = 5;
/// Widget or field value.
pub const PDF_WIDGET_TYPE_SIGNATURE: i32 = 6;
/// Widget or field value.
pub const PDF_WIDGET_TYPE_TEXT: i32 = 7;
/// Widget or field value.
pub const PDF_WIDGET_TYPE_UNKNOWN: i32 = 0;

// Stamp names (14).
/// Stamp code.
pub const STAMP_Approved: i32 = 0;
/// Stamp code.
pub const STAMP_AsIs: i32 = 1;
/// Stamp code.
pub const STAMP_Confidential: i32 = 2;
/// Stamp code.
pub const STAMP_Departmental: i32 = 3;
/// Stamp code.
pub const STAMP_Draft: i32 = 13;
/// Stamp code.
pub const STAMP_Experimental: i32 = 4;
/// Stamp code.
pub const STAMP_Expired: i32 = 5;
/// Stamp code.
pub const STAMP_Final: i32 = 6;
/// Stamp code.
pub const STAMP_ForComment: i32 = 7;
/// Stamp code.
pub const STAMP_ForPublicRelease: i32 = 8;
/// Stamp code.
pub const STAMP_NotApproved: i32 = 9;
/// Stamp code.
pub const STAMP_NotForPublicRelease: i32 = 10;
/// Stamp code.
pub const STAMP_Sold: i32 = 11;
/// Stamp code.
pub const STAMP_TopSecret: i32 = 12;

// Blend modes (16).
/// Blend mode.
pub const PDF_BM_Color: &str = "Color";
/// Blend mode.
pub const PDF_BM_ColorBurn: &str = "ColorBurn";
/// Blend mode.
pub const PDF_BM_ColorDodge: &str = "ColorDodge";
/// Blend mode.
pub const PDF_BM_Darken: &str = "Darken";
/// Blend mode.
pub const PDF_BM_Difference: &str = "Difference";
/// Blend mode.
pub const PDF_BM_Exclusion: &str = "Exclusion";
/// Blend mode.
pub const PDF_BM_HardLight: &str = "HardLight";
/// Blend mode.
pub const PDF_BM_Hue: &str = "Hue";
/// Blend mode.
pub const PDF_BM_Lighten: &str = "Lighten";
/// Blend mode.
pub const PDF_BM_Luminosity: &str = "Luminosity";
/// Blend mode.
pub const PDF_BM_Multiply: &str = "Multiply";
/// Blend mode.
pub const PDF_BM_Normal: &str = "Normal";
/// Blend mode.
pub const PDF_BM_Overlay: &str = "Overlay";
/// Blend mode.
pub const PDF_BM_Saturation: &str = "Saturation";
/// Blend mode.
pub const PDF_BM_Screen: &str = "Screen";
/// Blend mode.
pub const PDF_BM_SoftLight: &str = "Softlight";

// Redaction controls (10).
/// Redaction value.
pub const PDF_REDACT_IMAGE_NONE: i32 = 0;
/// Redaction value.
pub const PDF_REDACT_IMAGE_PIXELS: i32 = 2;
/// Redaction value.
pub const PDF_REDACT_IMAGE_REMOVE: i32 = 1;
/// Redaction value.
pub const PDF_REDACT_IMAGE_REMOVE_UNLESS_INVISIBLE: i32 = 3;
/// Redaction value.
pub const PDF_REDACT_LINE_ART_NONE: i32 = 0;
/// Redaction value.
pub const PDF_REDACT_LINE_ART_REMOVE_IF_COVERED: i32 = 1;
/// Redaction value.
pub const PDF_REDACT_LINE_ART_REMOVE_IF_TOUCHED: i32 = 2;
/// Redaction value.
pub const PDF_REDACT_TEXT_NONE: i32 = 1;
/// Redaction value.
pub const PDF_REDACT_TEXT_REMOVE: i32 = 0;
/// Redaction value.
pub const PDF_REDACT_TEXT_REMOVE_INVISIBLE: i32 = 2;

// Script codes for font setup (171).
/// Script code.
pub const UCDN_SCRIPT_ADLAM: i32 = 132;
/// Script code.
pub const UCDN_SCRIPT_AHOM: i32 = 126;
/// Script code.
pub const UCDN_SCRIPT_ANATOLIAN_HIEROGLYPHS: i32 = 127;
/// Script code.
pub const UCDN_SCRIPT_ARABIC: i32 = 6;
/// Script code.
pub const UCDN_SCRIPT_ARMENIAN: i32 = 4;
/// Script code.
pub const UCDN_SCRIPT_AVESTAN: i32 = 79;
/// Script code.
pub const UCDN_SCRIPT_BALINESE: i32 = 61;
/// Script code.
pub const UCDN_SCRIPT_BAMUM: i32 = 83;
/// Script code.
pub const UCDN_SCRIPT_BASSA_VAH: i32 = 103;
/// Script code.
pub const UCDN_SCRIPT_BATAK: i32 = 92;
/// Script code.
pub const UCDN_SCRIPT_BENGALI: i32 = 10;
/// Script code.
pub const UCDN_SCRIPT_BHAIKSUKI: i32 = 133;
/// Script code.
pub const UCDN_SCRIPT_BOPOMOFO: i32 = 34;
/// Script code.
pub const UCDN_SCRIPT_BRAHMI: i32 = 93;
/// Script code.
pub const UCDN_SCRIPT_BRAILLE: i32 = 52;
/// Script code.
pub const UCDN_SCRIPT_BUGINESE: i32 = 53;
/// Script code.
pub const UCDN_SCRIPT_BUHID: i32 = 43;
/// Script code.
pub const UCDN_SCRIPT_CANADIAN_ABORIGINAL: i32 = 27;
/// Script code.
pub const UCDN_SCRIPT_CARIAN: i32 = 74;
/// Script code.
pub const UCDN_SCRIPT_CAUCASIAN_ALBANIAN: i32 = 104;
/// Script code.
pub const UCDN_SCRIPT_CHAKMA: i32 = 95;
/// Script code.
pub const UCDN_SCRIPT_CHAM: i32 = 76;
/// Script code.
pub const UCDN_SCRIPT_CHEROKEE: i32 = 26;
/// Script code.
pub const UCDN_SCRIPT_CHORASMIAN: i32 = 153;
/// Script code.
pub const UCDN_SCRIPT_COMMON: i32 = 0;
/// Script code.
pub const UCDN_SCRIPT_COPTIC: i32 = 54;
/// Script code.
pub const UCDN_SCRIPT_CUNEIFORM: i32 = 62;
/// Script code.
pub const UCDN_SCRIPT_CYPRIOT: i32 = 51;
/// Script code.
pub const UCDN_SCRIPT_CYPRO_MINOAN: i32 = 159;
/// Script code.
pub const UCDN_SCRIPT_CYRILLIC: i32 = 3;
/// Script code.
pub const UCDN_SCRIPT_DESERET: i32 = 39;
/// Script code.
pub const UCDN_SCRIPT_DEVANAGARI: i32 = 9;
/// Script code.
pub const UCDN_SCRIPT_DIVES_AKURU: i32 = 154;
/// Script code.
pub const UCDN_SCRIPT_DOGRA: i32 = 142;
/// Script code.
pub const UCDN_SCRIPT_DUPLOYAN: i32 = 105;
/// Script code.
pub const UCDN_SCRIPT_EGYPTIAN_HIEROGLYPHS: i32 = 80;
/// Script code.
pub const UCDN_SCRIPT_ELBASAN: i32 = 106;
/// Script code.
pub const UCDN_SCRIPT_ELYMAIC: i32 = 149;
/// Script code.
pub const UCDN_SCRIPT_ETHIOPIC: i32 = 25;
/// Script code.
pub const UCDN_SCRIPT_GARAY: i32 = 165;
/// Script code.
pub const UCDN_SCRIPT_GEORGIAN: i32 = 23;
/// Script code.
pub const UCDN_SCRIPT_GLAGOLITIC: i32 = 56;
/// Script code.
pub const UCDN_SCRIPT_GOTHIC: i32 = 38;
/// Script code.
pub const UCDN_SCRIPT_GRANTHA: i32 = 107;
/// Script code.
pub const UCDN_SCRIPT_GREEK: i32 = 2;
/// Script code.
pub const UCDN_SCRIPT_GUJARATI: i32 = 12;
/// Script code.
pub const UCDN_SCRIPT_GUNJALA_GONDI: i32 = 143;
/// Script code.
pub const UCDN_SCRIPT_GURMUKHI: i32 = 11;
/// Script code.
pub const UCDN_SCRIPT_GURUNG_KHEMA: i32 = 168;
/// Script code.
pub const UCDN_SCRIPT_HAN: i32 = 35;
/// Script code.
pub const UCDN_SCRIPT_HANGUL: i32 = 24;
/// Script code.
pub const UCDN_SCRIPT_HANIFI_ROHINGYA: i32 = 144;
/// Script code.
pub const UCDN_SCRIPT_HANUNOO: i32 = 42;
/// Script code.
pub const UCDN_SCRIPT_HATRAN: i32 = 128;
/// Script code.
pub const UCDN_SCRIPT_HEBREW: i32 = 5;
/// Script code.
pub const UCDN_SCRIPT_HIRAGANA: i32 = 32;
/// Script code.
pub const UCDN_SCRIPT_IMPERIAL_ARAMAIC: i32 = 86;
/// Script code.
pub const UCDN_SCRIPT_INHERITED: i32 = 40;
/// Script code.
pub const UCDN_SCRIPT_INSCRIPTIONAL_PAHLAVI: i32 = 89;
/// Script code.
pub const UCDN_SCRIPT_INSCRIPTIONAL_PARTHIAN: i32 = 88;
/// Script code.
pub const UCDN_SCRIPT_JAVANESE: i32 = 84;
/// Script code.
pub const UCDN_SCRIPT_KAITHI: i32 = 91;
/// Script code.
pub const UCDN_SCRIPT_KANNADA: i32 = 16;
/// Script code.
pub const UCDN_SCRIPT_KATAKANA: i32 = 33;
/// Script code.
pub const UCDN_SCRIPT_KAWI: i32 = 162;
/// Script code.
pub const UCDN_SCRIPT_KAYAH_LI: i32 = 71;
/// Script code.
pub const UCDN_SCRIPT_KHAROSHTHI: i32 = 60;
/// Script code.
pub const UCDN_SCRIPT_KHITAN_SMALL_SCRIPT: i32 = 155;
/// Script code.
pub const UCDN_SCRIPT_KHMER: i32 = 30;
/// Script code.
pub const UCDN_SCRIPT_KHOJKI: i32 = 108;
/// Script code.
pub const UCDN_SCRIPT_KHUDAWADI: i32 = 109;
/// Script code.
pub const UCDN_SCRIPT_KIRAT_RAI: i32 = 169;
/// Script code.
pub const UCDN_SCRIPT_LAO: i32 = 20;
/// Script code.
pub const UCDN_SCRIPT_LATIN: i32 = 1;
/// Script code.
pub const UCDN_SCRIPT_LEPCHA: i32 = 67;
/// Script code.
pub const UCDN_SCRIPT_LIMBU: i32 = 45;
/// Script code.
pub const UCDN_SCRIPT_LINEAR_A: i32 = 110;
/// Script code.
pub const UCDN_SCRIPT_LINEAR_B: i32 = 47;
/// Script code.
pub const UCDN_SCRIPT_LISU: i32 = 82;
/// Script code.
pub const UCDN_SCRIPT_LYCIAN: i32 = 73;
/// Script code.
pub const UCDN_SCRIPT_LYDIAN: i32 = 75;
/// Script code.
pub const UCDN_SCRIPT_MAHAJANI: i32 = 111;
/// Script code.
pub const UCDN_SCRIPT_MAKASAR: i32 = 145;
/// Script code.
pub const UCDN_SCRIPT_MALAYALAM: i32 = 17;
/// Script code.
pub const UCDN_SCRIPT_MANDAIC: i32 = 94;
/// Script code.
pub const UCDN_SCRIPT_MANICHAEAN: i32 = 112;
/// Script code.
pub const UCDN_SCRIPT_MARCHEN: i32 = 134;
/// Script code.
pub const UCDN_SCRIPT_MASARAM_GONDI: i32 = 138;
/// Script code.
pub const UCDN_SCRIPT_MEDEFAIDRIN: i32 = 146;
/// Script code.
pub const UCDN_SCRIPT_MEETEI_MAYEK: i32 = 85;
/// Script code.
pub const UCDN_SCRIPT_MENDE_KIKAKUI: i32 = 113;
/// Script code.
pub const UCDN_SCRIPT_MEROITIC_CURSIVE: i32 = 96;
/// Script code.
pub const UCDN_SCRIPT_MEROITIC_HIEROGLYPHS: i32 = 97;
/// Script code.
pub const UCDN_SCRIPT_MIAO: i32 = 98;
/// Script code.
pub const UCDN_SCRIPT_MODI: i32 = 114;
/// Script code.
pub const UCDN_SCRIPT_MONGOLIAN: i32 = 31;
/// Script code.
pub const UCDN_SCRIPT_MRO: i32 = 115;
/// Script code.
pub const UCDN_SCRIPT_MULTANI: i32 = 129;
/// Script code.
pub const UCDN_SCRIPT_MYANMAR: i32 = 22;
/// Script code.
pub const UCDN_SCRIPT_NABATAEAN: i32 = 116;
/// Script code.
pub const UCDN_SCRIPT_NAG_MUNDARI: i32 = 163;
/// Script code.
pub const UCDN_SCRIPT_NANDINAGARI: i32 = 150;
/// Script code.
pub const UCDN_SCRIPT_NEWA: i32 = 135;
/// Script code.
pub const UCDN_SCRIPT_NEW_TAI_LUE: i32 = 55;
/// Script code.
pub const UCDN_SCRIPT_NKO: i32 = 65;
/// Script code.
pub const UCDN_SCRIPT_NUSHU: i32 = 139;
/// Script code.
pub const UCDN_SCRIPT_NYIAKENG_PUACHUE_HMONG: i32 = 151;
/// Script code.
pub const UCDN_SCRIPT_OGHAM: i32 = 28;
/// Script code.
pub const UCDN_SCRIPT_OLD_HUNGARIAN: i32 = 130;
/// Script code.
pub const UCDN_SCRIPT_OLD_ITALIC: i32 = 37;
/// Script code.
pub const UCDN_SCRIPT_OLD_NORTH_ARABIAN: i32 = 117;
/// Script code.
pub const UCDN_SCRIPT_OLD_PERMIC: i32 = 118;
/// Script code.
pub const UCDN_SCRIPT_OLD_PERSIAN: i32 = 59;
/// Script code.
pub const UCDN_SCRIPT_OLD_SOGDIAN: i32 = 147;
/// Script code.
pub const UCDN_SCRIPT_OLD_SOUTH_ARABIAN: i32 = 87;
/// Script code.
pub const UCDN_SCRIPT_OLD_TURKIC: i32 = 90;
/// Script code.
pub const UCDN_SCRIPT_OLD_UYGHUR: i32 = 158;
/// Script code.
pub const UCDN_SCRIPT_OL_CHIKI: i32 = 68;
/// Script code.
pub const UCDN_SCRIPT_OL_ONAL: i32 = 170;
/// Script code.
pub const UCDN_SCRIPT_ORIYA: i32 = 13;
/// Script code.
pub const UCDN_SCRIPT_OSAGE: i32 = 136;
/// Script code.
pub const UCDN_SCRIPT_OSMANYA: i32 = 50;
/// Script code.
pub const UCDN_SCRIPT_PAHAWH_HMONG: i32 = 119;
/// Script code.
pub const UCDN_SCRIPT_PALMYRENE: i32 = 120;
/// Script code.
pub const UCDN_SCRIPT_PAU_CIN_HAU: i32 = 121;
/// Script code.
pub const UCDN_SCRIPT_PHAGS_PA: i32 = 64;
/// Script code.
pub const UCDN_SCRIPT_PHOENICIAN: i32 = 63;
/// Script code.
pub const UCDN_SCRIPT_PSALTER_PAHLAVI: i32 = 122;
/// Script code.
pub const UCDN_SCRIPT_REJANG: i32 = 72;
/// Script code.
pub const UCDN_SCRIPT_RUNIC: i32 = 29;
/// Script code.
pub const UCDN_SCRIPT_SAMARITAN: i32 = 81;
/// Script code.
pub const UCDN_SCRIPT_SAURASHTRA: i32 = 70;
/// Script code.
pub const UCDN_SCRIPT_SHARADA: i32 = 99;
/// Script code.
pub const UCDN_SCRIPT_SHAVIAN: i32 = 49;
/// Script code.
pub const UCDN_SCRIPT_SIDDHAM: i32 = 123;
/// Script code.
pub const UCDN_SCRIPT_SIGNWRITING: i32 = 131;
/// Script code.
pub const UCDN_SCRIPT_SINHALA: i32 = 18;
/// Script code.
pub const UCDN_SCRIPT_SOGDIAN: i32 = 148;
/// Script code.
pub const UCDN_SCRIPT_SORA_SOMPENG: i32 = 100;
/// Script code.
pub const UCDN_SCRIPT_SOYOMBO: i32 = 140;
/// Script code.
pub const UCDN_SCRIPT_SUNDANESE: i32 = 66;
/// Script code.
pub const UCDN_SCRIPT_SUNUWAR: i32 = 167;
/// Script code.
pub const UCDN_SCRIPT_SYLOTI_NAGRI: i32 = 58;
/// Script code.
pub const UCDN_SCRIPT_SYRIAC: i32 = 7;
/// Script code.
pub const UCDN_SCRIPT_TAGALOG: i32 = 41;
/// Script code.
pub const UCDN_SCRIPT_TAGBANWA: i32 = 44;
/// Script code.
pub const UCDN_SCRIPT_TAI_LE: i32 = 46;
/// Script code.
pub const UCDN_SCRIPT_TAI_THAM: i32 = 77;
/// Script code.
pub const UCDN_SCRIPT_TAI_VIET: i32 = 78;
/// Script code.
pub const UCDN_SCRIPT_TAKRI: i32 = 101;
/// Script code.
pub const UCDN_SCRIPT_TAMIL: i32 = 14;
/// Script code.
pub const UCDN_SCRIPT_TANGSA: i32 = 160;
/// Script code.
pub const UCDN_SCRIPT_TANGUT: i32 = 137;
/// Script code.
pub const UCDN_SCRIPT_TELUGU: i32 = 15;
/// Script code.
pub const UCDN_SCRIPT_THAANA: i32 = 8;
/// Script code.
pub const UCDN_SCRIPT_THAI: i32 = 19;
/// Script code.
pub const UCDN_SCRIPT_TIBETAN: i32 = 21;
/// Script code.
pub const UCDN_SCRIPT_TIFINAGH: i32 = 57;
/// Script code.
pub const UCDN_SCRIPT_TIRHUTA: i32 = 124;
/// Script code.
pub const UCDN_SCRIPT_TODHRI: i32 = 164;
/// Script code.
pub const UCDN_SCRIPT_TOTO: i32 = 161;
/// Script code.
pub const UCDN_SCRIPT_TULU_TIGALARI: i32 = 166;
/// Script code.
pub const UCDN_SCRIPT_UGARITIC: i32 = 48;
/// Script code.
pub const UCDN_SCRIPT_UNKNOWN: i32 = 102;
/// Script code.
pub const UCDN_SCRIPT_VAI: i32 = 69;
/// Script code.
pub const UCDN_SCRIPT_VITHKUQI: i32 = 157;
/// Script code.
pub const UCDN_SCRIPT_WANCHO: i32 = 152;
/// Script code.
pub const UCDN_SCRIPT_WARANG_CITI: i32 = 125;
/// Script code.
pub const UCDN_SCRIPT_YEZIDI: i32 = 156;
/// Script code.
pub const UCDN_SCRIPT_YI: i32 = 36;
/// Script code.
pub const UCDN_SCRIPT_ZANABAZAR_SQUARE: i32 = 141;

// Font descriptor flags and resources (15).
/// Font descriptor value.
pub const PDF_CID_FONT_RESOURCE: i32 = 2;
/// Font descriptor value.
pub const PDF_CJK_FONT_RESOURCE: i32 = 3;
/// Font descriptor value.
pub const PDF_FD_ALL_CAP: i32 = 65536;
/// Font descriptor value.
pub const PDF_FD_FIXED_PITCH: i32 = 1;
/// Font descriptor value.
pub const PDF_FD_FORCE_BOLD: i32 = 262144;
/// Font descriptor value.
pub const PDF_FD_ITALIC: i32 = 64;
/// Font descriptor value.
pub const PDF_FD_NONSYMBOLIC: i32 = 32;
/// Font descriptor value.
pub const PDF_FD_SCRIPT: i32 = 8;
/// Font descriptor value.
pub const PDF_FD_SERIF: i32 = 2;
/// Font descriptor value.
pub const PDF_FD_SMALL_CAP: i32 = 131072;
/// Font descriptor value.
pub const PDF_FD_SYMBOLIC: i32 = 4;
/// Font descriptor value.
pub const PDF_SIMPLE_ENCODING_CYRILLIC: i32 = 2;
/// Font descriptor value.
pub const PDF_SIMPLE_ENCODING_GREEK: i32 = 1;
/// Font descriptor value.
pub const PDF_SIMPLE_ENCODING_LATIN: i32 = 0;
/// Font descriptor value.
pub const PDF_SIMPLE_FONT_RESOURCE: i32 = 1;

// Base-14 font tables (4).
/// Base-14 font table.
pub const Base14_fontdict: &[(&str, &str)] = &[
    ("cobi", "Courier-BoldOblique"),
    ("cobo", "Courier-Bold"),
    ("coit", "Courier-Oblique"),
    ("cour", "Courier"),
    ("courier", "Courier"),
    ("courier-bold", "Courier-Bold"),
    ("courier-boldoblique", "Courier-BoldOblique"),
    ("courier-oblique", "Courier-Oblique"),
    ("hebi", "Helvetica-BoldOblique"),
    ("hebo", "Helvetica-Bold"),
    ("heit", "Helvetica-Oblique"),
    ("helv", "Helvetica"),
    ("helvetica", "Helvetica"),
    ("helvetica-bold", "Helvetica-Bold"),
    ("helvetica-boldoblique", "Helvetica-BoldOblique"),
    ("helvetica-oblique", "Helvetica-Oblique"),
    ("symb", "Symbol"),
    ("symbol", "Symbol"),
    ("tibi", "Times-BoldItalic"),
    ("tibo", "Times-Bold"),
    ("tiit", "Times-Italic"),
    ("times-bold", "Times-Bold"),
    ("times-bolditalic", "Times-BoldItalic"),
    ("times-italic", "Times-Italic"),
    ("times-roman", "Times-Roman"),
    ("tiro", "Times-Roman"),
    ("zadb", "ZapfDingbats"),
    ("zapfdingbats", "ZapfDingbats"),
];
/// Base-14 font table.
pub const Base14_fontnames: &[&str] = &[
    "Courier",
    "Courier-Oblique",
    "Courier-Bold",
    "Courier-BoldOblique",
    "Helvetica",
    "Helvetica-Oblique",
    "Helvetica-Bold",
    "Helvetica-BoldOblique",
    "Times-Roman",
    "Times-Italic",
    "Times-Bold",
    "Times-BoldItalic",
    "Symbol",
    "ZapfDingbats",
];
/// Base-14 font table.
pub const symbol_glyphs: &[(i32, f64)] = &[
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (32, 0.25),
    (33, 0.333),
    (34, 0.713),
    (35, 0.5),
    (36, 0.549),
    (37, 0.833),
    (38, 0.778),
    (39, 0.439),
    (40, 0.333),
    (41, 0.333),
    (42, 0.5),
    (43, 0.549),
    (44, 0.25),
    (45, 0.549),
    (46, 0.25),
    (47, 0.278),
    (48, 0.5),
    (49, 0.5),
    (50, 0.5),
    (51, 0.5),
    (52, 0.5),
    (53, 0.5),
    (54, 0.5),
    (55, 0.5),
    (56, 0.5),
    (57, 0.5),
    (58, 0.278),
    (59, 0.278),
    (60, 0.549),
    (61, 0.549),
    (62, 0.549),
    (63, 0.444),
    (64, 0.549),
    (65, 0.722),
    (66, 0.667),
    (67, 0.722),
    (68, 0.612),
    (69, 0.611),
    (70, 0.763),
    (71, 0.603),
    (72, 0.722),
    (73, 0.333),
    (74, 0.631),
    (75, 0.722),
    (76, 0.686),
    (77, 0.889),
    (78, 0.722),
    (79, 0.722),
    (80, 0.768),
    (81, 0.741),
    (82, 0.556),
    (83, 0.592),
    (84, 0.611),
    (85, 0.69),
    (86, 0.439),
    (87, 0.768),
    (88, 0.645),
    (89, 0.795),
    (90, 0.611),
    (91, 0.333),
    (92, 0.863),
    (93, 0.333),
    (94, 0.658),
    (95, 0.5),
    (96, 0.5),
    (97, 0.631),
    (98, 0.549),
    (99, 0.549),
    (100, 0.494),
    (101, 0.439),
    (102, 0.521),
    (103, 0.411),
    (104, 0.603),
    (105, 0.329),
    (106, 0.603),
    (107, 0.549),
    (108, 0.549),
    (109, 0.576),
    (110, 0.521),
    (111, 0.549),
    (112, 0.549),
    (113, 0.521),
    (114, 0.549),
    (115, 0.603),
    (116, 0.439),
    (117, 0.576),
    (118, 0.713),
    (119, 0.686),
    (120, 0.493),
    (121, 0.686),
    (122, 0.494),
    (123, 0.48),
    (124, 0.2),
    (125, 0.48),
    (126, 0.549),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (183, 0.46),
    (160, 0.25),
    (161, 0.62),
    (162, 0.247),
    (163, 0.549),
    (164, 0.167),
    (165, 0.713),
    (166, 0.5),
    (167, 0.753),
    (168, 0.753),
    (169, 0.753),
    (170, 0.753),
    (171, 1.042),
    (172, 0.713),
    (173, 0.603),
    (174, 0.987),
    (175, 0.603),
    (176, 0.4),
    (177, 0.549),
    (178, 0.411),
    (179, 0.549),
    (180, 0.549),
    (181, 0.576),
    (182, 0.494),
    (183, 0.46),
    (184, 0.549),
    (185, 0.549),
    (186, 0.549),
    (187, 0.549),
    (188, 1.0),
    (189, 0.603),
    (190, 1.0),
    (191, 0.658),
    (192, 0.823),
    (193, 0.686),
    (194, 0.795),
    (195, 0.987),
    (196, 0.768),
    (197, 0.768),
    (198, 0.823),
    (199, 0.768),
    (200, 0.768),
    (201, 0.713),
    (202, 0.713),
    (203, 0.713),
    (204, 0.713),
    (205, 0.713),
    (206, 0.713),
    (207, 0.713),
    (208, 0.768),
    (209, 0.713),
    (210, 0.79),
    (211, 0.79),
    (212, 0.89),
    (213, 0.823),
    (214, 0.549),
    (215, 0.549),
    (216, 0.713),
    (217, 0.603),
    (218, 0.603),
    (219, 1.042),
    (220, 0.987),
    (221, 0.603),
    (222, 0.987),
    (223, 0.603),
    (224, 0.494),
    (225, 0.329),
    (226, 0.79),
    (227, 0.79),
    (228, 0.786),
    (229, 0.713),
    (230, 0.384),
    (231, 0.384),
    (232, 0.384),
    (233, 0.384),
    (234, 0.384),
    (235, 0.384),
    (236, 0.494),
    (237, 0.494),
    (238, 0.494),
    (239, 0.494),
    (183, 0.46),
    (241, 0.329),
    (242, 0.274),
    (243, 0.686),
    (244, 0.686),
    (245, 0.686),
    (246, 0.384),
    (247, 0.549),
    (248, 0.384),
    (249, 0.384),
    (250, 0.384),
    (251, 0.384),
    (252, 0.494),
    (253, 0.494),
    (254, 0.494),
    (183, 0.46),
];
/// Base-14 font table.
pub const zapf_glyphs: &[(i32, f64)] = &[
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (32, 0.278),
    (33, 0.974),
    (34, 0.961),
    (35, 0.974),
    (36, 0.98),
    (37, 0.719),
    (38, 0.789),
    (39, 0.79),
    (40, 0.791),
    (41, 0.69),
    (42, 0.96),
    (43, 0.939),
    (44, 0.549),
    (45, 0.855),
    (46, 0.911),
    (47, 0.933),
    (48, 0.911),
    (49, 0.945),
    (50, 0.974),
    (51, 0.755),
    (52, 0.846),
    (53, 0.762),
    (54, 0.761),
    (55, 0.571),
    (56, 0.677),
    (57, 0.763),
    (58, 0.76),
    (59, 0.759),
    (60, 0.754),
    (61, 0.494),
    (62, 0.552),
    (63, 0.537),
    (64, 0.577),
    (65, 0.692),
    (66, 0.786),
    (67, 0.788),
    (68, 0.788),
    (69, 0.79),
    (70, 0.793),
    (71, 0.794),
    (72, 0.816),
    (73, 0.823),
    (74, 0.789),
    (75, 0.841),
    (76, 0.823),
    (77, 0.833),
    (78, 0.816),
    (79, 0.831),
    (80, 0.923),
    (81, 0.744),
    (82, 0.723),
    (83, 0.749),
    (84, 0.79),
    (85, 0.792),
    (86, 0.695),
    (87, 0.776),
    (88, 0.768),
    (89, 0.792),
    (90, 0.759),
    (91, 0.707),
    (92, 0.708),
    (93, 0.682),
    (94, 0.701),
    (95, 0.826),
    (96, 0.815),
    (97, 0.789),
    (98, 0.789),
    (99, 0.707),
    (100, 0.687),
    (101, 0.696),
    (102, 0.689),
    (103, 0.786),
    (104, 0.787),
    (105, 0.713),
    (106, 0.791),
    (107, 0.785),
    (108, 0.791),
    (109, 0.873),
    (110, 0.761),
    (111, 0.762),
    (112, 0.762),
    (113, 0.759),
    (114, 0.759),
    (115, 0.892),
    (116, 0.892),
    (117, 0.788),
    (118, 0.784),
    (119, 0.438),
    (120, 0.138),
    (121, 0.277),
    (122, 0.415),
    (123, 0.392),
    (124, 0.392),
    (125, 0.668),
    (126, 0.668),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (183, 0.788),
    (161, 0.732),
    (162, 0.544),
    (163, 0.544),
    (164, 0.91),
    (165, 0.667),
    (166, 0.76),
    (167, 0.76),
    (168, 0.776),
    (169, 0.595),
    (170, 0.694),
    (171, 0.626),
    (172, 0.788),
    (173, 0.788),
    (174, 0.788),
    (175, 0.788),
    (176, 0.788),
    (177, 0.788),
    (178, 0.788),
    (179, 0.788),
    (180, 0.788),
    (181, 0.788),
    (182, 0.788),
    (183, 0.788),
    (184, 0.788),
    (185, 0.788),
    (186, 0.788),
    (187, 0.788),
    (188, 0.788),
    (189, 0.788),
    (190, 0.788),
    (191, 0.788),
    (192, 0.788),
    (193, 0.788),
    (194, 0.788),
    (195, 0.788),
    (196, 0.788),
    (197, 0.788),
    (198, 0.788),
    (199, 0.788),
    (200, 0.788),
    (201, 0.788),
    (202, 0.788),
    (203, 0.788),
    (204, 0.788),
    (205, 0.788),
    (206, 0.788),
    (207, 0.788),
    (208, 0.788),
    (209, 0.788),
    (210, 0.788),
    (211, 0.788),
    (212, 0.894),
    (213, 0.838),
    (214, 1.016),
    (215, 0.458),
    (216, 0.748),
    (217, 0.924),
    (218, 0.748),
    (219, 0.918),
    (220, 0.927),
    (221, 0.928),
    (222, 0.928),
    (223, 0.834),
    (224, 0.873),
    (225, 0.828),
    (226, 0.924),
    (227, 0.924),
    (228, 0.917),
    (229, 0.93),
    (230, 0.931),
    (231, 0.463),
    (232, 0.883),
    (233, 0.836),
    (234, 0.836),
    (235, 0.867),
    (236, 0.867),
    (237, 0.696),
    (238, 0.696),
    (239, 0.874),
    (183, 0.788),
    (241, 0.874),
    (242, 0.76),
    (243, 0.946),
    (244, 0.771),
    (245, 0.865),
    (246, 0.771),
    (247, 0.888),
    (248, 0.967),
    (249, 0.888),
    (250, 0.831),
    (251, 0.873),
    (252, 0.927),
    (253, 0.97),
    (183, 0.788),
    (183, 0.788),
];

// Named colour table (1).
/// Named colour table.
pub const pdfcolor: &[(&str, [f64; 3])] = &[
    ("aliceblue", [0.9411764705882353, 0.9725490196078431, 1.0]),
    (
        "antiquewhite",
        [0.9803921568627451, 0.9215686274509803, 0.8431372549019608],
    ),
    (
        "antiquewhite1",
        [1.0, 0.9372549019607843, 0.8588235294117647],
    ),
    (
        "antiquewhite2",
        [0.9333333333333333, 0.8745098039215686, 0.8],
    ),
    (
        "antiquewhite3",
        [0.803921568627451, 0.7529411764705882, 0.6901960784313725],
    ),
    (
        "antiquewhite4",
        [0.5450980392156862, 0.5137254901960784, 0.47058823529411764],
    ),
    ("aqua", [0.0, 1.0, 1.0]),
    ("aquamarine", [0.4980392156862745, 1.0, 0.8313725490196079]),
    ("aquamarine1", [0.4980392156862745, 1.0, 0.8313725490196079]),
    (
        "aquamarine2",
        [0.4627450980392157, 0.9333333333333333, 0.7764705882352941],
    ),
    ("aquamarine3", [0.4, 0.803921568627451, 0.6666666666666666]),
    (
        "aquamarine4",
        [0.27058823529411763, 0.5450980392156862, 0.4549019607843137],
    ),
    ("azure", [0.9411764705882353, 1.0, 1.0]),
    ("azure1", [0.9411764705882353, 1.0, 1.0]),
    (
        "azure2",
        [0.8784313725490196, 0.9333333333333333, 0.9333333333333333],
    ),
    (
        "azure3",
        [0.7568627450980392, 0.803921568627451, 0.803921568627451],
    ),
    (
        "azure4",
        [0.5137254901960784, 0.5450980392156862, 0.5450980392156862],
    ),
    (
        "beige",
        [0.9607843137254902, 0.9607843137254902, 0.8627450980392157],
    ),
    ("bisque", [1.0, 0.8941176470588236, 0.7686274509803922]),
    ("bisque1", [1.0, 0.8941176470588236, 0.7686274509803922]),
    (
        "bisque2",
        [0.9333333333333333, 0.8352941176470589, 0.7176470588235294],
    ),
    (
        "bisque3",
        [0.803921568627451, 0.7176470588235294, 0.6196078431372549],
    ),
    (
        "bisque4",
        [0.5450980392156862, 0.49019607843137253, 0.4196078431372549],
    ),
    ("black", [0.0, 0.0, 0.0]),
    (
        "blanchedalmond",
        [1.0, 0.9215686274509803, 0.803921568627451],
    ),
    ("blue", [0.0, 0.0, 1.0]),
    ("blue1", [0.0, 0.0, 1.0]),
    ("blue2", [0.0, 0.0, 0.9333333333333333]),
    ("blue3", [0.0, 0.0, 0.803921568627451]),
    ("blue4", [0.0, 0.0, 0.5450980392156862]),
    (
        "blueviolet",
        [0.5411764705882353, 0.16862745098039217, 0.8862745098039215],
    ),
    (
        "brown",
        [0.6470588235294118, 0.16470588235294117, 0.16470588235294117],
    ),
    ("brown1", [1.0, 0.25098039215686274, 0.25098039215686274]),
    (
        "brown2",
        [0.9333333333333333, 0.23137254901960785, 0.23137254901960785],
    ),
    ("brown3", [0.803921568627451, 0.2, 0.2]),
    (
        "brown4",
        [0.5450980392156862, 0.13725490196078433, 0.13725490196078433],
    ),
    (
        "burlywood",
        [0.8705882352941177, 0.7215686274509804, 0.5294117647058824],
    ),
    ("burlywood1", [1.0, 0.8274509803921568, 0.6078431372549019]),
    (
        "burlywood2",
        [0.9333333333333333, 0.7725490196078432, 0.5686274509803921],
    ),
    (
        "burlywood3",
        [0.803921568627451, 0.6666666666666666, 0.49019607843137253],
    ),
    (
        "burlywood4",
        [0.5450980392156862, 0.45098039215686275, 0.3333333333333333],
    ),
    (
        "cadetblue",
        [0.37254901960784315, 0.6196078431372549, 0.6274509803921569],
    ),
    ("cadetblue1", [0.596078431372549, 0.9607843137254902, 1.0]),
    (
        "cadetblue2",
        [0.5568627450980392, 0.8980392156862745, 0.9333333333333333],
    ),
    (
        "cadetblue3",
        [0.47843137254901963, 0.7725490196078432, 0.803921568627451],
    ),
    (
        "cadetblue4",
        [0.3254901960784314, 0.5254901960784314, 0.5450980392156862],
    ),
    ("chartreuse", [0.4980392156862745, 1.0, 0.0]),
    ("chartreuse1", [0.4980392156862745, 1.0, 0.0]),
    ("chartreuse2", [0.4627450980392157, 0.9333333333333333, 0.0]),
    ("chartreuse3", [0.4, 0.803921568627451, 0.0]),
    (
        "chartreuse4",
        [0.27058823529411763, 0.5450980392156862, 0.0],
    ),
    (
        "chocolate",
        [0.8235294117647058, 0.4117647058823529, 0.11764705882352941],
    ),
    ("chocolate1", [1.0, 0.4980392156862745, 0.1411764705882353]),
    (
        "chocolate2",
        [0.9333333333333333, 0.4627450980392157, 0.12941176470588237],
    ),
    ("chocolate3", [0.803921568627451, 0.4, 0.11372549019607843]),
    (
        "chocolate4",
        [0.5450980392156862, 0.27058823529411763, 0.07450980392156863],
    ),
    ("coffee", [0.611764705882353, 0.30980392156862746, 0.0]),
    ("coral", [1.0, 0.4980392156862745, 0.3137254901960784]),
    ("coral1", [1.0, 0.4470588235294118, 0.33725490196078434]),
    (
        "coral2",
        [0.9333333333333333, 0.41568627450980394, 0.3137254901960784],
    ),
    (
        "coral3",
        [0.803921568627451, 0.3568627450980392, 0.27058823529411763],
    ),
    (
        "coral4",
        [0.5450980392156862, 0.24313725490196078, 0.1843137254901961],
    ),
    (
        "cornflowerblue",
        [0.39215686274509803, 0.5843137254901961, 0.9294117647058824],
    ),
    ("cornsilk", [1.0, 0.9725490196078431, 0.8627450980392157]),
    ("cornsilk1", [1.0, 0.9725490196078431, 0.8627450980392157]),
    (
        "cornsilk2",
        [0.9333333333333333, 0.9098039215686274, 0.803921568627451],
    ),
    (
        "cornsilk3",
        [0.803921568627451, 0.7843137254901961, 0.6941176470588235],
    ),
    (
        "cornsilk4",
        [0.5450980392156862, 0.5333333333333333, 0.47058823529411764],
    ),
    (
        "crimson",
        [0.8627450980392157, 0.0784313725490196, 0.23529411764705882],
    ),
    ("cyan", [0.0, 1.0, 1.0]),
    ("cyan1", [0.0, 1.0, 1.0]),
    ("cyan2", [0.0, 0.9333333333333333, 0.9333333333333333]),
    ("cyan3", [0.0, 0.803921568627451, 0.803921568627451]),
    ("cyan4", [0.0, 0.5450980392156862, 0.5450980392156862]),
    ("darkblue", [0.0, 0.0, 0.5450980392156862]),
    ("darkcyan", [0.0, 0.5450980392156862, 0.5450980392156862]),
    (
        "darkgoldenrod",
        [0.7215686274509804, 0.5254901960784314, 0.043137254901960784],
    ),
    (
        "darkgoldenrod1",
        [1.0, 0.7254901960784313, 0.058823529411764705],
    ),
    (
        "darkgoldenrod2",
        [0.9333333333333333, 0.6784313725490196, 0.054901960784313725],
    ),
    (
        "darkgoldenrod3",
        [0.803921568627451, 0.5843137254901961, 0.047058823529411764],
    ),
    (
        "darkgoldenrod4",
        [0.5450980392156862, 0.396078431372549, 0.03137254901960784],
    ),
    (
        "darkgray",
        [0.6627450980392157, 0.6627450980392157, 0.6627450980392157],
    ),
    ("darkgreen", [0.0, 0.39215686274509803, 0.0]),
    (
        "darkgrey",
        [0.6627450980392157, 0.6627450980392157, 0.6627450980392157],
    ),
    (
        "darkkhaki",
        [0.7411764705882353, 0.7176470588235294, 0.4196078431372549],
    ),
    ("darkmagenta", [0.5450980392156862, 0.0, 0.5450980392156862]),
    (
        "darkolivegreen",
        [0.3333333333333333, 0.4196078431372549, 0.1843137254901961],
    ),
    (
        "darkolivegreen1",
        [0.792156862745098, 1.0, 0.4392156862745098],
    ),
    (
        "darkolivegreen2",
        [0.7372549019607844, 0.9333333333333333, 0.40784313725490196],
    ),
    (
        "darkolivegreen3",
        [0.6352941176470588, 0.803921568627451, 0.35294117647058826],
    ),
    (
        "darkolivegreen4",
        [0.43137254901960786, 0.5450980392156862, 0.23921568627450981],
    ),
    ("darkorange", [1.0, 0.5490196078431373, 0.0]),
    ("darkorange1", [1.0, 0.4980392156862745, 0.0]),
    ("darkorange2", [0.9333333333333333, 0.4627450980392157, 0.0]),
    ("darkorange3", [0.803921568627451, 0.4, 0.0]),
    (
        "darkorange4",
        [0.5450980392156862, 0.27058823529411763, 0.0],
    ),
    ("darkorchid", [0.6, 0.19607843137254902, 0.8]),
    (
        "darkorchid1",
        [0.7490196078431373, 0.24313725490196078, 1.0],
    ),
    (
        "darkorchid2",
        [0.6980392156862745, 0.22745098039215686, 0.9333333333333333],
    ),
    (
        "darkorchid3",
        [0.6039215686274509, 0.19607843137254902, 0.803921568627451],
    ),
    (
        "darkorchid4",
        [0.40784313725490196, 0.13333333333333333, 0.5450980392156862],
    ),
    ("darkred", [0.5450980392156862, 0.0, 0.0]),
    (
        "darksalmon",
        [0.9137254901960784, 0.5882352941176471, 0.47843137254901963],
    ),
    (
        "darkseagreen",
        [0.5607843137254902, 0.7372549019607844, 0.5607843137254902],
    ),
    (
        "darkseagreen1",
        [0.7568627450980392, 1.0, 0.7568627450980392],
    ),
    (
        "darkseagreen2",
        [0.7058823529411765, 0.9333333333333333, 0.7058823529411765],
    ),
    (
        "darkseagreen3",
        [0.6078431372549019, 0.803921568627451, 0.6078431372549019],
    ),
    (
        "darkseagreen4",
        [0.4117647058823529, 0.5450980392156862, 0.4117647058823529],
    ),
    (
        "darkslateblue",
        [0.2823529411764706, 0.23921568627450981, 0.5450980392156862],
    ),
    (
        "darkslategray",
        [0.1843137254901961, 0.30980392156862746, 0.30980392156862746],
    ),
    (
        "darkslategrey",
        [0.1843137254901961, 0.30980392156862746, 0.30980392156862746],
    ),
    (
        "darkturquoise",
        [0.0, 0.807843137254902, 0.8196078431372549],
    ),
    ("darkviolet", [0.5803921568627451, 0.0, 0.8274509803921568]),
    ("deeppink", [1.0, 0.0784313725490196, 0.5764705882352941]),
    ("deeppink1", [1.0, 0.0784313725490196, 0.5764705882352941]),
    (
        "deeppink2",
        [0.9333333333333333, 0.07058823529411765, 0.5372549019607843],
    ),
    (
        "deeppink3",
        [0.803921568627451, 0.06274509803921569, 0.4627450980392157],
    ),
    (
        "deeppink4",
        [0.5450980392156862, 0.0392156862745098, 0.3137254901960784],
    ),
    ("deepskyblue", [0.0, 0.7490196078431373, 1.0]),
    ("deepskyblue1", [0.0, 0.7490196078431373, 1.0]),
    (
        "deepskyblue2",
        [0.0, 0.6980392156862745, 0.9333333333333333],
    ),
    ("deepskyblue3", [0.0, 0.6039215686274509, 0.803921568627451]),
    (
        "deepskyblue4",
        [0.0, 0.40784313725490196, 0.5450980392156862],
    ),
    (
        "dimgray",
        [0.4117647058823529, 0.4117647058823529, 0.4117647058823529],
    ),
    (
        "dimgrey",
        [0.4117647058823529, 0.4117647058823529, 0.4117647058823529],
    ),
    ("dodgerblue", [0.11764705882352941, 0.5647058823529412, 1.0]),
    (
        "dodgerblue1",
        [0.11764705882352941, 0.5647058823529412, 1.0],
    ),
    (
        "dodgerblue2",
        [0.10980392156862745, 0.5254901960784314, 0.9333333333333333],
    ),
    (
        "dodgerblue3",
        [0.09411764705882353, 0.4549019607843137, 0.803921568627451],
    ),
    (
        "dodgerblue4",
        [0.06274509803921569, 0.3058823529411765, 0.5450980392156862],
    ),
    (
        "firebrick",
        [0.6980392156862745, 0.13333333333333333, 0.13333333333333333],
    ),
    (
        "firebrick1",
        [1.0, 0.18823529411764706, 0.18823529411764706],
    ),
    (
        "firebrick2",
        [0.9333333333333333, 0.17254901960784313, 0.17254901960784313],
    ),
    (
        "firebrick3",
        [0.803921568627451, 0.14901960784313725, 0.14901960784313725],
    ),
    (
        "firebrick4",
        [0.5450980392156862, 0.10196078431372549, 0.10196078431372549],
    ),
    ("floralwhite", [1.0, 0.9803921568627451, 0.9411764705882353]),
    (
        "forestgreen",
        [0.13333333333333333, 0.5450980392156862, 0.13333333333333333],
    ),
    ("fuchsia", [1.0, 0.0, 1.0]),
    (
        "gainsboro",
        [0.8627450980392157, 0.8627450980392157, 0.8627450980392157],
    ),
    ("ghostwhite", [0.9725490196078431, 0.9725490196078431, 1.0]),
    ("gold", [1.0, 0.8431372549019608, 0.0]),
    ("gold1", [1.0, 0.8431372549019608, 0.0]),
    ("gold2", [0.9333333333333333, 0.788235294117647, 0.0]),
    ("gold3", [0.803921568627451, 0.6784313725490196, 0.0]),
    ("gold4", [0.5450980392156862, 0.4588235294117647, 0.0]),
    (
        "goldenrod",
        [0.8549019607843137, 0.6470588235294118, 0.12549019607843137],
    ),
    ("goldenrod1", [1.0, 0.7568627450980392, 0.1450980392156863]),
    (
        "goldenrod2",
        [0.9333333333333333, 0.7058823529411765, 0.13333333333333333],
    ),
    (
        "goldenrod3",
        [0.803921568627451, 0.6078431372549019, 0.11372549019607843],
    ),
    (
        "goldenrod4",
        [0.5450980392156862, 0.4117647058823529, 0.0784313725490196],
    ),
    (
        "gray",
        [0.7450980392156863, 0.7450980392156863, 0.7450980392156863],
    ),
    ("gray0", [0.0, 0.0, 0.0]),
    (
        "gray1",
        [
            0.011764705882352941,
            0.011764705882352941,
            0.011764705882352941,
        ],
    ),
    (
        "gray10",
        [
            0.10196078431372549,
            0.10196078431372549,
            0.10196078431372549,
        ],
    ),
    ("gray100", [1.0, 1.0, 1.0]),
    (
        "gray11",
        [
            0.10980392156862745,
            0.10980392156862745,
            0.10980392156862745,
        ],
    ),
    (
        "gray12",
        [
            0.12156862745098039,
            0.12156862745098039,
            0.12156862745098039,
        ],
    ),
    (
        "gray13",
        [
            0.12941176470588237,
            0.12941176470588237,
            0.12941176470588237,
        ],
    ),
    (
        "gray14",
        [0.1411764705882353, 0.1411764705882353, 0.1411764705882353],
    ),
    (
        "gray15",
        [
            0.14901960784313725,
            0.14901960784313725,
            0.14901960784313725,
        ],
    ),
    (
        "gray16",
        [0.1607843137254902, 0.1607843137254902, 0.1607843137254902],
    ),
    (
        "gray17",
        [
            0.16862745098039217,
            0.16862745098039217,
            0.16862745098039217,
        ],
    ),
    (
        "gray18",
        [0.1803921568627451, 0.1803921568627451, 0.1803921568627451],
    ),
    (
        "gray19",
        [
            0.18823529411764706,
            0.18823529411764706,
            0.18823529411764706,
        ],
    ),
    (
        "gray2",
        [0.0196078431372549, 0.0196078431372549, 0.0196078431372549],
    ),
    ("gray20", [0.2, 0.2, 0.2]),
    (
        "gray21",
        [
            0.21176470588235294,
            0.21176470588235294,
            0.21176470588235294,
        ],
    ),
    (
        "gray22",
        [0.2196078431372549, 0.2196078431372549, 0.2196078431372549],
    ),
    (
        "gray23",
        [
            0.23137254901960785,
            0.23137254901960785,
            0.23137254901960785,
        ],
    ),
    (
        "gray24",
        [
            0.23921568627450981,
            0.23921568627450981,
            0.23921568627450981,
        ],
    ),
    (
        "gray25",
        [
            0.25098039215686274,
            0.25098039215686274,
            0.25098039215686274,
        ],
    ),
    (
        "gray26",
        [
            0.25882352941176473,
            0.25882352941176473,
            0.25882352941176473,
        ],
    ),
    (
        "gray27",
        [
            0.27058823529411763,
            0.27058823529411763,
            0.27058823529411763,
        ],
    ),
    (
        "gray28",
        [0.2784313725490196, 0.2784313725490196, 0.2784313725490196],
    ),
    (
        "gray29",
        [0.2901960784313726, 0.2901960784313726, 0.2901960784313726],
    ),
    (
        "gray3",
        [
            0.03137254901960784,
            0.03137254901960784,
            0.03137254901960784,
        ],
    ),
    (
        "gray30",
        [
            0.30196078431372547,
            0.30196078431372547,
            0.30196078431372547,
        ],
    ),
    (
        "gray31",
        [
            0.30980392156862746,
            0.30980392156862746,
            0.30980392156862746,
        ],
    ),
    (
        "gray32",
        [0.3215686274509804, 0.3215686274509804, 0.3215686274509804],
    ),
    (
        "gray33",
        [
            0.32941176470588235,
            0.32941176470588235,
            0.32941176470588235,
        ],
    ),
    (
        "gray34",
        [0.3411764705882353, 0.3411764705882353, 0.3411764705882353],
    ),
    (
        "gray35",
        [
            0.34901960784313724,
            0.34901960784313724,
            0.34901960784313724,
        ],
    ),
    (
        "gray36",
        [0.3607843137254902, 0.3607843137254902, 0.3607843137254902],
    ),
    (
        "gray37",
        [0.3686274509803922, 0.3686274509803922, 0.3686274509803922],
    ),
    (
        "gray38",
        [0.3803921568627451, 0.3803921568627451, 0.3803921568627451],
    ),
    (
        "gray39",
        [
            0.38823529411764707,
            0.38823529411764707,
            0.38823529411764707,
        ],
    ),
    (
        "gray4",
        [0.0392156862745098, 0.0392156862745098, 0.0392156862745098],
    ),
    ("gray40", [0.4, 0.4, 0.4]),
    (
        "gray41",
        [0.4117647058823529, 0.4117647058823529, 0.4117647058823529],
    ),
    (
        "gray42",
        [0.4196078431372549, 0.4196078431372549, 0.4196078431372549],
    ),
    (
        "gray43",
        [
            0.43137254901960786,
            0.43137254901960786,
            0.43137254901960786,
        ],
    ),
    (
        "gray44",
        [0.4392156862745098, 0.4392156862745098, 0.4392156862745098],
    ),
    (
        "gray45",
        [
            0.45098039215686275,
            0.45098039215686275,
            0.45098039215686275,
        ],
    ),
    (
        "gray46",
        [0.4588235294117647, 0.4588235294117647, 0.4588235294117647],
    ),
    (
        "gray47",
        [
            0.47058823529411764,
            0.47058823529411764,
            0.47058823529411764,
        ],
    ),
    (
        "gray48",
        [
            0.47843137254901963,
            0.47843137254901963,
            0.47843137254901963,
        ],
    ),
    (
        "gray49",
        [
            0.49019607843137253,
            0.49019607843137253,
            0.49019607843137253,
        ],
    ),
    (
        "gray5",
        [
            0.050980392156862744,
            0.050980392156862744,
            0.050980392156862744,
        ],
    ),
    (
        "gray50",
        [0.4980392156862745, 0.4980392156862745, 0.4980392156862745],
    ),
    (
        "gray51",
        [0.5098039215686274, 0.5098039215686274, 0.5098039215686274],
    ),
    (
        "gray52",
        [0.5215686274509804, 0.5215686274509804, 0.5215686274509804],
    ),
    (
        "gray53",
        [0.5294117647058824, 0.5294117647058824, 0.5294117647058824],
    ),
    (
        "gray54",
        [0.5411764705882353, 0.5411764705882353, 0.5411764705882353],
    ),
    (
        "gray55",
        [0.5490196078431373, 0.5490196078431373, 0.5490196078431373],
    ),
    (
        "gray56",
        [0.5607843137254902, 0.5607843137254902, 0.5607843137254902],
    ),
    (
        "gray57",
        [0.5686274509803921, 0.5686274509803921, 0.5686274509803921],
    ),
    (
        "gray58",
        [0.5803921568627451, 0.5803921568627451, 0.5803921568627451],
    ),
    (
        "gray59",
        [0.5882352941176471, 0.5882352941176471, 0.5882352941176471],
    ),
    (
        "gray6",
        [
            0.058823529411764705,
            0.058823529411764705,
            0.058823529411764705,
        ],
    ),
    ("gray60", [0.6, 0.6, 0.6]),
    (
        "gray61",
        [0.611764705882353, 0.611764705882353, 0.611764705882353],
    ),
    (
        "gray62",
        [0.6196078431372549, 0.6196078431372549, 0.6196078431372549],
    ),
    (
        "gray63",
        [0.6313725490196078, 0.6313725490196078, 0.6313725490196078],
    ),
    (
        "gray64",
        [0.6392156862745098, 0.6392156862745098, 0.6392156862745098],
    ),
    (
        "gray65",
        [0.6509803921568628, 0.6509803921568628, 0.6509803921568628],
    ),
    (
        "gray66",
        [0.6588235294117647, 0.6588235294117647, 0.6588235294117647],
    ),
    (
        "gray67",
        [0.6705882352941176, 0.6705882352941176, 0.6705882352941176],
    ),
    (
        "gray68",
        [0.6784313725490196, 0.6784313725490196, 0.6784313725490196],
    ),
    (
        "gray69",
        [0.6901960784313725, 0.6901960784313725, 0.6901960784313725],
    ),
    (
        "gray7",
        [
            0.07058823529411765,
            0.07058823529411765,
            0.07058823529411765,
        ],
    ),
    (
        "gray70",
        [0.7019607843137254, 0.7019607843137254, 0.7019607843137254],
    ),
    (
        "gray71",
        [0.7098039215686275, 0.7098039215686275, 0.7098039215686275],
    ),
    (
        "gray72",
        [0.7215686274509804, 0.7215686274509804, 0.7215686274509804],
    ),
    (
        "gray73",
        [0.7294117647058823, 0.7294117647058823, 0.7294117647058823],
    ),
    (
        "gray74",
        [0.7411764705882353, 0.7411764705882353, 0.7411764705882353],
    ),
    (
        "gray75",
        [0.7490196078431373, 0.7490196078431373, 0.7490196078431373],
    ),
    (
        "gray76",
        [0.7607843137254902, 0.7607843137254902, 0.7607843137254902],
    ),
    (
        "gray77",
        [0.7686274509803922, 0.7686274509803922, 0.7686274509803922],
    ),
    (
        "gray78",
        [0.7803921568627451, 0.7803921568627451, 0.7803921568627451],
    ),
    (
        "gray79",
        [0.788235294117647, 0.788235294117647, 0.788235294117647],
    ),
    (
        "gray8",
        [0.0784313725490196, 0.0784313725490196, 0.0784313725490196],
    ),
    ("gray80", [0.8, 0.8, 0.8]),
    (
        "gray81",
        [0.8117647058823529, 0.8117647058823529, 0.8117647058823529],
    ),
    (
        "gray82",
        [0.8196078431372549, 0.8196078431372549, 0.8196078431372549],
    ),
    (
        "gray83",
        [0.8313725490196079, 0.8313725490196079, 0.8313725490196079],
    ),
    (
        "gray84",
        [0.8392156862745098, 0.8392156862745098, 0.8392156862745098],
    ),
    (
        "gray85",
        [0.8509803921568627, 0.8509803921568627, 0.8509803921568627],
    ),
    (
        "gray86",
        [0.8588235294117647, 0.8588235294117647, 0.8588235294117647],
    ),
    (
        "gray87",
        [0.8705882352941177, 0.8705882352941177, 0.8705882352941177],
    ),
    (
        "gray88",
        [0.8784313725490196, 0.8784313725490196, 0.8784313725490196],
    ),
    (
        "gray89",
        [0.8901960784313725, 0.8901960784313725, 0.8901960784313725],
    ),
    (
        "gray9",
        [
            0.09019607843137255,
            0.09019607843137255,
            0.09019607843137255,
        ],
    ),
    (
        "gray90",
        [0.8980392156862745, 0.8980392156862745, 0.8980392156862745],
    ),
    (
        "gray91",
        [0.9098039215686274, 0.9098039215686274, 0.9098039215686274],
    ),
    (
        "gray92",
        [0.9215686274509803, 0.9215686274509803, 0.9215686274509803],
    ),
    (
        "gray93",
        [0.9294117647058824, 0.9294117647058824, 0.9294117647058824],
    ),
    (
        "gray94",
        [0.9411764705882353, 0.9411764705882353, 0.9411764705882353],
    ),
    (
        "gray95",
        [0.9490196078431372, 0.9490196078431372, 0.9490196078431372],
    ),
    (
        "gray96",
        [0.9607843137254902, 0.9607843137254902, 0.9607843137254902],
    ),
    (
        "gray97",
        [0.9686274509803922, 0.9686274509803922, 0.9686274509803922],
    ),
    (
        "gray98",
        [0.9803921568627451, 0.9803921568627451, 0.9803921568627451],
    ),
    (
        "gray99",
        [0.9882352941176471, 0.9882352941176471, 0.9882352941176471],
    ),
    ("green", [0.0, 1.0, 0.0]),
    (
        "green yellow",
        [0.6784313725490196, 1.0, 0.1843137254901961],
    ),
    ("green1", [0.0, 1.0, 0.0]),
    ("green2", [0.0, 0.9333333333333333, 0.0]),
    ("green3", [0.0, 0.803921568627451, 0.0]),
    ("green4", [0.0, 0.5450980392156862, 0.0]),
    ("greenyellow", [0.6784313725490196, 1.0, 0.1843137254901961]),
    (
        "grey",
        [0.5019607843137255, 0.5019607843137255, 0.5019607843137255],
    ),
    ("honeydew", [0.9411764705882353, 1.0, 0.9411764705882353]),
    ("honeydew1", [0.9411764705882353, 1.0, 0.9411764705882353]),
    (
        "honeydew2",
        [0.8784313725490196, 0.9333333333333333, 0.8784313725490196],
    ),
    (
        "honeydew3",
        [0.7568627450980392, 0.803921568627451, 0.7568627450980392],
    ),
    (
        "honeydew4",
        [0.5137254901960784, 0.5450980392156862, 0.5137254901960784],
    ),
    ("hotpink", [1.0, 0.4117647058823529, 0.7058823529411765]),
    ("hotpink1", [1.0, 0.43137254901960786, 0.7058823529411765]),
    (
        "hotpink2",
        [0.9333333333333333, 0.41568627450980394, 0.6549019607843137],
    ),
    (
        "hotpink3",
        [0.803921568627451, 0.3764705882352941, 0.5647058823529412],
    ),
    (
        "hotpink4",
        [0.5450980392156862, 0.22745098039215686, 0.3843137254901961],
    ),
    (
        "indianred",
        [0.803921568627451, 0.3607843137254902, 0.3607843137254902],
    ),
    (
        "indianred1",
        [1.0, 0.41568627450980394, 0.41568627450980394],
    ),
    (
        "indianred2",
        [0.9333333333333333, 0.38823529411764707, 0.38823529411764707],
    ),
    (
        "indianred3",
        [0.803921568627451, 0.3333333333333333, 0.3333333333333333],
    ),
    (
        "indianred4",
        [0.5450980392156862, 0.22745098039215686, 0.22745098039215686],
    ),
    ("indigo", [0.29411764705882354, 0.0, 0.5098039215686274]),
    ("ivory", [1.0, 1.0, 0.9411764705882353]),
    ("ivory1", [1.0, 1.0, 0.9411764705882353]),
    (
        "ivory2",
        [0.9333333333333333, 0.9333333333333333, 0.8784313725490196],
    ),
    (
        "ivory3",
        [0.803921568627451, 0.803921568627451, 0.7568627450980392],
    ),
    (
        "ivory4",
        [0.5450980392156862, 0.5450980392156862, 0.5137254901960784],
    ),
    (
        "khaki",
        [0.9411764705882353, 0.9019607843137255, 0.5490196078431373],
    ),
    ("khaki1", [1.0, 0.9647058823529412, 0.5607843137254902]),
    (
        "khaki2",
        [0.9333333333333333, 0.9019607843137255, 0.5215686274509804],
    ),
    (
        "khaki3",
        [0.803921568627451, 0.7764705882352941, 0.45098039215686275],
    ),
    (
        "khaki4",
        [0.5450980392156862, 0.5254901960784314, 0.3058823529411765],
    ),
    (
        "lavender",
        [0.9019607843137255, 0.9019607843137255, 0.9803921568627451],
    ),
    (
        "lavenderblush",
        [1.0, 0.9411764705882353, 0.9607843137254902],
    ),
    (
        "lavenderblush1",
        [1.0, 0.9411764705882353, 0.9607843137254902],
    ),
    (
        "lavenderblush2",
        [0.9333333333333333, 0.8784313725490196, 0.8980392156862745],
    ),
    (
        "lavenderblush3",
        [0.803921568627451, 0.7568627450980392, 0.7725490196078432],
    ),
    (
        "lavenderblush4",
        [0.5450980392156862, 0.5137254901960784, 0.5254901960784314],
    ),
    ("lawngreen", [0.48627450980392156, 0.9882352941176471, 0.0]),
    ("lemonchiffon", [1.0, 0.9803921568627451, 0.803921568627451]),
    (
        "lemonchiffon1",
        [1.0, 0.9803921568627451, 0.803921568627451],
    ),
    (
        "lemonchiffon2",
        [0.9333333333333333, 0.9137254901960784, 0.7490196078431373],
    ),
    (
        "lemonchiffon3",
        [0.803921568627451, 0.788235294117647, 0.6470588235294118],
    ),
    (
        "lemonchiffon4",
        [0.5450980392156862, 0.5372549019607843, 0.4392156862745098],
    ),
    (
        "lightblue",
        [0.6784313725490196, 0.8470588235294118, 0.9019607843137255],
    ),
    ("lightblue1", [0.7490196078431373, 0.9372549019607843, 1.0]),
    (
        "lightblue2",
        [0.6980392156862745, 0.8745098039215686, 0.9333333333333333],
    ),
    (
        "lightblue3",
        [0.6039215686274509, 0.7529411764705882, 0.803921568627451],
    ),
    (
        "lightblue4",
        [0.40784313725490196, 0.5137254901960784, 0.5450980392156862],
    ),
    (
        "lightcoral",
        [0.9411764705882353, 0.5019607843137255, 0.5019607843137255],
    ),
    ("lightcyan", [0.8784313725490196, 1.0, 1.0]),
    ("lightcyan1", [0.8784313725490196, 1.0, 1.0]),
    (
        "lightcyan2",
        [0.8196078431372549, 0.9333333333333333, 0.9333333333333333],
    ),
    (
        "lightcyan3",
        [0.7058823529411765, 0.803921568627451, 0.803921568627451],
    ),
    (
        "lightcyan4",
        [0.47843137254901963, 0.5450980392156862, 0.5450980392156862],
    ),
    (
        "lightgoldenrod",
        [0.9333333333333333, 0.8666666666666667, 0.5098039215686274],
    ),
    (
        "lightgoldenrod1",
        [1.0, 0.9254901960784314, 0.5450980392156862],
    ),
    (
        "lightgoldenrod2",
        [0.9333333333333333, 0.8627450980392157, 0.5098039215686274],
    ),
    (
        "lightgoldenrod3",
        [0.803921568627451, 0.7450980392156863, 0.4392156862745098],
    ),
    (
        "lightgoldenrod4",
        [0.5450980392156862, 0.5058823529411764, 0.2980392156862745],
    ),
    (
        "lightgoldenrodyellow",
        [0.9803921568627451, 0.9803921568627451, 0.8235294117647058],
    ),
    (
        "lightgray",
        [0.8274509803921568, 0.8274509803921568, 0.8274509803921568],
    ),
    (
        "lightgreen",
        [0.5647058823529412, 0.9333333333333333, 0.5647058823529412],
    ),
    (
        "lightgrey",
        [0.8274509803921568, 0.8274509803921568, 0.8274509803921568],
    ),
    ("lightpink", [1.0, 0.7137254901960784, 0.7568627450980392]),
    ("lightpink1", [1.0, 0.6823529411764706, 0.7254901960784313]),
    (
        "lightpink2",
        [0.9333333333333333, 0.6352941176470588, 0.6784313725490196],
    ),
    (
        "lightpink3",
        [0.803921568627451, 0.5490196078431373, 0.5843137254901961],
    ),
    (
        "lightpink4",
        [0.5450980392156862, 0.37254901960784315, 0.396078431372549],
    ),
    (
        "lightsalmon",
        [1.0, 0.6274509803921569, 0.47843137254901963],
    ),
    (
        "lightsalmon1",
        [1.0, 0.6274509803921569, 0.47843137254901963],
    ),
    (
        "lightsalmon2",
        [0.9333333333333333, 0.5843137254901961, 0.4470588235294118],
    ),
    (
        "lightsalmon3",
        [0.803921568627451, 0.5058823529411764, 0.3843137254901961],
    ),
    (
        "lightsalmon4",
        [0.5450980392156862, 0.3411764705882353, 0.25882352941176473],
    ),
    (
        "lightseagreen",
        [0.12549019607843137, 0.6980392156862745, 0.6666666666666666],
    ),
    (
        "lightskyblue",
        [0.5294117647058824, 0.807843137254902, 0.9803921568627451],
    ),
    (
        "lightskyblue1",
        [0.6901960784313725, 0.8862745098039215, 1.0],
    ),
    (
        "lightskyblue2",
        [0.6431372549019608, 0.8274509803921568, 0.9333333333333333],
    ),
    (
        "lightskyblue3",
        [0.5529411764705883, 0.7137254901960784, 0.803921568627451],
    ),
    (
        "lightskyblue4",
        [0.3764705882352941, 0.4823529411764706, 0.5450980392156862],
    ),
    (
        "lightslateblue",
        [0.5176470588235295, 0.4392156862745098, 1.0],
    ),
    (
        "lightslategray",
        [0.4666666666666667, 0.5333333333333333, 0.6],
    ),
    (
        "lightslategrey",
        [0.4666666666666667, 0.5333333333333333, 0.6],
    ),
    (
        "lightsteelblue",
        [0.6901960784313725, 0.7686274509803922, 0.8705882352941177],
    ),
    (
        "lightsteelblue1",
        [0.792156862745098, 0.8823529411764706, 1.0],
    ),
    (
        "lightsteelblue2",
        [0.7372549019607844, 0.8235294117647058, 0.9333333333333333],
    ),
    (
        "lightsteelblue3",
        [0.6352941176470588, 0.7098039215686275, 0.803921568627451],
    ),
    (
        "lightsteelblue4",
        [0.43137254901960786, 0.4823529411764706, 0.5450980392156862],
    ),
    ("lightyellow", [1.0, 1.0, 0.8784313725490196]),
    ("lightyellow1", [1.0, 1.0, 0.8784313725490196]),
    (
        "lightyellow2",
        [0.9333333333333333, 0.9333333333333333, 0.8196078431372549],
    ),
    (
        "lightyellow3",
        [0.803921568627451, 0.803921568627451, 0.7058823529411765],
    ),
    (
        "lightyellow4",
        [0.5450980392156862, 0.5450980392156862, 0.47843137254901963],
    ),
    ("lime", [0.0, 1.0, 0.0]),
    (
        "limegreen",
        [0.19607843137254902, 0.803921568627451, 0.19607843137254902],
    ),
    (
        "linen",
        [0.9803921568627451, 0.9411764705882353, 0.9019607843137255],
    ),
    ("magenta", [1.0, 0.0, 1.0]),
    ("magenta1", [1.0, 0.0, 1.0]),
    ("magenta2", [0.9333333333333333, 0.0, 0.9333333333333333]),
    ("magenta3", [0.803921568627451, 0.0, 0.803921568627451]),
    ("magenta4", [0.5450980392156862, 0.0, 0.5450980392156862]),
    (
        "maroon",
        [0.6901960784313725, 0.18823529411764706, 0.3764705882352941],
    ),
    ("maroon1", [1.0, 0.20392156862745098, 0.7019607843137254]),
    (
        "maroon2",
        [0.9333333333333333, 0.18823529411764706, 0.6549019607843137],
    ),
    (
        "maroon3",
        [0.803921568627451, 0.1607843137254902, 0.5647058823529412],
    ),
    (
        "maroon4",
        [0.5450980392156862, 0.10980392156862745, 0.3843137254901961],
    ),
    (
        "mediumaquamarine",
        [0.4, 0.803921568627451, 0.6666666666666666],
    ),
    ("mediumblue", [0.0, 0.0, 0.803921568627451]),
    (
        "mediumorchid",
        [0.7294117647058823, 0.3333333333333333, 0.8274509803921568],
    ),
    ("mediumorchid1", [0.8784313725490196, 0.4, 1.0]),
    (
        "mediumorchid2",
        [0.8196078431372549, 0.37254901960784315, 0.9333333333333333],
    ),
    (
        "mediumorchid3",
        [0.7058823529411765, 0.3215686274509804, 0.803921568627451],
    ),
    (
        "mediumorchid4",
        [0.47843137254901963, 0.21568627450980393, 0.5450980392156862],
    ),
    (
        "mediumpurple",
        [0.5764705882352941, 0.4392156862745098, 0.8588235294117647],
    ),
    (
        "mediumpurple1",
        [0.6705882352941176, 0.5098039215686274, 1.0],
    ),
    (
        "mediumpurple2",
        [0.6235294117647059, 0.4745098039215686, 0.9333333333333333],
    ),
    (
        "mediumpurple3",
        [0.5372549019607843, 0.40784313725490196, 0.803921568627451],
    ),
    (
        "mediumpurple4",
        [0.36470588235294116, 0.2784313725490196, 0.5450980392156862],
    ),
    (
        "mediumseagreen",
        [0.23529411764705882, 0.7019607843137254, 0.44313725490196076],
    ),
    (
        "mediumslateblue",
        [0.4823529411764706, 0.40784313725490196, 0.9333333333333333],
    ),
    (
        "mediumspringgreen",
        [0.0, 0.9803921568627451, 0.6039215686274509],
    ),
    (
        "mediumturquoise",
        [0.2823529411764706, 0.8196078431372549, 0.8],
    ),
    (
        "mediumvioletred",
        [0.7803921568627451, 0.08235294117647059, 0.5215686274509804],
    ),
    (
        "midnightblue",
        [0.09803921568627451, 0.09803921568627451, 0.4392156862745098],
    ),
    ("mintcream", [0.9607843137254902, 1.0, 0.9803921568627451]),
    ("mistyrose", [1.0, 0.8941176470588236, 0.8823529411764706]),
    ("mistyrose1", [1.0, 0.8941176470588236, 0.8823529411764706]),
    (
        "mistyrose2",
        [0.9333333333333333, 0.8352941176470589, 0.8235294117647058],
    ),
    (
        "mistyrose3",
        [0.803921568627451, 0.7176470588235294, 0.7098039215686275],
    ),
    (
        "mistyrose4",
        [0.5450980392156862, 0.49019607843137253, 0.4823529411764706],
    ),
    ("moccasin", [1.0, 0.8941176470588236, 0.7098039215686275]),
    (
        "mupdfblue",
        [0.1450980392156863, 0.4470588235294118, 0.6745098039215687],
    ),
    ("navajowhite", [1.0, 0.8705882352941177, 0.6784313725490196]),
    (
        "navajowhite1",
        [1.0, 0.8705882352941177, 0.6784313725490196],
    ),
    (
        "navajowhite2",
        [0.9333333333333333, 0.8117647058823529, 0.6313725490196078],
    ),
    (
        "navajowhite3",
        [0.803921568627451, 0.7019607843137254, 0.5450980392156862],
    ),
    (
        "navajowhite4",
        [0.5450980392156862, 0.4745098039215686, 0.3686274509803922],
    ),
    ("navy", [0.0, 0.0, 0.5019607843137255]),
    ("navyblue", [0.0, 0.0, 0.5019607843137255]),
    (
        "oldlace",
        [0.9921568627450981, 0.9607843137254902, 0.9019607843137255],
    ),
    ("olive", [0.5019607843137255, 0.5019607843137255, 0.0]),
    (
        "olivedrab",
        [0.4196078431372549, 0.5568627450980392, 0.13725490196078433],
    ),
    ("olivedrab1", [0.7529411764705882, 1.0, 0.24313725490196078]),
    (
        "olivedrab2",
        [0.7019607843137254, 0.9333333333333333, 0.22745098039215686],
    ),
    (
        "olivedrab3",
        [0.6039215686274509, 0.803921568627451, 0.19607843137254902],
    ),
    (
        "olivedrab4",
        [0.4117647058823529, 0.5450980392156862, 0.13333333333333333],
    ),
    ("orange", [1.0, 0.6470588235294118, 0.0]),
    ("orange1", [1.0, 0.6470588235294118, 0.0]),
    ("orange2", [0.9333333333333333, 0.6039215686274509, 0.0]),
    ("orange3", [0.803921568627451, 0.5215686274509804, 0.0]),
    ("orange4", [0.5450980392156862, 0.35294117647058826, 0.0]),
    ("orangered", [1.0, 0.27058823529411763, 0.0]),
    ("orangered1", [1.0, 0.27058823529411763, 0.0]),
    ("orangered2", [0.9333333333333333, 0.25098039215686274, 0.0]),
    ("orangered3", [0.803921568627451, 0.21568627450980393, 0.0]),
    ("orangered4", [0.5450980392156862, 0.1450980392156863, 0.0]),
    (
        "orchid",
        [0.8549019607843137, 0.4392156862745098, 0.8392156862745098],
    ),
    ("orchid1", [1.0, 0.5137254901960784, 0.9803921568627451]),
    (
        "orchid2",
        [0.9333333333333333, 0.47843137254901963, 0.9137254901960784],
    ),
    (
        "orchid3",
        [0.803921568627451, 0.4117647058823529, 0.788235294117647],
    ),
    (
        "orchid4",
        [0.5450980392156862, 0.2784313725490196, 0.5372549019607843],
    ),
    (
        "palegoldenrod",
        [0.9333333333333333, 0.9098039215686274, 0.6666666666666666],
    ),
    (
        "palegreen",
        [0.596078431372549, 0.984313725490196, 0.596078431372549],
    ),
    ("palegreen1", [0.6039215686274509, 1.0, 0.6039215686274509]),
    (
        "palegreen2",
        [0.5647058823529412, 0.9333333333333333, 0.5647058823529412],
    ),
    (
        "palegreen3",
        [0.48627450980392156, 0.803921568627451, 0.48627450980392156],
    ),
    (
        "palegreen4",
        [0.32941176470588235, 0.5450980392156862, 0.32941176470588235],
    ),
    (
        "paleturquoise",
        [0.6862745098039216, 0.9333333333333333, 0.9333333333333333],
    ),
    ("paleturquoise1", [0.7333333333333333, 1.0, 1.0]),
    (
        "paleturquoise2",
        [0.6823529411764706, 0.9333333333333333, 0.9333333333333333],
    ),
    (
        "paleturquoise3",
        [0.5882352941176471, 0.803921568627451, 0.803921568627451],
    ),
    (
        "paleturquoise4",
        [0.4, 0.5450980392156862, 0.5450980392156862],
    ),
    (
        "palevioletred",
        [0.8588235294117647, 0.4392156862745098, 0.5764705882352941],
    ),
    (
        "palevioletred1",
        [1.0, 0.5098039215686274, 0.6705882352941176],
    ),
    (
        "palevioletred2",
        [0.9333333333333333, 0.4745098039215686, 0.6235294117647059],
    ),
    (
        "palevioletred3",
        [0.803921568627451, 0.40784313725490196, 0.5372549019607843],
    ),
    (
        "palevioletred4",
        [0.5450980392156862, 0.2784313725490196, 0.36470588235294116],
    ),
    ("papayawhip", [1.0, 0.9372549019607843, 0.8352941176470589]),
    ("peachpuff", [1.0, 0.8549019607843137, 0.7254901960784313]),
    ("peachpuff1", [1.0, 0.8549019607843137, 0.7254901960784313]),
    (
        "peachpuff2",
        [0.9333333333333333, 0.796078431372549, 0.6784313725490196],
    ),
    (
        "peachpuff3",
        [0.803921568627451, 0.6862745098039216, 0.5843137254901961],
    ),
    (
        "peachpuff4",
        [0.5450980392156862, 0.4666666666666667, 0.396078431372549],
    ),
    (
        "peru",
        [0.803921568627451, 0.5215686274509804, 0.24705882352941178],
    ),
    ("pink", [1.0, 0.7529411764705882, 0.796078431372549]),
    ("pink1", [1.0, 0.7098039215686275, 0.7725490196078432]),
    (
        "pink2",
        [0.9333333333333333, 0.6627450980392157, 0.7215686274509804],
    ),
    (
        "pink3",
        [0.803921568627451, 0.5686274509803921, 0.6196078431372549],
    ),
    (
        "pink4",
        [0.5450980392156862, 0.38823529411764707, 0.4235294117647059],
    ),
    (
        "plum",
        [0.8666666666666667, 0.6274509803921569, 0.8666666666666667],
    ),
    ("plum1", [1.0, 0.7333333333333333, 1.0]),
    (
        "plum2",
        [0.9333333333333333, 0.6823529411764706, 0.9333333333333333],
    ),
    (
        "plum3",
        [0.803921568627451, 0.5882352941176471, 0.803921568627451],
    ),
    ("plum4", [0.5450980392156862, 0.4, 0.5450980392156862]),
    (
        "powderblue",
        [0.6901960784313725, 0.8784313725490196, 0.9019607843137255],
    ),
    (
        "purple",
        [0.6274509803921569, 0.12549019607843137, 0.9411764705882353],
    ),
    ("purple1", [0.6078431372549019, 0.18823529411764706, 1.0]),
    (
        "purple2",
        [0.5686274509803921, 0.17254901960784313, 0.9333333333333333],
    ),
    (
        "purple3",
        [0.49019607843137253, 0.14901960784313725, 0.803921568627451],
    ),
    (
        "purple4",
        [0.3333333333333333, 0.10196078431372549, 0.5450980392156862],
    ),
    ("py_color", [0.9411764705882353, 1.0, 0.8235294117647058]),
    ("red", [1.0, 0.0, 0.0]),
    ("red1", [1.0, 0.0, 0.0]),
    ("red2", [0.9333333333333333, 0.0, 0.0]),
    ("red3", [0.803921568627451, 0.0, 0.0]),
    ("red4", [0.5450980392156862, 0.0, 0.0]),
    (
        "rosybrown",
        [0.7372549019607844, 0.5607843137254902, 0.5607843137254902],
    ),
    ("rosybrown1", [1.0, 0.7568627450980392, 0.7568627450980392]),
    (
        "rosybrown2",
        [0.9333333333333333, 0.7058823529411765, 0.7058823529411765],
    ),
    (
        "rosybrown3",
        [0.803921568627451, 0.6078431372549019, 0.6078431372549019],
    ),
    (
        "rosybrown4",
        [0.5450980392156862, 0.4117647058823529, 0.4117647058823529],
    ),
    (
        "royalblue",
        [0.2549019607843137, 0.4117647058823529, 0.8823529411764706],
    ),
    ("royalblue1", [0.2823529411764706, 0.4627450980392157, 1.0]),
    (
        "royalblue2",
        [0.2627450980392157, 0.43137254901960786, 0.9333333333333333],
    ),
    (
        "royalblue3",
        [0.22745098039215686, 0.37254901960784315, 0.803921568627451],
    ),
    (
        "royalblue4",
        [0.15294117647058825, 0.25098039215686274, 0.5450980392156862],
    ),
    (
        "saddlebrown",
        [0.5450980392156862, 0.27058823529411763, 0.07450980392156863],
    ),
    (
        "salmon",
        [0.9803921568627451, 0.5019607843137255, 0.4470588235294118],
    ),
    ("salmon1", [1.0, 0.5490196078431373, 0.4117647058823529]),
    (
        "salmon2",
        [0.9333333333333333, 0.5098039215686274, 0.3843137254901961],
    ),
    (
        "salmon3",
        [0.803921568627451, 0.4392156862745098, 0.32941176470588235],
    ),
    (
        "salmon4",
        [0.5450980392156862, 0.2980392156862745, 0.2235294117647059],
    ),
    (
        "sandybrown",
        [0.9568627450980393, 0.6431372549019608, 0.3764705882352941],
    ),
    (
        "seagreen",
        [0.1803921568627451, 0.5450980392156862, 0.3411764705882353],
    ),
    ("seagreen1", [0.32941176470588235, 1.0, 0.6235294117647059]),
    (
        "seagreen2",
        [0.3058823529411765, 0.9333333333333333, 0.5803921568627451],
    ),
    (
        "seagreen3",
        [0.2627450980392157, 0.803921568627451, 0.5019607843137255],
    ),
    (
        "seagreen4",
        [0.1803921568627451, 0.5450980392156862, 0.3411764705882353],
    ),
    ("seashell", [1.0, 0.9607843137254902, 0.9333333333333333]),
    ("seashell1", [1.0, 0.9607843137254902, 0.9333333333333333]),
    (
        "seashell2",
        [0.9333333333333333, 0.8980392156862745, 0.8705882352941177],
    ),
    (
        "seashell3",
        [0.803921568627451, 0.7725490196078432, 0.7490196078431373],
    ),
    (
        "seashell4",
        [0.5450980392156862, 0.5254901960784314, 0.5098039215686274],
    ),
    (
        "sienna",
        [0.6274509803921569, 0.3215686274509804, 0.17647058823529413],
    ),
    ("sienna1", [1.0, 0.5098039215686274, 0.2784313725490196]),
    (
        "sienna2",
        [0.9333333333333333, 0.4745098039215686, 0.25882352941176473],
    ),
    (
        "sienna3",
        [0.803921568627451, 0.40784313725490196, 0.2235294117647059],
    ),
    (
        "sienna4",
        [0.5450980392156862, 0.2784313725490196, 0.14901960784313725],
    ),
    (
        "silver",
        [0.7529411764705882, 0.7529411764705882, 0.7529411764705882],
    ),
    (
        "skyblue",
        [0.5294117647058824, 0.807843137254902, 0.9215686274509803],
    ),
    ("skyblue1", [0.5294117647058824, 0.807843137254902, 1.0]),
    (
        "skyblue2",
        [0.49411764705882355, 0.7529411764705882, 0.9333333333333333],
    ),
    (
        "skyblue3",
        [0.4235294117647059, 0.6509803921568628, 0.803921568627451],
    ),
    (
        "skyblue4",
        [0.2901960784313726, 0.4392156862745098, 0.5450980392156862],
    ),
    (
        "slateblue",
        [0.41568627450980394, 0.35294117647058826, 0.803921568627451],
    ),
    ("slateblue1", [0.5137254901960784, 0.43529411764705883, 1.0]),
    (
        "slateblue2",
        [0.47843137254901963, 0.403921568627451, 0.9333333333333333],
    ),
    (
        "slateblue3",
        [0.4117647058823529, 0.34901960784313724, 0.803921568627451],
    ),
    (
        "slateblue4",
        [0.2784313725490196, 0.23529411764705882, 0.5450980392156862],
    ),
    (
        "slategray",
        [0.4392156862745098, 0.5019607843137255, 0.5647058823529412],
    ),
    (
        "slategrey",
        [0.4392156862745098, 0.5019607843137255, 0.5647058823529412],
    ),
    ("snow", [1.0, 0.9803921568627451, 0.9803921568627451]),
    ("snow1", [1.0, 0.9803921568627451, 0.9803921568627451]),
    (
        "snow2",
        [0.9333333333333333, 0.9137254901960784, 0.9137254901960784],
    ),
    (
        "snow3",
        [0.803921568627451, 0.788235294117647, 0.788235294117647],
    ),
    (
        "snow4",
        [0.5450980392156862, 0.5372549019607843, 0.5372549019607843],
    ),
    ("springgreen", [0.0, 1.0, 0.4980392156862745]),
    ("springgreen1", [0.0, 1.0, 0.4980392156862745]),
    (
        "springgreen2",
        [0.0, 0.9333333333333333, 0.4627450980392157],
    ),
    ("springgreen3", [0.0, 0.803921568627451, 0.4]),
    (
        "springgreen4",
        [0.0, 0.5450980392156862, 0.27058823529411763],
    ),
    (
        "steelblue",
        [0.27450980392156865, 0.5098039215686274, 0.7058823529411765],
    ),
    ("steelblue1", [0.38823529411764707, 0.7215686274509804, 1.0]),
    (
        "steelblue2",
        [0.3607843137254902, 0.6745098039215687, 0.9333333333333333],
    ),
    (
        "steelblue3",
        [0.30980392156862746, 0.5803921568627451, 0.803921568627451],
    ),
    (
        "steelblue4",
        [0.21176470588235294, 0.39215686274509803, 0.5450980392156862],
    ),
    (
        "tan",
        [0.8235294117647058, 0.7058823529411765, 0.5490196078431373],
    ),
    ("tan1", [1.0, 0.6470588235294118, 0.30980392156862746]),
    (
        "tan2",
        [0.9333333333333333, 0.6039215686274509, 0.28627450980392155],
    ),
    (
        "tan3",
        [0.803921568627451, 0.5215686274509804, 0.24705882352941178],
    ),
    (
        "tan4",
        [0.5450980392156862, 0.35294117647058826, 0.16862745098039217],
    ),
    ("teal", [0.0, 0.5019607843137255, 0.5019607843137255]),
    (
        "thistle",
        [0.8470588235294118, 0.7490196078431373, 0.8470588235294118],
    ),
    ("thistle1", [1.0, 0.8823529411764706, 1.0]),
    (
        "thistle2",
        [0.9333333333333333, 0.8235294117647058, 0.9333333333333333],
    ),
    (
        "thistle3",
        [0.803921568627451, 0.7098039215686275, 0.803921568627451],
    ),
    (
        "thistle4",
        [0.5450980392156862, 0.4823529411764706, 0.5450980392156862],
    ),
    ("tomato", [1.0, 0.38823529411764707, 0.2784313725490196]),
    ("tomato1", [1.0, 0.38823529411764707, 0.2784313725490196]),
    (
        "tomato2",
        [0.9333333333333333, 0.3607843137254902, 0.25882352941176473],
    ),
    (
        "tomato3",
        [0.803921568627451, 0.30980392156862746, 0.2235294117647059],
    ),
    (
        "tomato4",
        [0.5450980392156862, 0.21176470588235294, 0.14901960784313725],
    ),
    (
        "turquoise",
        [0.25098039215686274, 0.8784313725490196, 0.8156862745098039],
    ),
    ("turquoise1", [0.0, 0.9607843137254902, 1.0]),
    ("turquoise2", [0.0, 0.8980392156862745, 0.9333333333333333]),
    ("turquoise3", [0.0, 0.7725490196078432, 0.803921568627451]),
    ("turquoise4", [0.0, 0.5254901960784314, 0.5450980392156862]),
    (
        "violet",
        [0.9333333333333333, 0.5098039215686274, 0.9333333333333333],
    ),
    (
        "violetred",
        [0.8156862745098039, 0.12549019607843137, 0.5647058823529412],
    ),
    ("violetred1", [1.0, 0.24313725490196078, 0.5882352941176471]),
    (
        "violetred2",
        [0.9333333333333333, 0.22745098039215686, 0.5490196078431373],
    ),
    (
        "violetred3",
        [0.803921568627451, 0.19607843137254902, 0.47058823529411764],
    ),
    (
        "violetred4",
        [0.5450980392156862, 0.13333333333333333, 0.3215686274509804],
    ),
    (
        "wheat",
        [0.9607843137254902, 0.8705882352941177, 0.7019607843137254],
    ),
    ("wheat1", [1.0, 0.9058823529411765, 0.7294117647058823]),
    (
        "wheat2",
        [0.9333333333333333, 0.8470588235294118, 0.6823529411764706],
    ),
    (
        "wheat3",
        [0.803921568627451, 0.7294117647058823, 0.5882352941176471],
    ),
    ("wheat4", [0.5450980392156862, 0.49411764705882355, 0.4]),
    ("white", [1.0, 1.0, 1.0]),
    (
        "whitesmoke",
        [0.9607843137254902, 0.9607843137254902, 0.9607843137254902],
    ),
    ("yellow", [1.0, 1.0, 0.0]),
    ("yellow1", [1.0, 1.0, 0.0]),
    ("yellow2", [0.9333333333333333, 0.9333333333333333, 0.0]),
    ("yellow3", [0.803921568627451, 0.803921568627451, 0.0]),
    ("yellow4", [0.5450980392156862, 0.5450980392156862, 0.0]),
    (
        "yellowgreen",
        [0.6039215686274509, 0.803921568627451, 0.19607843137254902],
    ),
];

// Document and parsing miscellany (50).
/// Document value.
pub const INVALID_NAME_CHARS: &[&str] = &[
    "\0", "\t", "\n", "\x0B", "\x0C", "\r", " ", "%", "(", ")", "/", "<", ">", "[", "]", "{", "}",
];
/// Document value.
pub const PDF_ENUM_FALSE: i32 = 2;
/// Document value.
pub const PDF_ENUM_LIMIT: i32 = 611;
/// Document value.
pub const PDF_ENUM_NULL: i32 = 0;
/// Document value.
pub const PDF_ENUM_TRUE: i32 = 1;
/// Document value.
pub const PDF_LEXBUF_LARGE: i32 = 65536;
/// Document value.
pub const PDF_LEXBUF_SMALL: i32 = 256;
/// Document value.
pub const PDF_MAX_GEN_NUMBER: i32 = 65535;
/// Document value.
pub const PDF_MAX_OBJECT_NUMBER: i32 = 8388607;
/// Document value.
pub const PDF_MRANGE_CAP: i32 = 256;
/// Document value.
pub const PDF_NUM_TOKENS: i32 = 25;
/// Document value.
pub const PDF_PROCESSOR_REQUIRES_DECODED_IMAGES: i32 = 1;
/// Document value.
pub const PDF_TOK_CLOSE_ARRAY: i32 = 3;
/// Document value.
pub const PDF_TOK_CLOSE_BRACE: i32 = 7;
/// Document value.
pub const PDF_TOK_CLOSE_DICT: i32 = 5;
/// Document value.
pub const PDF_TOK_ENDOBJ: i32 = 18;
/// Document value.
pub const PDF_TOK_ENDSTREAM: i32 = 20;
/// Document value.
pub const PDF_TOK_EOF: i32 = 1;
/// Document value.
pub const PDF_TOK_ERROR: i32 = 0;
/// Document value.
pub const PDF_TOK_FALSE: i32 = 15;
/// Document value.
pub const PDF_TOK_INT: i32 = 9;
/// Document value.
pub const PDF_TOK_KEYWORD: i32 = 12;
/// Document value.
pub const PDF_TOK_NAME: i32 = 8;
/// Document value.
pub const PDF_TOK_NEWOBJ: i32 = 24;
/// Document value.
pub const PDF_TOK_NULL: i32 = 16;
/// Document value.
pub const PDF_TOK_OBJ: i32 = 17;
/// Document value.
pub const PDF_TOK_OPEN_ARRAY: i32 = 2;
/// Document value.
pub const PDF_TOK_OPEN_BRACE: i32 = 6;
/// Document value.
pub const PDF_TOK_OPEN_DICT: i32 = 4;
/// Document value.
pub const PDF_TOK_R: i32 = 13;
/// Document value.
pub const PDF_TOK_REAL: i32 = 10;
/// Document value.
pub const PDF_TOK_STARTXREF: i32 = 23;
/// Document value.
pub const PDF_TOK_STREAM: i32 = 19;
/// Document value.
pub const PDF_TOK_STRING: i32 = 11;
/// Document value.
pub const PDF_TOK_TRAILER: i32 = 22;
/// Document value.
pub const PDF_TOK_TRUE: i32 = 14;
/// Document value.
pub const PDF_TOK_XREF: i32 = 21;
/// Document value.
pub const b: i32 = 50;
/// Document value.
pub const f: &str = "ZapfDingbats";
/// Document value.
pub const g: i32 = 205;
/// Document value.
pub const matrix_like: &str = "matrix_like";
/// Document value.
pub const name: &str = "YELLOWGREEN";
/// Document value.
pub const point_like: &str = "point_like";
/// Document value.
pub const quad_like: &str = "quad_like";
/// Document value.
pub const r: i32 = 154;
/// Document value.
pub const rect_like: &str = "rect_like";
/// Document value.
pub const trace_device_CLIP_PATH: i32 = 3;
/// Document value.
pub const trace_device_CLIP_STROKE_PATH: i32 = 4;
/// Document value.
pub const trace_device_FILL_PATH: i32 = 1;
/// Document value.
pub const trace_device_STROKE_PATH: i32 = 2;
