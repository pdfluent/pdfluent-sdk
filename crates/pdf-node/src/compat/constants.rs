//! Compatible named constants.
#![allow(non_upper_case_globals)]
// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use super::geometry::Matrix;
use super::pixmap::Colorspace;
use super::*;
use napi_derive::napi;
use pdfluent::compat as fc;

#[napi]
pub const EPSILON: f64 = fc::constants::EPSILON;

#[napi]
pub const FLT_EPSILON: f64 = fc::constants::FLT_EPSILON;

#[napi]
pub const FZ_MAX_INF_RECT: i32 = fc::constants::FZ_MAX_INF_RECT;

#[napi]
pub const FZ_MIN_INF_RECT: i32 = fc::constants::FZ_MIN_INF_RECT;

#[napi]
pub const TEXT_ACCURATE_ASCENDERS: i32 = fc::constants::TEXT_ACCURATE_ASCENDERS;

#[napi]
pub const TEXT_ACCURATE_BBOXES: i32 = fc::constants::TEXT_ACCURATE_BBOXES;

#[napi]
pub const TEXT_ACCURATE_SIDE_BEARINGS: i32 = fc::constants::TEXT_ACCURATE_SIDE_BEARINGS;

#[napi]
pub const TEXT_ALIGN_CENTER: i32 = fc::constants::TEXT_ALIGN_CENTER;

#[napi]
pub const TEXT_ALIGN_JUSTIFY: i32 = fc::constants::TEXT_ALIGN_JUSTIFY;

#[napi]
pub const TEXT_ALIGN_LEFT: i32 = fc::constants::TEXT_ALIGN_LEFT;

#[napi]
pub const TEXT_ALIGN_RIGHT: i32 = fc::constants::TEXT_ALIGN_RIGHT;

#[napi]
pub const TEXT_CID_FOR_UNKNOWN_UNICODE: i32 = fc::constants::TEXT_CID_FOR_UNKNOWN_UNICODE;

#[napi]
pub const TEXT_CLIP: i32 = fc::constants::TEXT_CLIP;

#[napi]
pub const TEXT_CLIP_RECT: i32 = fc::constants::TEXT_CLIP_RECT;

#[napi]
pub const TEXT_COLLECT_STRUCTURE: i32 = fc::constants::TEXT_COLLECT_STRUCTURE;

#[napi]
pub const TEXT_COLLECT_STYLES: i32 = fc::constants::TEXT_COLLECT_STYLES;

#[napi]
pub const TEXT_COLLECT_VECTORS: i32 = fc::constants::TEXT_COLLECT_VECTORS;

#[napi]
pub const TEXT_DEHYPHENATE: i32 = fc::constants::TEXT_DEHYPHENATE;

#[napi]
pub const TEXT_ENCODING_CYRILLIC: i32 = fc::constants::TEXT_ENCODING_CYRILLIC;

#[napi]
pub const TEXT_ENCODING_GREEK: i32 = fc::constants::TEXT_ENCODING_GREEK;

#[napi]
pub const TEXT_ENCODING_LATIN: i32 = fc::constants::TEXT_ENCODING_LATIN;

#[napi]
pub const TEXT_FONT_BOLD: i32 = fc::constants::TEXT_FONT_BOLD;

#[napi]
pub const TEXT_FONT_ITALIC: i32 = fc::constants::TEXT_FONT_ITALIC;

#[napi]
pub const TEXT_FONT_MONOSPACED: i32 = fc::constants::TEXT_FONT_MONOSPACED;

#[napi]
pub const TEXT_FONT_SERIFED: i32 = fc::constants::TEXT_FONT_SERIFED;

#[napi]
pub const TEXT_FONT_SUPERSCRIPT: i32 = fc::constants::TEXT_FONT_SUPERSCRIPT;

#[napi]
pub const TEXT_FUZZY_VECTORS: i32 = fc::constants::TEXT_FUZZY_VECTORS;

#[napi]
pub const TEXT_IGNORE_ACTUALTEXT: i32 = fc::constants::TEXT_IGNORE_ACTUALTEXT;

#[napi]
pub const TEXT_INHIBIT_SPACES: i32 = fc::constants::TEXT_INHIBIT_SPACES;

#[napi]
pub const TEXT_LAZY_VECTORS: i32 = fc::constants::TEXT_LAZY_VECTORS;

#[napi]
pub const TEXT_MEDIABOX_CLIP: i32 = fc::constants::TEXT_MEDIABOX_CLIP;

#[napi]
pub const TEXT_OUTPUT_HTML: i32 = fc::constants::TEXT_OUTPUT_HTML;

#[napi]
pub const TEXT_OUTPUT_JSON: i32 = fc::constants::TEXT_OUTPUT_JSON;

#[napi]
pub const TEXT_OUTPUT_TEXT: i32 = fc::constants::TEXT_OUTPUT_TEXT;

#[napi]
pub const TEXT_OUTPUT_XHTML: i32 = fc::constants::TEXT_OUTPUT_XHTML;

#[napi]
pub const TEXT_OUTPUT_XML: i32 = fc::constants::TEXT_OUTPUT_XML;

#[napi]
pub const TEXT_PARAGRAPH_BREAK: i32 = fc::constants::TEXT_PARAGRAPH_BREAK;

#[napi]
pub const TEXT_PRESERVE_IMAGES: i32 = fc::constants::TEXT_PRESERVE_IMAGES;

#[napi]
pub const TEXT_PRESERVE_LIGATURES: i32 = fc::constants::TEXT_PRESERVE_LIGATURES;

#[napi]
pub const TEXT_PRESERVE_SPANS: i32 = fc::constants::TEXT_PRESERVE_SPANS;

#[napi]
pub const TEXT_PRESERVE_WHITESPACE: i32 = fc::constants::TEXT_PRESERVE_WHITESPACE;

#[napi]
pub const TEXT_SEGMENT: i32 = fc::constants::TEXT_SEGMENT;

#[napi]
pub const TEXT_STEXT_SEGMENT: i32 = fc::constants::TEXT_STEXT_SEGMENT;

#[napi]
pub const TEXT_TABLE_HUNT: i32 = fc::constants::TEXT_TABLE_HUNT;

#[napi]
pub const TEXT_USE_CID_FOR_UNKNOWN_UNICODE: i32 = fc::constants::TEXT_USE_CID_FOR_UNKNOWN_UNICODE;

#[napi]
pub const TEXT_USE_GID_FOR_UNKNOWN_UNICODE: i32 = fc::constants::TEXT_USE_GID_FOR_UNKNOWN_UNICODE;

#[napi]
pub const TEXTFLAGS_BLOCKS: i32 = fc::constants::TEXTFLAGS_BLOCKS;

#[napi]
pub const TEXTFLAGS_DICT: i32 = fc::constants::TEXTFLAGS_DICT;

#[napi]
pub const TEXTFLAGS_HTML: i32 = fc::constants::TEXTFLAGS_HTML;

#[napi]
pub const TEXTFLAGS_RAWDICT: i32 = fc::constants::TEXTFLAGS_RAWDICT;

#[napi]
pub const TEXTFLAGS_SEARCH: i32 = fc::constants::TEXTFLAGS_SEARCH;

#[napi]
pub const TEXTFLAGS_TEXT: i32 = fc::constants::TEXTFLAGS_TEXT;

#[napi]
pub const TEXTFLAGS_WORDS: i32 = fc::constants::TEXTFLAGS_WORDS;

#[napi]
pub const TEXTFLAGS_XHTML: i32 = fc::constants::TEXTFLAGS_XHTML;

#[napi]
pub const TEXTFLAGS_XML: i32 = fc::constants::TEXTFLAGS_XML;

#[napi]
pub const dictkey_align: &str = fc::constants::dictkey_align;

#[napi]
pub const dictkey_asc: &str = fc::constants::dictkey_asc;

#[napi]
pub const dictkey_bbox: &str = fc::constants::dictkey_bbox;

#[napi]
pub const dictkey_bidi: &str = fc::constants::dictkey_bidi;

#[napi]
pub const dictkey_blocks: &str = fc::constants::dictkey_blocks;

#[napi]
pub const dictkey_bpc: &str = fc::constants::dictkey_bpc;

#[napi]
pub const dictkey_c: &str = fc::constants::dictkey_c;

#[napi]
pub const dictkey_char_flags: &str = fc::constants::dictkey_char_flags;

#[napi]
pub const dictkey_chars: &str = fc::constants::dictkey_chars;

#[napi]
pub const dictkey_color: &str = fc::constants::dictkey_color;

#[napi]
pub const dictkey_colorspace: &str = fc::constants::dictkey_colorspace;

#[napi]
pub const dictkey_content: &str = fc::constants::dictkey_content;

#[napi]
pub const dictkey_creationDate: &str = fc::constants::dictkey_creationDate;

#[napi]
pub const dictkey_cs_name: &str = fc::constants::dictkey_cs_name;

#[napi]
pub const dictkey_da: &str = fc::constants::dictkey_da;

#[napi]
pub const dictkey_dashes: &str = fc::constants::dictkey_dashes;

#[napi]
pub const dictkey_desc: &str = fc::constants::dictkey_desc;

#[napi]
pub const dictkey_descr: &str = fc::constants::dictkey_descr;

#[napi]
pub const dictkey_dir: &str = fc::constants::dictkey_dir;

#[napi]
pub const dictkey_effect: &str = fc::constants::dictkey_effect;

#[napi]
pub const dictkey_ext: &str = fc::constants::dictkey_ext;

#[napi]
pub const dictkey_filename: &str = fc::constants::dictkey_filename;

#[napi]
pub const dictkey_fill: &str = fc::constants::dictkey_fill;

#[napi]
pub const dictkey_flags: &str = fc::constants::dictkey_flags;

#[napi]
pub const dictkey_font: &str = fc::constants::dictkey_font;

#[napi]
pub const dictkey_glyph: &str = fc::constants::dictkey_glyph;

#[napi]
pub const dictkey_height: &str = fc::constants::dictkey_height;

#[napi]
pub const dictkey_id: &str = fc::constants::dictkey_id;

#[napi]
pub const dictkey_image: &str = fc::constants::dictkey_image;

#[napi]
pub const dictkey_items: &str = fc::constants::dictkey_items;

#[napi]
pub const dictkey_length: &str = fc::constants::dictkey_length;

#[napi]
pub const dictkey_lines: &str = fc::constants::dictkey_lines;

#[napi]
pub const dictkey_matrix: &str = fc::constants::dictkey_matrix;

