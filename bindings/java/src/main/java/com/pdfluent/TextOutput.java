// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

import com.sun.jna.Pointer;
import com.sun.jna.ptr.DoubleByReference;
import com.sun.jna.ptr.IntByReference;
import com.sun.jna.ptr.PointerByReference;

import static com.pdfluent.CompatSupport.check;
import static com.pdfluent.CompatSupport.lib;
import static com.pdfluent.CompatSupport.readString;

/**
 * A {@code getText} result in the requested format.
 *
 * <p>Only the accessor matching {@link #getFormat()} holds content; the
 * others return null or empty lists. Format tags: 0 text, 1 words, 2 blocks,
 * 3 dict, 4 json, 5 raw json, 6 html, 7 xhtml, 8 xml.
 */
public final class TextOutput {
    /** Text format tag. */
    public static final int FORMAT_TEXT = 0;
    /** Words format tag. */
    public static final int FORMAT_WORDS = 1;
    /** Blocks format tag. */
    public static final int FORMAT_BLOCKS = 2;
    /** Dict format tag. */
    public static final int FORMAT_DICT = 3;
    /** JSON format tag. */
    public static final int FORMAT_JSON = 4;
    /** Raw JSON format tag. */
    public static final int FORMAT_RAW_JSON = 5;
    /** HTML format tag. */
    public static final int FORMAT_HTML = 6;
    /** XHTML format tag. */
    public static final int FORMAT_XHTML = 7;
    /** XML format tag. */
    public static final int FORMAT_XML = 8;

    private final int format;
    private final String text;
    private final java.util.List<Word> words;
    private final java.util.List<Block> blocks;
    private final Dict dict;

    TextOutput(int format, String text, java.util.List<Word> words,
            java.util.List<Block> blocks, Dict dict) {
        this.format = format;
        this.text = text;
        this.words = words;
        this.blocks = blocks;
        this.dict = dict;
    }

    /** Result format tag. */
    public int getFormat() { return format; }

    /** Result text for the string formats, or null. */
    public String getText() { return text; }

    /** Words for the words format. */
    public java.util.List<Word> getWords() { return words; }

    /** Blocks for the blocks format. */
    public java.util.List<Block> getBlocks() { return blocks; }

    /** Page dict for the dict format, or null. */
    public Dict getDict() { return dict; }

    /** One word: box, text, and position numbers. */
    public static final class Word {
        private final Rect rect;
        private final String text;
        private final int block;
        private final int line;
        private final int number;

        /** Build a word. */
        public Word(Rect rect, String text, int block, int line, int number) {
            this.rect = rect;
            this.text = text;
            this.block = block;
            this.line = line;
            this.number = number;
        }

        /** Word box. */
        public Rect getRect() { return rect; }

        /** Word text. */
        public String getText() { return text; }

        /** Block number. */
        public int getBlock() { return block; }

        /** Line number. */
        public int getLine() { return line; }

        /** Word number. */
        public int getNumber() { return number; }
    }

    /** One block: box, text, number, and type. */
    public static final class Block {
        private final Rect rect;
        private final String text;
        private final int number;
        private final int type;

        /** Build a block. */
        public Block(Rect rect, String text, int number, int type) {
            this.rect = rect;
            this.text = text;
            this.number = number;
            this.type = type;
        }

        /** Block box. */
        public Rect getRect() { return rect; }

        /** Block text. */
        public String getText() { return text; }

        /** Block number. */
        public int getNumber() { return number; }

        /** Block type. */
        public int getType() { return type; }
    }

    /** One page dict: size plus text and image blocks. */
    public static final class Dict {
        private final double width;
        private final double height;
        private final java.util.List<DictBlock> blocks;

        /** Build a page dict. */
        public Dict(double width, double height, java.util.List<DictBlock> blocks) {
            this.width = width;
            this.height = height;
            this.blocks = blocks;
        }

        /** Page width. */
        public double getWidth() { return width; }