#[napi]
pub const dictkey_modDate: &str = fc::constants::dictkey_modDate;

#[napi]
pub const dictkey_name: &str = fc::constants::dictkey_name;

#[napi]
pub const dictkey_number: &str = fc::constants::dictkey_number;

#[napi]
pub const dictkey_origin: &str = fc::constants::dictkey_origin;

#[napi]
pub const dictkey_rect: &str = fc::constants::dictkey_rect;

#[napi]
pub const dictkey_size: &str = fc::constants::dictkey_size;

#[napi]
pub const dictkey_smask: &str = fc::constants::dictkey_smask;

#[napi]
pub const dictkey_spans: &str = fc::constants::dictkey_spans;

#[napi]
pub const dictkey_stroke: &str = fc::constants::dictkey_stroke;

#[napi]
pub const dictkey_style: &str = fc::constants::dictkey_style;

#[napi]
pub const dictkey_subject: &str = fc::constants::dictkey_subject;

#[napi]
pub const dictkey_text: &str = fc::constants::dictkey_text;

#[napi]
pub const dictkey_title: &str = fc::constants::dictkey_title;

#[napi]
pub const dictkey_type: &str = fc::constants::dictkey_type;

#[napi]
pub const dictkey_ufilename: &str = fc::constants::dictkey_ufilename;

#[napi]
pub const dictkey_width: &str = fc::constants::dictkey_width;

#[napi]
pub const dictkey_wmode: &str = fc::constants::dictkey_wmode;

#[napi]
pub const dictkey_xref: &str = fc::constants::dictkey_xref;

#[napi]
pub const dictkey_xres: &str = fc::constants::dictkey_xres;

#[napi]
pub const dictkey_yres: &str = fc::constants::dictkey_yres;

#[napi]
pub const PDF_STRUCT_BROKEN: i32 = fc::constants::PDF_STRUCT_BROKEN;

#[napi]
pub const PDF_STRUCT_FIXED: i32 = fc::constants::PDF_STRUCT_FIXED;

#[napi]
pub const PDF_STRUCT_HAS_ATTRIBUTES: i32 = fc::constants::PDF_STRUCT_HAS_ATTRIBUTES;

#[napi]
pub const PDF_STRUCT_HAS_CYCLE: i32 = fc::constants::PDF_STRUCT_HAS_CYCLE;

#[napi]
pub const PDF_STRUCT_HAS_TABLE_ATTRIBUTES: i32 = fc::constants::PDF_STRUCT_HAS_TABLE_ATTRIBUTES;

#[napi]
pub const PDF_STRUCT_HAS_TABLE_SPAN_ATTRIBUTES: i32 =
    fc::constants::PDF_STRUCT_HAS_TABLE_SPAN_ATTRIBUTES;

#[napi]
pub const PDF_STRUCT_NOT_PRESENT: i32 = fc::constants::PDF_STRUCT_NOT_PRESENT;

#[napi]
pub const PDF_STRUCT_PRESENT: i32 = fc::constants::PDF_STRUCT_PRESENT;

#[napi]
pub const LINK_FLAG_B_VALID: i32 = fc::constants::LINK_FLAG_B_VALID;

#[napi]
pub const LINK_FLAG_FIT_H: i32 = fc::constants::LINK_FLAG_FIT_H;

#[napi]
pub const LINK_FLAG_FIT_V: i32 = fc::constants::LINK_FLAG_FIT_V;

#[napi]
pub const LINK_FLAG_L_VALID: i32 = fc::constants::LINK_FLAG_L_VALID;

#[napi]
pub const LINK_FLAG_R_IS_ZOOM: i32 = fc::constants::LINK_FLAG_R_IS_ZOOM;

#[napi]
pub const LINK_FLAG_R_VALID: i32 = fc::constants::LINK_FLAG_R_VALID;

#[napi]
pub const LINK_FLAG_T_VALID: i32 = fc::constants::LINK_FLAG_T_VALID;

#[napi]
pub const LINK_GOTO: i32 = fc::constants::LINK_GOTO;

#[napi]
pub const LINK_GOTOR: i32 = fc::constants::LINK_GOTOR;

#[napi]
pub const LINK_LAUNCH: i32 = fc::constants::LINK_LAUNCH;

#[napi]
pub const LINK_NAMED: i32 = fc::constants::LINK_NAMED;

#[napi]
pub const LINK_NONE: i32 = fc::constants::LINK_NONE;

#[napi]
pub const LINK_URI: i32 = fc::constants::LINK_URI;

#[napi]
pub const PDF_ENCRYPT_AES_128: i32 = fc::constants::PDF_ENCRYPT_AES_128;

#[napi]
pub const PDF_ENCRYPT_AES_256: i32 = fc::constants::PDF_ENCRYPT_AES_256;

#[napi]
pub const PDF_ENCRYPT_KEEP: i32 = fc::constants::PDF_ENCRYPT_KEEP;

#[napi]
pub const PDF_ENCRYPT_NONE: i32 = fc::constants::PDF_ENCRYPT_NONE;

#[napi]
pub const PDF_ENCRYPT_RC4_128: i32 = fc::constants::PDF_ENCRYPT_RC4_128;

#[napi]
pub const PDF_ENCRYPT_RC4_40: i32 = fc::constants::PDF_ENCRYPT_RC4_40;

#[napi]
pub const PDF_ENCRYPT_UNKNOWN: i32 = fc::constants::PDF_ENCRYPT_UNKNOWN;

#[napi]
pub const PDF_PERM_ACCESSIBILITY: i32 = fc::constants::PDF_PERM_ACCESSIBILITY;

#[napi]
pub const PDF_PERM_ANNOTATE: i32 = fc::constants::PDF_PERM_ANNOTATE;

#[napi]
pub const PDF_PERM_ASSEMBLE: i32 = fc::constants::PDF_PERM_ASSEMBLE;

#[napi]
pub const PDF_PERM_COPY: i32 = fc::constants::PDF_PERM_COPY;

#[napi]
pub const PDF_PERM_FORM: i32 = fc::constants::PDF_PERM_FORM;

#[napi]
pub const PDF_PERM_MODIFY: i32 = fc::constants::PDF_PERM_MODIFY;

#[napi]
pub const PDF_PERM_PRINT: i32 = fc::constants::PDF_PERM_PRINT;

#[napi]
pub const PDF_PERM_PRINT_HQ: i32 = fc::constants::PDF_PERM_PRINT_HQ;

#[napi]
pub const PDF_PAGE_LABEL_ALPHA_LC: &str = fc::constants::PDF_PAGE_LABEL_ALPHA_LC;

#[napi]
pub const PDF_PAGE_LABEL_ALPHA_UC: &str = fc::constants::PDF_PAGE_LABEL_ALPHA_UC;

#[napi]
pub const PDF_PAGE_LABEL_DECIMAL: &str = fc::constants::PDF_PAGE_LABEL_DECIMAL;

#[napi]
pub const PDF_PAGE_LABEL_NONE: i32 = fc::constants::PDF_PAGE_LABEL_NONE;

#[napi]
pub const PDF_PAGE_LABEL_ROMAN_LC: &str = fc::constants::PDF_PAGE_LABEL_ROMAN_LC;

#[napi]
pub const PDF_PAGE_LABEL_ROMAN_UC: &str = fc::constants::PDF_PAGE_LABEL_ROMAN_UC;

#[napi]
pub const PDF_SIGNATURE_DEFAULT_APPEARANCE: i32 = fc::constants::PDF_SIGNATURE_DEFAULT_APPEARANCE;

#[napi]
pub const PDF_SIGNATURE_ERROR_DIGEST_FAILURE: i32 =
    fc::constants::PDF_SIGNATURE_ERROR_DIGEST_FAILURE;

#[napi]
pub const PDF_SIGNATURE_ERROR_NOT_SIGNED: i32 = fc::constants::PDF_SIGNATURE_ERROR_NOT_SIGNED;

#[napi]
pub const PDF_SIGNATURE_ERROR_NOT_TRUSTED: i32 = fc::constants::PDF_SIGNATURE_ERROR_NOT_TRUSTED;

#[napi]
pub const PDF_SIGNATURE_ERROR_NO_CERTIFICATE: i32 =
    fc::constants::PDF_SIGNATURE_ERROR_NO_CERTIFICATE;

#[napi]
pub const PDF_SIGNATURE_ERROR_NO_SIGNATURES: i32 = fc::constants::PDF_SIGNATURE_ERROR_NO_SIGNATURES;

#[napi]
pub const PDF_SIGNATURE_ERROR_OKAY: i32 = fc::constants::PDF_SIGNATURE_ERROR_OKAY;

#[napi]
pub const PDF_SIGNATURE_ERROR_SELF_SIGNED: i32 = fc::constants::PDF_SIGNATURE_ERROR_SELF_SIGNED;

#[napi]
pub const PDF_SIGNATURE_ERROR_SELF_SIGNED_IN_CHAIN: i32 =
    fc::constants::PDF_SIGNATURE_ERROR_SELF_SIGNED_IN_CHAIN;

#[napi]
pub const PDF_SIGNATURE_ERROR_UNKNOWN: i32 = fc::constants::PDF_SIGNATURE_ERROR_UNKNOWN;

#[napi]
pub const PDF_SIGNATURE_SHOW_DATE: i32 = fc::constants::PDF_SIGNATURE_SHOW_DATE;

#[napi]
pub const PDF_SIGNATURE_SHOW_DN: i32 = fc::constants::PDF_SIGNATURE_SHOW_DN;

#[napi]
pub const PDF_SIGNATURE_SHOW_GRAPHIC_NAME: i32 = fc::constants::PDF_SIGNATURE_SHOW_GRAPHIC_NAME;

#[napi]
pub const PDF_SIGNATURE_SHOW_LABELS: i32 = fc::constants::PDF_SIGNATURE_SHOW_LABELS;

#[napi]
pub const PDF_SIGNATURE_SHOW_LOGO: i32 = fc::constants::PDF_SIGNATURE_SHOW_LOGO;

#[napi]
pub const PDF_SIGNATURE_SHOW_TEXT_NAME: i32 = fc::constants::PDF_SIGNATURE_SHOW_TEXT_NAME;

#[napi]
pub const SigFlag_AppendOnly: i32 = fc::constants::SigFlag_AppendOnly;

#[napi]
pub const SigFlag_SignaturesExist: i32 = fc::constants::SigFlag_SignaturesExist;

#[napi]
pub const PDF_ALERT_BUTTON_CANCEL: i32 = fc::constants::PDF_ALERT_BUTTON_CANCEL;

#[napi]
pub const PDF_ALERT_BUTTON_GROUP_OK: i32 = fc::constants::PDF_ALERT_BUTTON_GROUP_OK;

#[napi]
pub const PDF_ALERT_BUTTON_GROUP_OK_CANCEL: i32 = fc::constants::PDF_ALERT_BUTTON_GROUP_OK_CANCEL;

#[napi]
pub const PDF_ALERT_BUTTON_GROUP_YES_NO: i32 = fc::constants::PDF_ALERT_BUTTON_GROUP_YES_NO;

#[napi]
pub const PDF_ALERT_BUTTON_GROUP_YES_NO_CANCEL: i32 =
    fc::constants::PDF_ALERT_BUTTON_GROUP_YES_NO_CANCEL;

#[napi]
pub const PDF_ALERT_BUTTON_NO: i32 = fc::constants::PDF_ALERT_BUTTON_NO;

#[napi]
pub const PDF_ALERT_BUTTON_NONE: i32 = fc::constants::PDF_ALERT_BUTTON_NONE;

#[napi]
pub const PDF_ALERT_BUTTON_OK: i32 = fc::constants::PDF_ALERT_BUTTON_OK;

#[napi]
pub const PDF_ALERT_BUTTON_YES: i32 = fc::constants::PDF_ALERT_BUTTON_YES;

#[napi]
pub const PDF_ALERT_ICON_ERROR: i32 = fc::constants::PDF_ALERT_ICON_ERROR;

#[napi]
pub const PDF_ALERT_ICON_QUESTION: i32 = fc::constants::PDF_ALERT_ICON_QUESTION;

#[napi]
pub const PDF_ALERT_ICON_STATUS: i32 = fc::constants::PDF_ALERT_ICON_STATUS;

#[napi]
pub const PDF_ALERT_ICON_WARNING: i32 = fc::constants::PDF_ALERT_ICON_WARNING;

#[napi]
pub const PDF_DOCUMENT_EVENT_ALERT: i32 = fc::constants::PDF_DOCUMENT_EVENT_ALERT;

#[napi]
pub const PDF_DOCUMENT_EVENT_EXEC_MENU_ITEM: i32 = fc::constants::PDF_DOCUMENT_EVENT_EXEC_MENU_ITEM;

#[napi]
pub const PDF_DOCUMENT_EVENT_LAUNCH_URL: i32 = fc::constants::PDF_DOCUMENT_EVENT_LAUNCH_URL;

#[napi]
pub const PDF_DOCUMENT_EVENT_MAIL_DOC: i32 = fc::constants::PDF_DOCUMENT_EVENT_MAIL_DOC;

#[napi]
pub const PDF_DOCUMENT_EVENT_PRINT: i32 = fc::constants::PDF_DOCUMENT_EVENT_PRINT;

#[napi]
pub const PDF_DOCUMENT_EVENT_SUBMIT: i32 = fc::constants::PDF_DOCUMENT_EVENT_SUBMIT;

#[napi]
pub const PDF_LAYER_UI_CHECKBOX: i32 = fc::constants::PDF_LAYER_UI_CHECKBOX;

#[napi]
pub const PDF_LAYER_UI_LABEL: i32 = fc::constants::PDF_LAYER_UI_LABEL;

#[napi]
pub const PDF_LAYER_UI_RADIOBOX: i32 = fc::constants::PDF_LAYER_UI_RADIOBOX;

#[napi]
pub const PDF_OC_OFF: i32 = fc::constants::PDF_OC_OFF;

#[napi]
pub const PDF_OC_ON: i32 = fc::constants::PDF_OC_ON;

#[napi]
pub const PDF_OC_TOGGLE: i32 = fc::constants::PDF_OC_TOGGLE;

#[napi]
pub const PDF_NOT_ZUGFERD: i32 = fc::constants::PDF_NOT_ZUGFERD;

#[napi]
pub const PDF_ZUGFERD_BASIC: i32 = fc::constants::PDF_ZUGFERD_BASIC;

#[napi]
pub const PDF_ZUGFERD_BASIC_WL: i32 = fc::constants::PDF_ZUGFERD_BASIC_WL;

#[napi]
pub const PDF_ZUGFERD_COMFORT: i32 = fc::constants::PDF_ZUGFERD_COMFORT;

#[napi]
pub const PDF_ZUGFERD_EXTENDED: i32 = fc::constants::PDF_ZUGFERD_EXTENDED;

#[napi]
pub const PDF_ZUGFERD_MINIMUM: i32 = fc::constants::PDF_ZUGFERD_MINIMUM;

#[napi]
pub const PDF_ZUGFERD_UNKNOWN: i32 = fc::constants::PDF_ZUGFERD_UNKNOWN;

#[napi]
pub const PDF_ZUGFERD_XRECHNUNG: i32 = fc::constants::PDF_ZUGFERD_XRECHNUNG;

#[napi]
pub const PDF_CLEAN_STRUCTURE_DROP: i32 = fc::constants::PDF_CLEAN_STRUCTURE_DROP;

#[napi]
pub const PDF_CLEAN_STRUCTURE_KEEP: i32 = fc::constants::PDF_CLEAN_STRUCTURE_KEEP;

#[napi]
pub const PDF_CLEAN_VECTORIZE_NO: i32 = fc::constants::PDF_CLEAN_VECTORIZE_NO;

#[napi]
pub const PDF_CLEAN_VECTORIZE_YES: i32 = fc::constants::PDF_CLEAN_VECTORIZE_YES;

#[napi]
pub const CS_CMYK: i32 = fc::constants::CS_CMYK;

#[napi]
pub const CS_GRAY: i32 = fc::constants::CS_GRAY;

#[napi]
pub const CS_RGB: i32 = fc::constants::CS_RGB;

#[napi]
pub const FZ_RECOMPRESS_FAX: i32 = fc::constants::FZ_RECOMPRESS_FAX;

#[napi]
pub const FZ_RECOMPRESS_J2K: i32 = fc::constants::FZ_RECOMPRESS_J2K;

#[napi]
pub const FZ_RECOMPRESS_JPEG: i32 = fc::constants::FZ_RECOMPRESS_JPEG;

#[napi]
pub const FZ_RECOMPRESS_LOSSLESS: i32 = fc::constants::FZ_RECOMPRESS_LOSSLESS;

#[napi]
pub const FZ_RECOMPRESS_NEVER: i32 = fc::constants::FZ_RECOMPRESS_NEVER;

#[napi]
pub const FZ_RECOMPRESS_SAME: i32 = fc::constants::FZ_RECOMPRESS_SAME;

#[napi]
pub const FZ_SUBSAMPLE_AVERAGE: i32 = fc::constants::FZ_SUBSAMPLE_AVERAGE;

#[napi]
pub const FZ_SUBSAMPLE_BICUBIC: i32 = fc::constants::FZ_SUBSAMPLE_BICUBIC;

#[napi]
pub const PDF_ANNOT_3D: i32 = fc::constants::PDF_ANNOT_3D;

#[napi]
pub const PDF_ANNOT_CARET: i32 = fc::constants::PDF_ANNOT_CARET;

#[napi]
pub const PDF_ANNOT_CIRCLE: i32 = fc::constants::PDF_ANNOT_CIRCLE;

#[napi]
pub const PDF_ANNOT_FILE_ATTACHMENT: i32 = fc::constants::PDF_ANNOT_FILE_ATTACHMENT;

#[napi]
pub const PDF_ANNOT_FREE_TEXT: i32 = fc::constants::PDF_ANNOT_FREE_TEXT;

#[napi]
pub const PDF_ANNOT_HIGHLIGHT: i32 = fc::constants::PDF_ANNOT_HIGHLIGHT;

#[napi]
pub const PDF_ANNOT_INK: i32 = fc::constants::PDF_ANNOT_INK;

#[napi]
pub const PDF_ANNOT_IS_HIDDEN: i32 = fc::constants::PDF_ANNOT_IS_HIDDEN;

#[napi]
pub const PDF_ANNOT_IS_INVISIBLE: i32 = fc::constants::PDF_ANNOT_IS_INVISIBLE;

#[napi]
pub const PDF_ANNOT_IS_LOCKED: i32 = fc::constants::PDF_ANNOT_IS_LOCKED;

#[napi]
pub const PDF_ANNOT_IS_LOCKED_CONTENTS: i32 = fc::constants::PDF_ANNOT_IS_LOCKED_CONTENTS;

#[napi]
pub const PDF_ANNOT_IS_NO_ROTATE: i32 = fc::constants::PDF_ANNOT_IS_NO_ROTATE;

#[napi]
pub const PDF_ANNOT_IS_NO_VIEW: i32 = fc::constants::PDF_ANNOT_IS_NO_VIEW;

#[napi]
pub const PDF_ANNOT_IS_NO_ZOOM: i32 = fc::constants::PDF_ANNOT_IS_NO_ZOOM;

#[napi]
pub const PDF_ANNOT_IS_PRINT: i32 = fc::constants::PDF_ANNOT_IS_PRINT;

#[napi]
pub const PDF_ANNOT_IS_READ_ONLY: i32 = fc::constants::PDF_ANNOT_IS_READ_ONLY;

#[napi]
pub const PDF_ANNOT_IS_TOGGLE_NO_VIEW: i32 = fc::constants::PDF_ANNOT_IS_TOGGLE_NO_VIEW;

#[napi]
pub const PDF_ANNOT_IT_DEFAULT: i32 = fc::constants::PDF_ANNOT_IT_DEFAULT;

#[napi]
pub const PDF_ANNOT_IT_FREETEXT_CALLOUT: i32 = fc::constants::PDF_ANNOT_IT_FREETEXT_CALLOUT;

#[napi]
pub const PDF_ANNOT_IT_FREETEXT_TYPEWRITER: i32 = fc::constants::PDF_ANNOT_IT_FREETEXT_TYPEWRITER;

#[napi]
pub const PDF_ANNOT_IT_LINE_ARROW: i32 = fc::constants::PDF_ANNOT_IT_LINE_ARROW;

#[napi]
pub const PDF_ANNOT_IT_LINE_DIMENSION: i32 = fc::constants::PDF_ANNOT_IT_LINE_DIMENSION;

#[napi]
pub const PDF_ANNOT_IT_POLYGON_CLOUD: i32 = fc::constants::PDF_ANNOT_IT_POLYGON_CLOUD;

#[napi]
pub const PDF_ANNOT_IT_POLYGON_DIMENSION: i32 = fc::constants::PDF_ANNOT_IT_POLYGON_DIMENSION;