        /** Page height. */
        public double getHeight() { return height; }

        /** Dict blocks. */
        public java.util.List<DictBlock> getBlocks() { return blocks; }
    }

    /** One dict block: text (tag 0) or image (tag 1). */
    public static final class DictBlock {
        private final int tag;
        private final Rect bbox;
        private final int number;
        private final int flags;
        private final java.util.List<DictLine> lines;
        private final int width;
        private final int height;

        /** Build a dict block. */
        public DictBlock(int tag, Rect bbox, int number, int flags,
                java.util.List<DictLine> lines, int width, int height) {
            this.tag = tag;
            this.bbox = bbox;
            this.number = number;
            this.flags = flags;
            this.lines = lines;
            this.width = width;
            this.height = height;
        }

        /** Block tag: 0 text, 1 image. */
        public int getTag() { return tag; }

        /** Block box. */
        public Rect getBbox() { return bbox; }

        /** Block number. */
        public int getNumber() { return number; }

        /** Text block flags. */
        public int getFlags() { return flags; }

        /** Text block lines. */
        public java.util.List<DictLine> getLines() { return lines; }

        /** Image width in pixels. */
        public int getWidth() { return width; }

        /** Image height in pixels. */
        public int getHeight() { return height; }
    }

    /** One dict line: box, writing mode, direction, and spans. */
    public static final class DictLine {
        private final Rect bbox;
        private final int wmode;
        private final Point direction;
        private final java.util.List<DictSpan> spans;

        /** Build a dict line. */
        public DictLine(Rect bbox, int wmode, Point direction, java.util.List<DictSpan> spans) {
            this.bbox = bbox;
            this.wmode = wmode;
            this.direction = direction;
            this.spans = spans;
        }

        /** Line box. */
        public Rect getBbox() { return bbox; }

        /** Writing mode. */
        public int getWmode() { return wmode; }

        /** Line direction. */
        public Point getDirection() { return direction; }

        /** Line spans. */
        public java.util.List<DictSpan> getSpans() { return spans; }
    }

    /** One dict span: font run with metrics and optional characters. */
    public static final class DictSpan {
        private final Rect bbox;
        private final Point origin;
        private final String font;
        private final double size;
        private final int color;
        private final int alpha;
        private final int flags;
        private final int bidi;
        private final int charFlags;
        private final double ascender;
        private final double descender;
        private final String text;
        private final java.util.List<RawChar> chars;

        /** Build a dict span. */
        public DictSpan(Rect bbox, Point origin, String font, double size, int color,
                int alpha, int flags, int bidi, int charFlags, double ascender,
                double descender, String text, java.util.List<RawChar> chars) {
            this.bbox = bbox;
            this.origin = origin;
            this.font = font;
            this.size = size;
            this.color = color;
            this.alpha = alpha;
            this.flags = flags;
            this.bidi = bidi;
            this.charFlags = charFlags;
            this.ascender = ascender;
            this.descender = descender;
            this.text = text;
            this.chars = chars;
        }

        /** Span box. */
        public Rect getBbox() { return bbox; }

        /** Span origin. */
        public Point getOrigin() { return origin; }

        /** Font name. */
        public String getFont() { return font; }

        /** Font size. */
        public double getSize() { return size; }

        /** Span colour. */
        public int getColor() { return color; }

        /** Span alpha. */
        public int getAlpha() { return alpha; }

        /** Span flags. */
        public int getFlags() { return flags; }

        /** Span bidi level. */
        public int getBidi() { return bidi; }

        /** Character flags. */
        public int getCharFlags() { return charFlags; }

        /** Ascender. */
        public double getAscender() { return ascender; }

        /** Descender. */
        public double getDescender() { return descender; }

        /** Span text. */
        public String getText() { return text; }

        /** Span characters (raw dicts only). */
        public java.util.List<RawChar> getChars() { return chars; }
    }