#[napi]
pub const PDF_ANNOT_IT_POLYLINE_DIMENSION: i32 = fc::constants::PDF_ANNOT_IT_POLYLINE_DIMENSION;

#[napi]
pub const PDF_ANNOT_IT_STAMP_IMAGE: i32 = fc::constants::PDF_ANNOT_IT_STAMP_IMAGE;

#[napi]
pub const PDF_ANNOT_IT_STAMP_SNAPSHOT: i32 = fc::constants::PDF_ANNOT_IT_STAMP_SNAPSHOT;

#[napi]
pub const PDF_ANNOT_IT_UNKNOWN: i32 = fc::constants::PDF_ANNOT_IT_UNKNOWN;

#[napi]
pub const PDF_ANNOT_LE_BUTT: i32 = fc::constants::PDF_ANNOT_LE_BUTT;

#[napi]
pub const PDF_ANNOT_LE_CIRCLE: i32 = fc::constants::PDF_ANNOT_LE_CIRCLE;

#[napi]
pub const PDF_ANNOT_LE_CLOSED_ARROW: i32 = fc::constants::PDF_ANNOT_LE_CLOSED_ARROW;

#[napi]
pub const PDF_ANNOT_LE_DIAMOND: i32 = fc::constants::PDF_ANNOT_LE_DIAMOND;

#[napi]
pub const PDF_ANNOT_LE_NONE: i32 = fc::constants::PDF_ANNOT_LE_NONE;

#[napi]
pub const PDF_ANNOT_LE_OPEN_ARROW: i32 = fc::constants::PDF_ANNOT_LE_OPEN_ARROW;

#[napi]
pub const PDF_ANNOT_LE_R_CLOSED_ARROW: i32 = fc::constants::PDF_ANNOT_LE_R_CLOSED_ARROW;

#[napi]
pub const PDF_ANNOT_LE_R_OPEN_ARROW: i32 = fc::constants::PDF_ANNOT_LE_R_OPEN_ARROW;

#[napi]
pub const PDF_ANNOT_LE_SLASH: i32 = fc::constants::PDF_ANNOT_LE_SLASH;

#[napi]
pub const PDF_ANNOT_LE_SQUARE: i32 = fc::constants::PDF_ANNOT_LE_SQUARE;

#[napi]
pub const PDF_ANNOT_LINE: i32 = fc::constants::PDF_ANNOT_LINE;

#[napi]
pub const PDF_ANNOT_LINK: i32 = fc::constants::PDF_ANNOT_LINK;

#[napi]
pub const PDF_ANNOT_MOVIE: i32 = fc::constants::PDF_ANNOT_MOVIE;

#[napi]
pub const PDF_ANNOT_POLYGON: i32 = fc::constants::PDF_ANNOT_POLYGON;

#[napi]
pub const PDF_ANNOT_POLY_LINE: i32 = fc::constants::PDF_ANNOT_POLY_LINE;

#[napi]
pub const PDF_ANNOT_POPUP: i32 = fc::constants::PDF_ANNOT_POPUP;

#[napi]
pub const PDF_ANNOT_PRINTER_MARK: i32 = fc::constants::PDF_ANNOT_PRINTER_MARK;

#[napi]
pub const PDF_ANNOT_PROJECTION: i32 = fc::constants::PDF_ANNOT_PROJECTION;

#[napi]
pub const PDF_ANNOT_Q_CENTER: i32 = fc::constants::PDF_ANNOT_Q_CENTER;

#[napi]
pub const PDF_ANNOT_Q_LEFT: i32 = fc::constants::PDF_ANNOT_Q_LEFT;

#[napi]
pub const PDF_ANNOT_Q_RIGHT: i32 = fc::constants::PDF_ANNOT_Q_RIGHT;

#[napi]
pub const PDF_ANNOT_REDACT: i32 = fc::constants::PDF_ANNOT_REDACT;

#[napi]
pub const PDF_ANNOT_RICH_MEDIA: i32 = fc::constants::PDF_ANNOT_RICH_MEDIA;

#[napi]
pub const PDF_ANNOT_SCREEN: i32 = fc::constants::PDF_ANNOT_SCREEN;

#[napi]
pub const PDF_ANNOT_SOUND: i32 = fc::constants::PDF_ANNOT_SOUND;

#[napi]
pub const PDF_ANNOT_SQUARE: i32 = fc::constants::PDF_ANNOT_SQUARE;

#[napi]
pub const PDF_ANNOT_SQUIGGLY: i32 = fc::constants::PDF_ANNOT_SQUIGGLY;

#[napi]
pub const PDF_ANNOT_STAMP: i32 = fc::constants::PDF_ANNOT_STAMP;

#[napi]
pub const PDF_ANNOT_STRIKE_OUT: i32 = fc::constants::PDF_ANNOT_STRIKE_OUT;

#[napi]
pub const PDF_ANNOT_TEXT: i32 = fc::constants::PDF_ANNOT_TEXT;

#[napi]
pub const PDF_ANNOT_TRAP_NET: i32 = fc::constants::PDF_ANNOT_TRAP_NET;

#[napi]
pub const PDF_ANNOT_UNDERLINE: i32 = fc::constants::PDF_ANNOT_UNDERLINE;

#[napi]
pub const PDF_ANNOT_UNKNOWN: i32 = fc::constants::PDF_ANNOT_UNKNOWN;

#[napi]
pub const PDF_ANNOT_WATERMARK: i32 = fc::constants::PDF_ANNOT_WATERMARK;

#[napi]
pub const PDF_ANNOT_WIDGET: i32 = fc::constants::PDF_ANNOT_WIDGET;

#[napi]
pub const PDF_BORDER_EFFECT_CLOUDY: i32 = fc::constants::PDF_BORDER_EFFECT_CLOUDY;

#[napi]
pub const PDF_BORDER_EFFECT_NONE: i32 = fc::constants::PDF_BORDER_EFFECT_NONE;

#[napi]
pub const PDF_BORDER_STYLE_BEVELED: i32 = fc::constants::PDF_BORDER_STYLE_BEVELED;

#[napi]
pub const PDF_BORDER_STYLE_DASHED: i32 = fc::constants::PDF_BORDER_STYLE_DASHED;

#[napi]
pub const PDF_BORDER_STYLE_INSET: i32 = fc::constants::PDF_BORDER_STYLE_INSET;

#[napi]
pub const PDF_BORDER_STYLE_SOLID: i32 = fc::constants::PDF_BORDER_STYLE_SOLID;

#[napi]
pub const PDF_BORDER_STYLE_UNDERLINE: i32 = fc::constants::PDF_BORDER_STYLE_UNDERLINE;

#[napi]
pub const PDF_BTN_FIELD_IS_NO_TOGGLE_TO_OFF: i32 = fc::constants::PDF_BTN_FIELD_IS_NO_TOGGLE_TO_OFF;

#[napi]
pub const PDF_BTN_FIELD_IS_PUSHBUTTON: i32 = fc::constants::PDF_BTN_FIELD_IS_PUSHBUTTON;

#[napi]
pub const PDF_BTN_FIELD_IS_RADIO: i32 = fc::constants::PDF_BTN_FIELD_IS_RADIO;

#[napi]
pub const PDF_BTN_FIELD_IS_RADIOS_IN_UNISON: i32 = fc::constants::PDF_BTN_FIELD_IS_RADIOS_IN_UNISON;

#[napi]
pub const PDF_CH_FIELD_IS_COMBO: i32 = fc::constants::PDF_CH_FIELD_IS_COMBO;

#[napi]
pub const PDF_CH_FIELD_IS_COMMIT_ON_SEL_CHANGE: i32 =
    fc::constants::PDF_CH_FIELD_IS_COMMIT_ON_SEL_CHANGE;

#[napi]
pub const PDF_CH_FIELD_IS_DO_NOT_SPELL_CHECK: i32 =
    fc::constants::PDF_CH_FIELD_IS_DO_NOT_SPELL_CHECK;

#[napi]
pub const PDF_CH_FIELD_IS_EDIT: i32 = fc::constants::PDF_CH_FIELD_IS_EDIT;

#[napi]
pub const PDF_CH_FIELD_IS_MULTI_SELECT: i32 = fc::constants::PDF_CH_FIELD_IS_MULTI_SELECT;

#[napi]
pub const PDF_CH_FIELD_IS_SORT: i32 = fc::constants::PDF_CH_FIELD_IS_SORT;

#[napi]
pub const PDF_FIELD_IS_NO_EXPORT: i32 = fc::constants::PDF_FIELD_IS_NO_EXPORT;

#[napi]
pub const PDF_FIELD_IS_READ_ONLY: i32 = fc::constants::PDF_FIELD_IS_READ_ONLY;

#[napi]
pub const PDF_FIELD_IS_REQUIRED: i32 = fc::constants::PDF_FIELD_IS_REQUIRED;

#[napi]
pub const PDF_TX_FIELD_IS_COMB: i32 = fc::constants::PDF_TX_FIELD_IS_COMB;

#[napi]
pub const PDF_TX_FIELD_IS_DO_NOT_SCROLL: i32 = fc::constants::PDF_TX_FIELD_IS_DO_NOT_SCROLL;

#[napi]
pub const PDF_TX_FIELD_IS_DO_NOT_SPELL_CHECK: i32 =
    fc::constants::PDF_TX_FIELD_IS_DO_NOT_SPELL_CHECK;

#[napi]
pub const PDF_TX_FIELD_IS_FILE_SELECT: i32 = fc::constants::PDF_TX_FIELD_IS_FILE_SELECT;

#[napi]
pub const PDF_TX_FIELD_IS_MULTILINE: i32 = fc::constants::PDF_TX_FIELD_IS_MULTILINE;

#[napi]
pub const PDF_TX_FIELD_IS_PASSWORD: i32 = fc::constants::PDF_TX_FIELD_IS_PASSWORD;

#[napi]
pub const PDF_TX_FIELD_IS_RICH_TEXT: i32 = fc::constants::PDF_TX_FIELD_IS_RICH_TEXT;

#[napi]
pub const PDF_WIDGET_TX_FORMAT_DATE: i32 = fc::constants::PDF_WIDGET_TX_FORMAT_DATE;

#[napi]
pub const PDF_WIDGET_TX_FORMAT_NONE: i32 = fc::constants::PDF_WIDGET_TX_FORMAT_NONE;

#[napi]
pub const PDF_WIDGET_TX_FORMAT_NUMBER: i32 = fc::constants::PDF_WIDGET_TX_FORMAT_NUMBER;

#[napi]
pub const PDF_WIDGET_TX_FORMAT_SPECIAL: i32 = fc::constants::PDF_WIDGET_TX_FORMAT_SPECIAL;

#[napi]
pub const PDF_WIDGET_TX_FORMAT_TIME: i32 = fc::constants::PDF_WIDGET_TX_FORMAT_TIME;

#[napi]
pub const PDF_WIDGET_TYPE_BUTTON: i32 = fc::constants::PDF_WIDGET_TYPE_BUTTON;

#[napi]
pub const PDF_WIDGET_TYPE_CHECKBOX: i32 = fc::constants::PDF_WIDGET_TYPE_CHECKBOX;

#[napi]
pub const PDF_WIDGET_TYPE_COMBOBOX: i32 = fc::constants::PDF_WIDGET_TYPE_COMBOBOX;

#[napi]
pub const PDF_WIDGET_TYPE_LISTBOX: i32 = fc::constants::PDF_WIDGET_TYPE_LISTBOX;

#[napi]
pub const PDF_WIDGET_TYPE_RADIOBUTTON: i32 = fc::constants::PDF_WIDGET_TYPE_RADIOBUTTON;

#[napi]
pub const PDF_WIDGET_TYPE_SIGNATURE: i32 = fc::constants::PDF_WIDGET_TYPE_SIGNATURE;

#[napi]
pub const PDF_WIDGET_TYPE_TEXT: i32 = fc::constants::PDF_WIDGET_TYPE_TEXT;

#[napi]
pub const PDF_WIDGET_TYPE_UNKNOWN: i32 = fc::constants::PDF_WIDGET_TYPE_UNKNOWN;

#[napi]
pub const STAMP_Approved: i32 = fc::constants::STAMP_Approved;

#[napi]
pub const STAMP_AsIs: i32 = fc::constants::STAMP_AsIs;

#[napi]
pub const STAMP_Confidential: i32 = fc::constants::STAMP_Confidential;

#[napi]
pub const STAMP_Departmental: i32 = fc::constants::STAMP_Departmental;

#[napi]
pub const STAMP_Draft: i32 = fc::constants::STAMP_Draft;

#[napi]
pub const STAMP_Experimental: i32 = fc::constants::STAMP_Experimental;

#[napi]
pub const STAMP_Expired: i32 = fc::constants::STAMP_Expired;

#[napi]
pub const STAMP_Final: i32 = fc::constants::STAMP_Final;

#[napi]
pub const STAMP_ForComment: i32 = fc::constants::STAMP_ForComment;

#[napi]
pub const STAMP_ForPublicRelease: i32 = fc::constants::STAMP_ForPublicRelease;

#[napi]
pub const STAMP_NotApproved: i32 = fc::constants::STAMP_NotApproved;

#[napi]
pub const STAMP_NotForPublicRelease: i32 = fc::constants::STAMP_NotForPublicRelease;

#[napi]
pub const STAMP_Sold: i32 = fc::constants::STAMP_Sold;

#[napi]
pub const STAMP_TopSecret: i32 = fc::constants::STAMP_TopSecret;

#[napi]
pub const PDF_BM_Color: &str = fc::constants::PDF_BM_Color;

#[napi]
pub const PDF_BM_ColorBurn: &str = fc::constants::PDF_BM_ColorBurn;

#[napi]
pub const PDF_BM_ColorDodge: &str = fc::constants::PDF_BM_ColorDodge;

#[napi]
pub const PDF_BM_Darken: &str = fc::constants::PDF_BM_Darken;

#[napi]
pub const PDF_BM_Difference: &str = fc::constants::PDF_BM_Difference;

#[napi]
pub const PDF_BM_Exclusion: &str = fc::constants::PDF_BM_Exclusion;

#[napi]
pub const PDF_BM_HardLight: &str = fc::constants::PDF_BM_HardLight;

#[napi]
pub const PDF_BM_Hue: &str = fc::constants::PDF_BM_Hue;

#[napi]
pub const PDF_BM_Lighten: &str = fc::constants::PDF_BM_Lighten;

#[napi]
pub const PDF_BM_Luminosity: &str = fc::constants::PDF_BM_Luminosity;

#[napi]
pub const PDF_BM_Multiply: &str = fc::constants::PDF_BM_Multiply;

#[napi]
pub const PDF_BM_Normal: &str = fc::constants::PDF_BM_Normal;

#[napi]
pub const PDF_BM_Overlay: &str = fc::constants::PDF_BM_Overlay;

#[napi]
pub const PDF_BM_Saturation: &str = fc::constants::PDF_BM_Saturation;

#[napi]
pub const PDF_BM_Screen: &str = fc::constants::PDF_BM_Screen;

#[napi]
pub const PDF_BM_SoftLight: &str = fc::constants::PDF_BM_SoftLight;

#[napi]
pub const PDF_REDACT_IMAGE_NONE: i32 = fc::constants::PDF_REDACT_IMAGE_NONE;

#[napi]
pub const PDF_REDACT_IMAGE_PIXELS: i32 = fc::constants::PDF_REDACT_IMAGE_PIXELS;

#[napi]
pub const PDF_REDACT_IMAGE_REMOVE: i32 = fc::constants::PDF_REDACT_IMAGE_REMOVE;

#[napi]
pub const PDF_REDACT_IMAGE_REMOVE_UNLESS_INVISIBLE: i32 =
    fc::constants::PDF_REDACT_IMAGE_REMOVE_UNLESS_INVISIBLE;

#[napi]
pub const PDF_REDACT_LINE_ART_NONE: i32 = fc::constants::PDF_REDACT_LINE_ART_NONE;

#[napi]
pub const PDF_REDACT_LINE_ART_REMOVE_IF_COVERED: i32 =
    fc::constants::PDF_REDACT_LINE_ART_REMOVE_IF_COVERED;

#[napi]
pub const PDF_REDACT_LINE_ART_REMOVE_IF_TOUCHED: i32 =
    fc::constants::PDF_REDACT_LINE_ART_REMOVE_IF_TOUCHED;

#[napi]
pub const PDF_REDACT_TEXT_NONE: i32 = fc::constants::PDF_REDACT_TEXT_NONE;

#[napi]
pub const PDF_REDACT_TEXT_REMOVE: i32 = fc::constants::PDF_REDACT_TEXT_REMOVE;

#[napi]
pub const PDF_REDACT_TEXT_REMOVE_INVISIBLE: i32 = fc::constants::PDF_REDACT_TEXT_REMOVE_INVISIBLE;

#[napi]
pub const UCDN_SCRIPT_ADLAM: i32 = fc::constants::UCDN_SCRIPT_ADLAM;

#[napi]
pub const UCDN_SCRIPT_AHOM: i32 = fc::constants::UCDN_SCRIPT_AHOM;

#[napi]
pub const UCDN_SCRIPT_ANATOLIAN_HIEROGLYPHS: i32 = fc::constants::UCDN_SCRIPT_ANATOLIAN_HIEROGLYPHS;

#[napi]
pub const UCDN_SCRIPT_ARABIC: i32 = fc::constants::UCDN_SCRIPT_ARABIC;

#[napi]
pub const UCDN_SCRIPT_ARMENIAN: i32 = fc::constants::UCDN_SCRIPT_ARMENIAN;

#[napi]
pub const UCDN_SCRIPT_AVESTAN: i32 = fc::constants::UCDN_SCRIPT_AVESTAN;

#[napi]
pub const UCDN_SCRIPT_BALINESE: i32 = fc::constants::UCDN_SCRIPT_BALINESE;

#[napi]
pub const UCDN_SCRIPT_BAMUM: i32 = fc::constants::UCDN_SCRIPT_BAMUM;

#[napi]
pub const UCDN_SCRIPT_BASSA_VAH: i32 = fc::constants::UCDN_SCRIPT_BASSA_VAH;

#[napi]
pub const UCDN_SCRIPT_BATAK: i32 = fc::constants::UCDN_SCRIPT_BATAK;

#[napi]
pub const UCDN_SCRIPT_BENGALI: i32 = fc::constants::UCDN_SCRIPT_BENGALI;

#[napi]
pub const UCDN_SCRIPT_BHAIKSUKI: i32 = fc::constants::UCDN_SCRIPT_BHAIKSUKI;

#[napi]
pub const UCDN_SCRIPT_BOPOMOFO: i32 = fc::constants::UCDN_SCRIPT_BOPOMOFO;

#[napi]
pub const UCDN_SCRIPT_BRAHMI: i32 = fc::constants::UCDN_SCRIPT_BRAHMI;

#[napi]
pub const UCDN_SCRIPT_BRAILLE: i32 = fc::constants::UCDN_SCRIPT_BRAILLE;

#[napi]
pub const UCDN_SCRIPT_BUGINESE: i32 = fc::constants::UCDN_SCRIPT_BUGINESE;

#[napi]
pub const UCDN_SCRIPT_BUHID: i32 = fc::constants::UCDN_SCRIPT_BUHID;

#[napi]
pub const UCDN_SCRIPT_CANADIAN_ABORIGINAL: i32 = fc::constants::UCDN_SCRIPT_CANADIAN_ABORIGINAL;

#[napi]
pub const UCDN_SCRIPT_CARIAN: i32 = fc::constants::UCDN_SCRIPT_CARIAN;

#[napi]
pub const UCDN_SCRIPT_CAUCASIAN_ALBANIAN: i32 = fc::constants::UCDN_SCRIPT_CAUCASIAN_ALBANIAN;

#[napi]
pub const UCDN_SCRIPT_CHAKMA: i32 = fc::constants::UCDN_SCRIPT_CHAKMA;

#[napi]
pub const UCDN_SCRIPT_CHAM: i32 = fc::constants::UCDN_SCRIPT_CHAM;

#[napi]
pub const UCDN_SCRIPT_CHEROKEE: i32 = fc::constants::UCDN_SCRIPT_CHEROKEE;

#[napi]
pub const UCDN_SCRIPT_CHORASMIAN: i32 = fc::constants::UCDN_SCRIPT_CHORASMIAN;

#[napi]
pub const UCDN_SCRIPT_COMMON: i32 = fc::constants::UCDN_SCRIPT_COMMON;

#[napi]
pub const UCDN_SCRIPT_COPTIC: i32 = fc::constants::UCDN_SCRIPT_COPTIC;

#[napi]
pub const UCDN_SCRIPT_CUNEIFORM: i32 = fc::constants::UCDN_SCRIPT_CUNEIFORM;