    /** One raw character: box, text, origin, and synthetic flag. */
    public static final class RawChar {
        private final Rect bbox;
        private final String text;
        private final Point origin;
        private final boolean synthetic;

        /** Build a raw character. */
        public RawChar(Rect bbox, String text, Point origin, boolean synthetic) {
            this.bbox = bbox;
            this.text = text;
            this.origin = origin;
            this.synthetic = synthetic;
        }

        /** Character box. */
        public Rect getBbox() { return bbox; }

        /** Character text. */
        public String getText() { return text; }

        /** Character origin. */
        public Point getOrigin() { return origin; }

        /** Whether synthetic. */
        public boolean isSynthetic() { return synthetic; }
    }

    static TextOutput read(Pointer handle) {
        try {
            IntByReference format = new IntByReference();
            check(lib().pdf_compat_text_output_format(handle, format), "read");
            switch (format.getValue()) {
                case FORMAT_WORDS:
                    return new TextOutput(FORMAT_WORDS, null, readWords(handle),
                        new java.util.ArrayList<>(), null);
                case FORMAT_BLOCKS:
                    return new TextOutput(FORMAT_BLOCKS, null, new java.util.ArrayList<>(),
                        readBlocks(handle), null);
                case FORMAT_DICT:
                    return new TextOutput(FORMAT_DICT, null, new java.util.ArrayList<>(),
                        new java.util.ArrayList<>(), readDict(handle));
                default:
                    return new TextOutput(format.getValue(), readText(handle),
                        new java.util.ArrayList<>(), new java.util.ArrayList<>(), null);
            }
        } finally {
            lib().pdf_compat_text_output_free(handle);
        }
    }