#[napi]
pub const UCDN_SCRIPT_CYPRIOT: i32 = fc::constants::UCDN_SCRIPT_CYPRIOT;

#[napi]
pub const UCDN_SCRIPT_CYPRO_MINOAN: i32 = fc::constants::UCDN_SCRIPT_CYPRO_MINOAN;

#[napi]
pub const UCDN_SCRIPT_CYRILLIC: i32 = fc::constants::UCDN_SCRIPT_CYRILLIC;

#[napi]
pub const UCDN_SCRIPT_DESERET: i32 = fc::constants::UCDN_SCRIPT_DESERET;

#[napi]
pub const UCDN_SCRIPT_DEVANAGARI: i32 = fc::constants::UCDN_SCRIPT_DEVANAGARI;

#[napi]
pub const UCDN_SCRIPT_DIVES_AKURU: i32 = fc::constants::UCDN_SCRIPT_DIVES_AKURU;

#[napi]
pub const UCDN_SCRIPT_DOGRA: i32 = fc::constants::UCDN_SCRIPT_DOGRA;

#[napi]
pub const UCDN_SCRIPT_DUPLOYAN: i32 = fc::constants::UCDN_SCRIPT_DUPLOYAN;

#[napi]
pub const UCDN_SCRIPT_EGYPTIAN_HIEROGLYPHS: i32 = fc::constants::UCDN_SCRIPT_EGYPTIAN_HIEROGLYPHS;

#[napi]
pub const UCDN_SCRIPT_ELBASAN: i32 = fc::constants::UCDN_SCRIPT_ELBASAN;

#[napi]
pub const UCDN_SCRIPT_ELYMAIC: i32 = fc::constants::UCDN_SCRIPT_ELYMAIC;

#[napi]
pub const UCDN_SCRIPT_ETHIOPIC: i32 = fc::constants::UCDN_SCRIPT_ETHIOPIC;

#[napi]
pub const UCDN_SCRIPT_GARAY: i32 = fc::constants::UCDN_SCRIPT_GARAY;

#[napi]
pub const UCDN_SCRIPT_GEORGIAN: i32 = fc::constants::UCDN_SCRIPT_GEORGIAN;

#[napi]
pub const UCDN_SCRIPT_GLAGOLITIC: i32 = fc::constants::UCDN_SCRIPT_GLAGOLITIC;

#[napi]
pub const UCDN_SCRIPT_GOTHIC: i32 = fc::constants::UCDN_SCRIPT_GOTHIC;

#[napi]
pub const UCDN_SCRIPT_GRANTHA: i32 = fc::constants::UCDN_SCRIPT_GRANTHA;

#[napi]
pub const UCDN_SCRIPT_GREEK: i32 = fc::constants::UCDN_SCRIPT_GREEK;

#[napi]
pub const UCDN_SCRIPT_GUJARATI: i32 = fc::constants::UCDN_SCRIPT_GUJARATI;

#[napi]
pub const UCDN_SCRIPT_GUNJALA_GONDI: i32 = fc::constants::UCDN_SCRIPT_GUNJALA_GONDI;

#[napi]
pub const UCDN_SCRIPT_GURMUKHI: i32 = fc::constants::UCDN_SCRIPT_GURMUKHI;

#[napi]
pub const UCDN_SCRIPT_GURUNG_KHEMA: i32 = fc::constants::UCDN_SCRIPT_GURUNG_KHEMA;

#[napi]
pub const UCDN_SCRIPT_HAN: i32 = fc::constants::UCDN_SCRIPT_HAN;

#[napi]
pub const UCDN_SCRIPT_HANGUL: i32 = fc::constants::UCDN_SCRIPT_HANGUL;

#[napi]
pub const UCDN_SCRIPT_HANIFI_ROHINGYA: i32 = fc::constants::UCDN_SCRIPT_HANIFI_ROHINGYA;

#[napi]
pub const UCDN_SCRIPT_HANUNOO: i32 = fc::constants::UCDN_SCRIPT_HANUNOO;

#[napi]
pub const UCDN_SCRIPT_HATRAN: i32 = fc::constants::UCDN_SCRIPT_HATRAN;

#[napi]
pub const UCDN_SCRIPT_HEBREW: i32 = fc::constants::UCDN_SCRIPT_HEBREW;

#[napi]
pub const UCDN_SCRIPT_HIRAGANA: i32 = fc::constants::UCDN_SCRIPT_HIRAGANA;

#[napi]
pub const UCDN_SCRIPT_IMPERIAL_ARAMAIC: i32 = fc::constants::UCDN_SCRIPT_IMPERIAL_ARAMAIC;

#[napi]
pub const UCDN_SCRIPT_INHERITED: i32 = fc::constants::UCDN_SCRIPT_INHERITED;

#[napi]
pub const UCDN_SCRIPT_INSCRIPTIONAL_PAHLAVI: i32 = fc::constants::UCDN_SCRIPT_INSCRIPTIONAL_PAHLAVI;

#[napi]
pub const UCDN_SCRIPT_INSCRIPTIONAL_PARTHIAN: i32 =
    fc::constants::UCDN_SCRIPT_INSCRIPTIONAL_PARTHIAN;

#[napi]
pub const UCDN_SCRIPT_JAVANESE: i32 = fc::constants::UCDN_SCRIPT_JAVANESE;

#[napi]
pub const UCDN_SCRIPT_KAITHI: i32 = fc::constants::UCDN_SCRIPT_KAITHI;

#[napi]
pub const UCDN_SCRIPT_KANNADA: i32 = fc::constants::UCDN_SCRIPT_KANNADA;

#[napi]
pub const UCDN_SCRIPT_KATAKANA: i32 = fc::constants::UCDN_SCRIPT_KATAKANA;

#[napi]
pub const UCDN_SCRIPT_KAWI: i32 = fc::constants::UCDN_SCRIPT_KAWI;

#[napi]
pub const UCDN_SCRIPT_KAYAH_LI: i32 = fc::constants::UCDN_SCRIPT_KAYAH_LI;

#[napi]
pub const UCDN_SCRIPT_KHAROSHTHI: i32 = fc::constants::UCDN_SCRIPT_KHAROSHTHI;

#[napi]
pub const UCDN_SCRIPT_KHITAN_SMALL_SCRIPT: i32 = fc::constants::UCDN_SCRIPT_KHITAN_SMALL_SCRIPT;

#[napi]
pub const UCDN_SCRIPT_KHMER: i32 = fc::constants::UCDN_SCRIPT_KHMER;

#[napi]
pub const UCDN_SCRIPT_KHOJKI: i32 = fc::constants::UCDN_SCRIPT_KHOJKI;

#[napi]
pub const UCDN_SCRIPT_KHUDAWADI: i32 = fc::constants::UCDN_SCRIPT_KHUDAWADI;

#[napi]
pub const UCDN_SCRIPT_KIRAT_RAI: i32 = fc::constants::UCDN_SCRIPT_KIRAT_RAI;

#[napi]
pub const UCDN_SCRIPT_LAO: i32 = fc::constants::UCDN_SCRIPT_LAO;

#[napi]
pub const UCDN_SCRIPT_LATIN: i32 = fc::constants::UCDN_SCRIPT_LATIN;

#[napi]
pub const UCDN_SCRIPT_LEPCHA: i32 = fc::constants::UCDN_SCRIPT_LEPCHA;

#[napi]
pub const UCDN_SCRIPT_LIMBU: i32 = fc::constants::UCDN_SCRIPT_LIMBU;

#[napi]
pub const UCDN_SCRIPT_LINEAR_A: i32 = fc::constants::UCDN_SCRIPT_LINEAR_A;

#[napi]
pub const UCDN_SCRIPT_LINEAR_B: i32 = fc::constants::UCDN_SCRIPT_LINEAR_B;

#[napi]
pub const UCDN_SCRIPT_LISU: i32 = fc::constants::UCDN_SCRIPT_LISU;

#[napi]
pub const UCDN_SCRIPT_LYCIAN: i32 = fc::constants::UCDN_SCRIPT_LYCIAN;

#[napi]
pub const UCDN_SCRIPT_LYDIAN: i32 = fc::constants::UCDN_SCRIPT_LYDIAN;

#[napi]
pub const UCDN_SCRIPT_MAHAJANI: i32 = fc::constants::UCDN_SCRIPT_MAHAJANI;

#[napi]
pub const UCDN_SCRIPT_MAKASAR: i32 = fc::constants::UCDN_SCRIPT_MAKASAR;

#[napi]
pub const UCDN_SCRIPT_MALAYALAM: i32 = fc::constants::UCDN_SCRIPT_MALAYALAM;

#[napi]
pub const UCDN_SCRIPT_MANDAIC: i32 = fc::constants::UCDN_SCRIPT_MANDAIC;

#[napi]
pub const UCDN_SCRIPT_MANICHAEAN: i32 = fc::constants::UCDN_SCRIPT_MANICHAEAN;

#[napi]
pub const UCDN_SCRIPT_MARCHEN: i32 = fc::constants::UCDN_SCRIPT_MARCHEN;

#[napi]
pub const UCDN_SCRIPT_MASARAM_GONDI: i32 = fc::constants::UCDN_SCRIPT_MASARAM_GONDI;

#[napi]
pub const UCDN_SCRIPT_MEDEFAIDRIN: i32 = fc::constants::UCDN_SCRIPT_MEDEFAIDRIN;

#[napi]
pub const UCDN_SCRIPT_MEETEI_MAYEK: i32 = fc::constants::UCDN_SCRIPT_MEETEI_MAYEK;

#[napi]
pub const UCDN_SCRIPT_MENDE_KIKAKUI: i32 = fc::constants::UCDN_SCRIPT_MENDE_KIKAKUI;

#[napi]
pub const UCDN_SCRIPT_MEROITIC_CURSIVE: i32 = fc::constants::UCDN_SCRIPT_MEROITIC_CURSIVE;

#[napi]
pub const UCDN_SCRIPT_MEROITIC_HIEROGLYPHS: i32 = fc::constants::UCDN_SCRIPT_MEROITIC_HIEROGLYPHS;

#[napi]
pub const UCDN_SCRIPT_MIAO: i32 = fc::constants::UCDN_SCRIPT_MIAO;

#[napi]
pub const UCDN_SCRIPT_MODI: i32 = fc::constants::UCDN_SCRIPT_MODI;

#[napi]
pub const UCDN_SCRIPT_MONGOLIAN: i32 = fc::constants::UCDN_SCRIPT_MONGOLIAN;

#[napi]
pub const UCDN_SCRIPT_MRO: i32 = fc::constants::UCDN_SCRIPT_MRO;

#[napi]
pub const UCDN_SCRIPT_MULTANI: i32 = fc::constants::UCDN_SCRIPT_MULTANI;

#[napi]
pub const UCDN_SCRIPT_MYANMAR: i32 = fc::constants::UCDN_SCRIPT_MYANMAR;

#[napi]
pub const UCDN_SCRIPT_NABATAEAN: i32 = fc::constants::UCDN_SCRIPT_NABATAEAN;

#[napi]
pub const UCDN_SCRIPT_NAG_MUNDARI: i32 = fc::constants::UCDN_SCRIPT_NAG_MUNDARI;

#[napi]
pub const UCDN_SCRIPT_NANDINAGARI: i32 = fc::constants::UCDN_SCRIPT_NANDINAGARI;

#[napi]
pub const UCDN_SCRIPT_NEWA: i32 = fc::constants::UCDN_SCRIPT_NEWA;

#[napi]
pub const UCDN_SCRIPT_NEW_TAI_LUE: i32 = fc::constants::UCDN_SCRIPT_NEW_TAI_LUE;

#[napi]
pub const UCDN_SCRIPT_NKO: i32 = fc::constants::UCDN_SCRIPT_NKO;

#[napi]
pub const UCDN_SCRIPT_NUSHU: i32 = fc::constants::UCDN_SCRIPT_NUSHU;

#[napi]
pub const UCDN_SCRIPT_NYIAKENG_PUACHUE_HMONG: i32 =
    fc::constants::UCDN_SCRIPT_NYIAKENG_PUACHUE_HMONG;

#[napi]
pub const UCDN_SCRIPT_OGHAM: i32 = fc::constants::UCDN_SCRIPT_OGHAM;

#[napi]
pub const UCDN_SCRIPT_OLD_HUNGARIAN: i32 = fc::constants::UCDN_SCRIPT_OLD_HUNGARIAN;

#[napi]
pub const UCDN_SCRIPT_OLD_ITALIC: i32 = fc::constants::UCDN_SCRIPT_OLD_ITALIC;

#[napi]
pub const UCDN_SCRIPT_OLD_NORTH_ARABIAN: i32 = fc::constants::UCDN_SCRIPT_OLD_NORTH_ARABIAN;

#[napi]
pub const UCDN_SCRIPT_OLD_PERMIC: i32 = fc::constants::UCDN_SCRIPT_OLD_PERMIC;

#[napi]
pub const UCDN_SCRIPT_OLD_PERSIAN: i32 = fc::constants::UCDN_SCRIPT_OLD_PERSIAN;

#[napi]
pub const UCDN_SCRIPT_OLD_SOGDIAN: i32 = fc::constants::UCDN_SCRIPT_OLD_SOGDIAN;

#[napi]
pub const UCDN_SCRIPT_OLD_SOUTH_ARABIAN: i32 = fc::constants::UCDN_SCRIPT_OLD_SOUTH_ARABIAN;

#[napi]
pub const UCDN_SCRIPT_OLD_TURKIC: i32 = fc::constants::UCDN_SCRIPT_OLD_TURKIC;

#[napi]
pub const UCDN_SCRIPT_OLD_UYGHUR: i32 = fc::constants::UCDN_SCRIPT_OLD_UYGHUR;

#[napi]
pub const UCDN_SCRIPT_OL_CHIKI: i32 = fc::constants::UCDN_SCRIPT_OL_CHIKI;

#[napi]
pub const UCDN_SCRIPT_OL_ONAL: i32 = fc::constants::UCDN_SCRIPT_OL_ONAL;

#[napi]
pub const UCDN_SCRIPT_ORIYA: i32 = fc::constants::UCDN_SCRIPT_ORIYA;

#[napi]
pub const UCDN_SCRIPT_OSAGE: i32 = fc::constants::UCDN_SCRIPT_OSAGE;

#[napi]
pub const UCDN_SCRIPT_OSMANYA: i32 = fc::constants::UCDN_SCRIPT_OSMANYA;

#[napi]
pub const UCDN_SCRIPT_PAHAWH_HMONG: i32 = fc::constants::UCDN_SCRIPT_PAHAWH_HMONG;

#[napi]
pub const UCDN_SCRIPT_PALMYRENE: i32 = fc::constants::UCDN_SCRIPT_PALMYRENE;

#[napi]
pub const UCDN_SCRIPT_PAU_CIN_HAU: i32 = fc::constants::UCDN_SCRIPT_PAU_CIN_HAU;

#[napi]
pub const UCDN_SCRIPT_PHAGS_PA: i32 = fc::constants::UCDN_SCRIPT_PHAGS_PA;

#[napi]
pub const UCDN_SCRIPT_PHOENICIAN: i32 = fc::constants::UCDN_SCRIPT_PHOENICIAN;

#[napi]
pub const UCDN_SCRIPT_PSALTER_PAHLAVI: i32 = fc::constants::UCDN_SCRIPT_PSALTER_PAHLAVI;

#[napi]
pub const UCDN_SCRIPT_REJANG: i32 = fc::constants::UCDN_SCRIPT_REJANG;

#[napi]
pub const UCDN_SCRIPT_RUNIC: i32 = fc::constants::UCDN_SCRIPT_RUNIC;

#[napi]
pub const UCDN_SCRIPT_SAMARITAN: i32 = fc::constants::UCDN_SCRIPT_SAMARITAN;

#[napi]
pub const UCDN_SCRIPT_SAURASHTRA: i32 = fc::constants::UCDN_SCRIPT_SAURASHTRA;

#[napi]
pub const UCDN_SCRIPT_SHARADA: i32 = fc::constants::UCDN_SCRIPT_SHARADA;

#[napi]
pub const UCDN_SCRIPT_SHAVIAN: i32 = fc::constants::UCDN_SCRIPT_SHAVIAN;

#[napi]
pub const UCDN_SCRIPT_SIDDHAM: i32 = fc::constants::UCDN_SCRIPT_SIDDHAM;

#[napi]
pub const UCDN_SCRIPT_SIGNWRITING: i32 = fc::constants::UCDN_SCRIPT_SIGNWRITING;

#[napi]
pub const UCDN_SCRIPT_SINHALA: i32 = fc::constants::UCDN_SCRIPT_SINHALA;

#[napi]
pub const UCDN_SCRIPT_SOGDIAN: i32 = fc::constants::UCDN_SCRIPT_SOGDIAN;

#[napi]
pub const UCDN_SCRIPT_SORA_SOMPENG: i32 = fc::constants::UCDN_SCRIPT_SORA_SOMPENG;

#[napi]
pub const UCDN_SCRIPT_SOYOMBO: i32 = fc::constants::UCDN_SCRIPT_SOYOMBO;

#[napi]
pub const UCDN_SCRIPT_SUNDANESE: i32 = fc::constants::UCDN_SCRIPT_SUNDANESE;

#[napi]
pub const UCDN_SCRIPT_SUNUWAR: i32 = fc::constants::UCDN_SCRIPT_SUNUWAR;

#[napi]
pub const UCDN_SCRIPT_SYLOTI_NAGRI: i32 = fc::constants::UCDN_SCRIPT_SYLOTI_NAGRI;

#[napi]
pub const UCDN_SCRIPT_SYRIAC: i32 = fc::constants::UCDN_SCRIPT_SYRIAC;

#[napi]
pub const UCDN_SCRIPT_TAGALOG: i32 = fc::constants::UCDN_SCRIPT_TAGALOG;

#[napi]
pub const UCDN_SCRIPT_TAGBANWA: i32 = fc::constants::UCDN_SCRIPT_TAGBANWA;

#[napi]
pub const UCDN_SCRIPT_TAI_LE: i32 = fc::constants::UCDN_SCRIPT_TAI_LE;

#[napi]
pub const UCDN_SCRIPT_TAI_THAM: i32 = fc::constants::UCDN_SCRIPT_TAI_THAM;

#[napi]
pub const UCDN_SCRIPT_TAI_VIET: i32 = fc::constants::UCDN_SCRIPT_TAI_VIET;

#[napi]
pub const UCDN_SCRIPT_TAKRI: i32 = fc::constants::UCDN_SCRIPT_TAKRI;

#[napi]
pub const UCDN_SCRIPT_TAMIL: i32 = fc::constants::UCDN_SCRIPT_TAMIL;

#[napi]
pub const UCDN_SCRIPT_TANGSA: i32 = fc::constants::UCDN_SCRIPT_TANGSA;

#[napi]
pub const UCDN_SCRIPT_TANGUT: i32 = fc::constants::UCDN_SCRIPT_TANGUT;

#[napi]
pub const UCDN_SCRIPT_TELUGU: i32 = fc::constants::UCDN_SCRIPT_TELUGU;

#[napi]
pub const UCDN_SCRIPT_THAANA: i32 = fc::constants::UCDN_SCRIPT_THAANA;

#[napi]
pub const UCDN_SCRIPT_THAI: i32 = fc::constants::UCDN_SCRIPT_THAI;

#[napi]
pub const UCDN_SCRIPT_TIBETAN: i32 = fc::constants::UCDN_SCRIPT_TIBETAN;

#[napi]
pub const UCDN_SCRIPT_TIFINAGH: i32 = fc::constants::UCDN_SCRIPT_TIFINAGH;

#[napi]
pub const UCDN_SCRIPT_TIRHUTA: i32 = fc::constants::UCDN_SCRIPT_TIRHUTA;

#[napi]
pub const UCDN_SCRIPT_TODHRI: i32 = fc::constants::UCDN_SCRIPT_TODHRI;

#[napi]
pub const UCDN_SCRIPT_TOTO: i32 = fc::constants::UCDN_SCRIPT_TOTO;

#[napi]
pub const UCDN_SCRIPT_TULU_TIGALARI: i32 = fc::constants::UCDN_SCRIPT_TULU_TIGALARI;

#[napi]
pub const UCDN_SCRIPT_UGARITIC: i32 = fc::constants::UCDN_SCRIPT_UGARITIC;

#[napi]
pub const UCDN_SCRIPT_UNKNOWN: i32 = fc::constants::UCDN_SCRIPT_UNKNOWN;

#[napi]
pub const UCDN_SCRIPT_VAI: i32 = fc::constants::UCDN_SCRIPT_VAI;

#[napi]
pub const UCDN_SCRIPT_VITHKUQI: i32 = fc::constants::UCDN_SCRIPT_VITHKUQI;

#[napi]
pub const UCDN_SCRIPT_WANCHO: i32 = fc::constants::UCDN_SCRIPT_WANCHO;