    private static String readText(Pointer handle) {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_text_output_text(handle, out), "read");
        return readString(out);
    }

    private static java.util.List<Word> readWords(Pointer handle) {
        IntByReference count = new IntByReference();
        check(lib().pdf_compat_text_output_word_count(handle, count), "read");
        java.util.List<Word> result = new java.util.ArrayList<>();
        for (int i = 0; i < count.getValue(); i++) {
            PdfCapiLibrary.PdfCompatRect rect = new PdfCapiLibrary.PdfCompatRect();
            check(lib().pdf_compat_text_output_word_rect(handle, i, rect), "read");
            PointerByReference text = new PointerByReference();
            check(lib().pdf_compat_text_output_word_text(handle, i, text), "read");
            IntByReference block = new IntByReference();
            check(lib().pdf_compat_text_output_word_block(handle, i, block), "read");
            IntByReference line = new IntByReference();
            check(lib().pdf_compat_text_output_word_line(handle, i, line), "read");
            IntByReference number = new IntByReference();
            check(lib().pdf_compat_text_output_word_number(handle, i, number), "read");
            result.add(new Word(Rect.fromNative(rect), readString(text), block.getValue(),
                line.getValue(), number.getValue()));
        }
        return result;
    }

    private static java.util.List<Block> readBlocks(Pointer handle) {
        IntByReference count = new IntByReference();
        check(lib().pdf_compat_text_output_block_count(handle, count), "read");
        java.util.List<Block> result = new java.util.ArrayList<>();
        for (int i = 0; i < count.getValue(); i++) {
            PdfCapiLibrary.PdfCompatRect rect = new PdfCapiLibrary.PdfCompatRect();
            check(lib().pdf_compat_text_output_block_rect(handle, i, rect), "read");
            PointerByReference text = new PointerByReference();
            check(lib().pdf_compat_text_output_block_text(handle, i, text), "read");
            IntByReference number = new IntByReference();
            check(lib().pdf_compat_text_output_block_number(handle, i, number), "read");
            IntByReference type = new IntByReference();
            check(lib().pdf_compat_text_output_block_type(handle, i, type), "read");
            result.add(new Block(Rect.fromNative(rect), readString(text), number.getValue(),
                type.getValue()));
        }
        return result;
    }

    private static Dict readDict(Pointer handle) {
        DoubleByReference width = new DoubleByReference();
        check(lib().pdf_compat_text_output_dict_width(handle, width), "read");
        DoubleByReference height = new DoubleByReference();
        check(lib().pdf_compat_text_output_dict_height(handle, height), "read");
        IntByReference count = new IntByReference();
        check(lib().pdf_compat_text_output_dict_block_count(handle, count), "read");
        java.util.List<DictBlock> blocks = new java.util.ArrayList<>();
        for (int i = 0; i < count.getValue(); i++) {
            IntByReference tag = new IntByReference();
            check(lib().pdf_compat_text_output_dict_block_tag(handle, i, tag), "read");
            if (tag.getValue() == 0) {
                blocks.add(readTextBlock(handle, i));
            } else {
                blocks.add(readImageBlock(handle, i));
            }
        }
        return new Dict(width.getValue(), height.getValue(), blocks);
    }

    private static DictBlock readTextBlock(Pointer handle, int i) {
        PdfCapiLibrary.PdfCompatRect bbox = new PdfCapiLibrary.PdfCompatRect();
        check(lib().pdf_compat_text_output_dict_text_bbox(handle, i, bbox), "read");
        IntByReference number = new IntByReference();
        check(lib().pdf_compat_text_output_dict_text_number(handle, i, number), "read");
        IntByReference flags = new IntByReference();
        check(lib().pdf_compat_text_output_dict_text_flags(handle, i, flags), "read");
        IntByReference lines = new IntByReference();
        check(lib().pdf_compat_text_output_dict_line_count(handle, i, lines), "read");
        java.util.List<DictLine> result = new java.util.ArrayList<>();
        for (int l = 0; l < lines.getValue(); l++) {
            result.add(readLine(handle, i, l));
        }
        return new DictBlock(0, Rect.fromNative(bbox), number.getValue(), flags.getValue(),
            result, 0, 0);
    }

    private static DictBlock readImageBlock(Pointer handle, int i) {
        PdfCapiLibrary.PdfCompatRect bbox = new PdfCapiLibrary.PdfCompatRect();
        check(lib().pdf_compat_text_output_dict_image_bbox(handle, i, bbox), "read");
        IntByReference number = new IntByReference();
        check(lib().pdf_compat_text_output_dict_image_number(handle, i, number), "read");
        IntByReference width = new IntByReference();
        check(lib().pdf_compat_text_output_dict_image_width(handle, i, width), "read");
        IntByReference height = new IntByReference();
        check(lib().pdf_compat_text_output_dict_image_height(handle, i, height), "read");
        return new DictBlock(1, Rect.fromNative(bbox), number.getValue(), 0,
            new java.util.ArrayList<>(), width.getValue(), height.getValue());
    }

    private static DictLine readLine(Pointer handle, int i, int l) {
        PdfCapiLibrary.PdfCompatRect bbox = new PdfCapiLibrary.PdfCompatRect();
        check(lib().pdf_compat_text_output_dict_line_bbox(handle, i, l, bbox), "read");
        IntByReference wmode = new IntByReference();
        check(lib().pdf_compat_text_output_dict_line_wmode(handle, i, l, wmode), "read");
        PdfCapiLibrary.PdfCompatPoint direction = new PdfCapiLibrary.PdfCompatPoint();
        check(lib().pdf_compat_text_output_dict_line_direction(handle, i, l, direction), "read");
        IntByReference spans = new IntByReference();
        check(lib().pdf_compat_text_output_dict_span_count(handle, i, l, spans), "read");
        java.util.List<DictSpan> result = new java.util.ArrayList<>();
        for (int s = 0; s < spans.getValue(); s++) {
            result.add(readSpan(handle, i, l, s));
        }
        return new DictLine(Rect.fromNative(bbox), wmode.getValue(),
            Point.fromNative(direction), result);
    }

    private static DictSpan readSpan(Pointer handle, int i, int l, int s) {
        PdfCapiLibrary.PdfCompatRect bbox = new PdfCapiLibrary.PdfCompatRect();
        check(lib().pdf_compat_text_output_dict_span_bbox(handle, i, l, s, bbox), "read");
        PdfCapiLibrary.PdfCompatPoint origin = new PdfCapiLibrary.PdfCompatPoint();
        check(lib().pdf_compat_text_output_dict_span_origin(handle, i, l, s, origin), "read");
        PointerByReference font = new PointerByReference();
        check(lib().pdf_compat_text_output_dict_span_font(handle, i, l, s, font), "read");
        DoubleByReference size = new DoubleByReference();
        check(lib().pdf_compat_text_output_dict_span_size(handle, i, l, s, size), "read");
        int color = readSpanInt(handle, i, l, s, "color");
        int alpha = readSpanInt(handle, i, l, s, "alpha");
        int flags = readSpanInt(handle, i, l, s, "flags");
        int bidi = readSpanInt(handle, i, l, s, "bidi");
        int charFlags = readSpanInt(handle, i, l, s, "char_flags");
        DoubleByReference ascender = new DoubleByReference();
        check(lib().pdf_compat_text_output_dict_span_ascender(handle, i, l, s, ascender), "read");
        DoubleByReference descender = new DoubleByReference();
        check(lib().pdf_compat_text_output_dict_span_descender(handle, i, l, s, descender), "read");
        PointerByReference text = new PointerByReference();
        check(lib().pdf_compat_text_output_dict_span_text(handle, i, l, s, text), "read");
        IntByReference chars = new IntByReference();
        check(lib().pdf_compat_text_output_dict_span_char_count(handle, i, l, s, chars), "read");
        java.util.List<RawChar> result = new java.util.ArrayList<>();
        for (int c = 0; c < chars.getValue(); c++) {
            PdfCapiLibrary.PdfCompatRect charBox = new PdfCapiLibrary.PdfCompatRect();
            check(lib().pdf_compat_text_output_dict_char_bbox(handle, i, l, s, c, charBox), "read");
            PointerByReference charText = new PointerByReference();
            check(lib().pdf_compat_text_output_dict_char_text(handle, i, l, s, c, charText), "read");
            PdfCapiLibrary.PdfCompatPoint charOrigin = new PdfCapiLibrary.PdfCompatPoint();
            check(lib().pdf_compat_text_output_dict_char_origin(handle, i, l, s, c, charOrigin), "read");
            IntByReference synthetic = new IntByReference();
            check(lib().pdf_compat_text_output_dict_char_synthetic(handle, i, l, s, c, synthetic), "read");
            result.add(new RawChar(Rect.fromNative(charBox), readString(charText),
                Point.fromNative(charOrigin), synthetic.getValue() != 0));
        }
        return new DictSpan(Rect.fromNative(bbox), Point.fromNative(origin), readString(font),
            size.getValue(), color, alpha, flags, bidi, charFlags, ascender.getValue(),
            descender.getValue(), readString(text), result);
    }

    private static int readSpanInt(Pointer handle, int i, int l, int s, String which) {
        IntByReference out = new IntByReference();
        switch (which) {
            case "color":
                check(lib().pdf_compat_text_output_dict_span_color(handle, i, l, s, out), "read");
                break;
            case "alpha":
                check(lib().pdf_compat_text_output_dict_span_alpha(handle, i, l, s, out), "read");
                break;
            case "flags":
                check(lib().pdf_compat_text_output_dict_span_flags(handle, i, l, s, out), "read");
                break;
            case "bidi":
                check(lib().pdf_compat_text_output_dict_span_bidi(handle, i, l, s, out), "read");
                break;
            default:
                check(lib().pdf_compat_text_output_dict_span_char_flags(handle, i, l, s, out), "read");
                break;
        }
        return out.getValue();
    }
}