#[napi]
pub const UCDN_SCRIPT_WARANG_CITI: i32 = fc::constants::UCDN_SCRIPT_WARANG_CITI;

#[napi]
pub const UCDN_SCRIPT_YEZIDI: i32 = fc::constants::UCDN_SCRIPT_YEZIDI;

#[napi]
pub const UCDN_SCRIPT_YI: i32 = fc::constants::UCDN_SCRIPT_YI;

#[napi]
pub const UCDN_SCRIPT_ZANABAZAR_SQUARE: i32 = fc::constants::UCDN_SCRIPT_ZANABAZAR_SQUARE;

#[napi]
pub const PDF_CID_FONT_RESOURCE: i32 = fc::constants::PDF_CID_FONT_RESOURCE;

#[napi]
pub const PDF_CJK_FONT_RESOURCE: i32 = fc::constants::PDF_CJK_FONT_RESOURCE;

#[napi]
pub const PDF_FD_ALL_CAP: i32 = fc::constants::PDF_FD_ALL_CAP;

#[napi]
pub const PDF_FD_FIXED_PITCH: i32 = fc::constants::PDF_FD_FIXED_PITCH;

#[napi]
pub const PDF_FD_FORCE_BOLD: i32 = fc::constants::PDF_FD_FORCE_BOLD;

#[napi]
pub const PDF_FD_ITALIC: i32 = fc::constants::PDF_FD_ITALIC;

#[napi]
pub const PDF_FD_NONSYMBOLIC: i32 = fc::constants::PDF_FD_NONSYMBOLIC;

#[napi]
pub const PDF_FD_SCRIPT: i32 = fc::constants::PDF_FD_SCRIPT;

#[napi]
pub const PDF_FD_SERIF: i32 = fc::constants::PDF_FD_SERIF;

#[napi]
pub const PDF_FD_SMALL_CAP: i32 = fc::constants::PDF_FD_SMALL_CAP;

#[napi]
pub const PDF_FD_SYMBOLIC: i32 = fc::constants::PDF_FD_SYMBOLIC;

#[napi]
pub const PDF_SIMPLE_ENCODING_CYRILLIC: i32 = fc::constants::PDF_SIMPLE_ENCODING_CYRILLIC;

#[napi]
pub const PDF_SIMPLE_ENCODING_GREEK: i32 = fc::constants::PDF_SIMPLE_ENCODING_GREEK;

#[napi]
pub const PDF_SIMPLE_ENCODING_LATIN: i32 = fc::constants::PDF_SIMPLE_ENCODING_LATIN;

#[napi]
pub const PDF_SIMPLE_FONT_RESOURCE: i32 = fc::constants::PDF_SIMPLE_FONT_RESOURCE;

#[napi]
pub const PDF_ENUM_FALSE: i32 = fc::constants::PDF_ENUM_FALSE;

#[napi]
pub const PDF_ENUM_LIMIT: i32 = fc::constants::PDF_ENUM_LIMIT;

#[napi]
pub const PDF_ENUM_NULL: i32 = fc::constants::PDF_ENUM_NULL;

#[napi]
pub const PDF_ENUM_TRUE: i32 = fc::constants::PDF_ENUM_TRUE;

#[napi]
pub const PDF_LEXBUF_LARGE: i32 = fc::constants::PDF_LEXBUF_LARGE;

#[napi]
pub const PDF_LEXBUF_SMALL: i32 = fc::constants::PDF_LEXBUF_SMALL;

#[napi]
pub const PDF_MAX_GEN_NUMBER: i32 = fc::constants::PDF_MAX_GEN_NUMBER;

#[napi]
pub const PDF_MAX_OBJECT_NUMBER: i32 = fc::constants::PDF_MAX_OBJECT_NUMBER;

#[napi]
pub const PDF_MRANGE_CAP: i32 = fc::constants::PDF_MRANGE_CAP;

#[napi]
pub const PDF_NUM_TOKENS: i32 = fc::constants::PDF_NUM_TOKENS;

#[napi]
pub const PDF_PROCESSOR_REQUIRES_DECODED_IMAGES: i32 =
    fc::constants::PDF_PROCESSOR_REQUIRES_DECODED_IMAGES;

#[napi]
pub const PDF_TOK_CLOSE_ARRAY: i32 = fc::constants::PDF_TOK_CLOSE_ARRAY;

#[napi]
pub const PDF_TOK_CLOSE_BRACE: i32 = fc::constants::PDF_TOK_CLOSE_BRACE;

#[napi]
pub const PDF_TOK_CLOSE_DICT: i32 = fc::constants::PDF_TOK_CLOSE_DICT;

#[napi]
pub const PDF_TOK_ENDOBJ: i32 = fc::constants::PDF_TOK_ENDOBJ;

#[napi]
pub const PDF_TOK_ENDSTREAM: i32 = fc::constants::PDF_TOK_ENDSTREAM;

#[napi]
pub const PDF_TOK_EOF: i32 = fc::constants::PDF_TOK_EOF;

#[napi]
pub const PDF_TOK_ERROR: i32 = fc::constants::PDF_TOK_ERROR;

#[napi]
pub const PDF_TOK_FALSE: i32 = fc::constants::PDF_TOK_FALSE;

#[napi]
pub const PDF_TOK_INT: i32 = fc::constants::PDF_TOK_INT;

#[napi]
pub const PDF_TOK_KEYWORD: i32 = fc::constants::PDF_TOK_KEYWORD;

#[napi]
pub const PDF_TOK_NAME: i32 = fc::constants::PDF_TOK_NAME;

#[napi]
pub const PDF_TOK_NEWOBJ: i32 = fc::constants::PDF_TOK_NEWOBJ;

#[napi]
pub const PDF_TOK_NULL: i32 = fc::constants::PDF_TOK_NULL;

#[napi]
pub const PDF_TOK_OBJ: i32 = fc::constants::PDF_TOK_OBJ;

#[napi]
pub const PDF_TOK_OPEN_ARRAY: i32 = fc::constants::PDF_TOK_OPEN_ARRAY;

#[napi]
pub const PDF_TOK_OPEN_BRACE: i32 = fc::constants::PDF_TOK_OPEN_BRACE;

#[napi]
pub const PDF_TOK_OPEN_DICT: i32 = fc::constants::PDF_TOK_OPEN_DICT;

#[napi]
pub const PDF_TOK_R: i32 = fc::constants::PDF_TOK_R;

#[napi]
pub const PDF_TOK_REAL: i32 = fc::constants::PDF_TOK_REAL;

#[napi]
pub const PDF_TOK_STARTXREF: i32 = fc::constants::PDF_TOK_STARTXREF;

#[napi]
pub const PDF_TOK_STREAM: i32 = fc::constants::PDF_TOK_STREAM;

#[napi]
pub const PDF_TOK_STRING: i32 = fc::constants::PDF_TOK_STRING;

#[napi]
pub const PDF_TOK_TRAILER: i32 = fc::constants::PDF_TOK_TRAILER;

#[napi]
pub const PDF_TOK_TRUE: i32 = fc::constants::PDF_TOK_TRUE;

#[napi]
pub const PDF_TOK_XREF: i32 = fc::constants::PDF_TOK_XREF;

#[napi]
pub const b: i32 = fc::constants::b;

#[napi]
pub const f: &str = fc::constants::f;

#[napi]
pub const g: i32 = fc::constants::g;

#[napi]
pub const matrix_like: &str = fc::constants::matrix_like;

#[napi]
pub const name: &str = fc::constants::name;

#[napi]
pub const point_like: &str = fc::constants::point_like;

#[napi]
pub const quad_like: &str = fc::constants::quad_like;

#[napi]
pub const r: i32 = fc::constants::r;

#[napi]
pub const rect_like: &str = fc::constants::rect_like;

#[napi]
pub const trace_device_CLIP_PATH: i32 = fc::constants::trace_device_CLIP_PATH;

#[napi]
pub const trace_device_CLIP_STROKE_PATH: i32 = fc::constants::trace_device_CLIP_STROKE_PATH;

#[napi]
pub const trace_device_FILL_PATH: i32 = fc::constants::trace_device_FILL_PATH;

#[napi]
pub const trace_device_STROKE_PATH: i32 = fc::constants::trace_device_STROKE_PATH;

/// The identity matrix.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "Identity")]
pub fn identity_matrix() -> Matrix {
    Matrix {
        inner: fc::constants::Identity,
    }
}

/// The gray colorspace.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "csGRAY")]
pub fn cs_gray() -> Colorspace {
    Colorspace {
        inner: fc::constants::csGRAY,
    }
}

/// The red-green-blue colorspace.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "csRGB")]
pub fn cs_rgb() -> Colorspace {
    Colorspace {
        inner: fc::constants::csRGB,
    }
}

/// The cyan-magenta-yellow-black colorspace.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "csCMYK")]
pub fn cs_cmyk() -> Colorspace {
    Colorspace {
        inner: fc::constants::csCMYK,
    }
}

/// The standard fourteen font names.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "Base14_fontnames")]
pub fn base14_fontnames() -> Vec<String> {
    fc::constants::Base14_fontnames
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// The standard fourteen font name pairs.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "Base14_fontdict")]
pub fn base14_fontdict() -> serde_json::Value {
    json_array(
        fc::constants::Base14_fontdict
            .iter()
            .map(|(a, v)| serde_json::json!([a, v]))
            .collect(),
    )
}

/// Symbol font glyph widths.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "symbol_glyphs")]
pub fn symbol_glyphs() -> serde_json::Value {
    json_array(
        fc::constants::symbol_glyphs
            .iter()
            .map(|(a, v)| serde_json::json!([a, v]))
            .collect(),
    )
}

/// Zapf dingbats glyph widths.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "zapf_glyphs")]
pub fn zapf_glyphs() -> serde_json::Value {
    json_array(
        fc::constants::zapf_glyphs
            .iter()
            .map(|(a, v)| serde_json::json!([a, v]))
            .collect(),
    )
}

/// The named PDF color table.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "pdfcolor")]
pub fn pdfcolor() -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (label, triple) in fc::constants::pdfcolor.iter() {
        map.insert(
            label.to_string(),
            serde_json::json!([triple[0], triple[1], triple[2]]),
        );
    }
    serde_json::Value::Object(map)
}

/// Characters forbidden in PDF names.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "INVALID_NAME_CHARS")]
pub fn invalid_name_chars() -> Vec<String> {
    fc::constants::INVALID_NAME_CHARS
        .iter()
        .map(|s| s.to_string())
        .collect()
}
