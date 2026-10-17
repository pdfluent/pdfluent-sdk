// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;

namespace PDFluent
{
    /// <summary>
    /// A <c>GetText</c> result in the requested format.
    ///
    /// <para>Only the accessor matching <see cref="Format"/> holds content; the
    /// others return null or empty lists. Format tags: 0 text, 1 words, 2 blocks,
    /// 3 dict, 4 json, 5 raw json, 6 html, 7 xhtml, 8 xml.</para>
    /// </summary>
    public sealed class TextOutput
    {
        /// <summary>Text format tag.</summary>
        public const int FormatText = 0;

        /// <summary>Words format tag.</summary>
        public const int FormatWords = 1;

        /// <summary>Blocks format tag.</summary>
        public const int FormatBlocks = 2;

        /// <summary>Dict format tag.</summary>
        public const int FormatDict = 3;

        /// <summary>JSON format tag.</summary>
        public const int FormatJson = 4;

        /// <summary>Raw JSON format tag.</summary>
        public const int FormatRawJson = 5;

        /// <summary>HTML format tag.</summary>
        public const int FormatHtml = 6;

        /// <summary>XHTML format tag.</summary>
        public const int FormatXhtml = 7;

        /// <summary>XML format tag.</summary>
        public const int FormatXml = 8;

        /// <summary>Result format tag.</summary>
        public int Format { get; }

        /// <summary>Result text for the string formats, or null.</summary>
        public string? Text { get; }

        /// <summary>Words for the words format.</summary>
        public List<Word> Words { get; }

        /// <summary>Blocks for the blocks format.</summary>
        public List<Block> Blocks { get; }

        /// <summary>Page dict for the dict format, or null.</summary>
        public Dict? PageDict { get; }

        internal TextOutput(int format, string? text, List<Word> words,
            List<Block> blocks, Dict? dict)
        {
            Format = format;
            Text = text;
            Words = words;
            Blocks = blocks;
            PageDict = dict;
        }

        /// <summary>One word: box, text, and position numbers.</summary>
        public sealed class Word
        {
            /// <summary>Word box.</summary>
            public Rect Rect { get; }

            /// <summary>Word text.</summary>
            public string Text { get; }

            /// <summary>Block number.</summary>
            public int Block { get; }

            /// <summary>Line number.</summary>
            public int Line { get; }

            /// <summary>Word number.</summary>
            public int Number { get; }

            /// <summary>Build a word.</summary>
            public Word(Rect rect, string text, int block, int line, int number)
            {
                Rect = rect;
                Text = text;
                Block = block;
                Line = line;
                Number = number;
            }
        }

        /// <summary>One block: box, text, number, and type.</summary>
        public sealed class Block
        {
            /// <summary>Block box.</summary>
            public Rect Rect { get; }

            /// <summary>Block text.</summary>
            public string Text { get; }

            /// <summary>Block number.</summary>
            public int Number { get; }

            /// <summary>Block type.</summary>
            public int Type { get; }

            /// <summary>Build a block.</summary>
            public Block(Rect rect, string text, int number, int type)
            {
                Rect = rect;
                Text = text;
                Number = number;
                Type = type;
            }
        }

        /// <summary>One page dict: size plus text and image blocks.</summary>
        public sealed class Dict
        {
            /// <summary>Page width.</summary>
            public double Width { get; }

            /// <summary>Page height.</summary>
            public double Height { get; }

            /// <summary>Dict blocks.</summary>
            public List<DictBlock> Blocks { get; }

            /// <summary>Build a page dict.</summary>
            public Dict(double width, double height, List<DictBlock> blocks)
            {
                Width = width;
                Height = height;
                Blocks = blocks;
            }
        }

        /// <summary>One dict block: text (tag 0) or image (tag 1).</summary>
        public sealed class DictBlock
        {
            /// <summary>Block tag: 0 text, 1 image.</summary>
            public int Tag { get; }

            /// <summary>Block box.</summary>
            public Rect Bbox { get; }

            /// <summary>Block number.</summary>
            public int Number { get; }

            /// <summary>Text block flags.</summary>
            public int Flags { get; }

            /// <summary>Text block lines.</summary>
            public List<DictLine> Lines { get; }

            /// <summary>Image width in pixels.</summary>
            public int Width { get; }

            /// <summary>Image height in pixels.</summary>
            public int Height { get; }

            /// <summary>Build a dict block.</summary>
            public DictBlock(int tag, Rect bbox, int number, int flags,
                List<DictLine> lines, int width, int height)
            {
                Tag = tag;
                Bbox = bbox;
                Number = number;
                Flags = flags;
                Lines = lines;
                Width = width;
                Height = height;
            }
        }

        /// <summary>One dict line: box, writing mode, direction, and spans.</summary>
        public sealed class DictLine
        {
            /// <summary>Line box.</summary>
            public Rect Bbox { get; }

            /// <summary>Writing mode.</summary>
            public int Wmode { get; }

            /// <summary>Line direction.</summary>
            public Point Direction { get; }

            /// <summary>Line spans.</summary>
            public List<DictSpan> Spans { get; }

            /// <summary>Build a dict line.</summary>
            public DictLine(Rect bbox, int wmode, Point direction, List<DictSpan> spans)
            {
                Bbox = bbox;
                Wmode = wmode;
                Direction = direction;
                Spans = spans;
            }
        }

        /// <summary>One dict span: font run with metrics and optional characters.</summary>
        public sealed class DictSpan
        {
            /// <summary>Span box.</summary>
            public Rect Bbox { get; }

            /// <summary>Span origin.</summary>
            public Point Origin { get; }

            /// <summary>Font name.</summary>
            public string Font { get; }

            /// <summary>Font size.</summary>
            public double Size { get; }

            /// <summary>Span colour.</summary>
            public int Color { get; }

            /// <summary>Span alpha.</summary>
            public int Alpha { get; }

            /// <summary>Span flags.</summary>
            public int Flags { get; }

            /// <summary>Span bidi level.</summary>
            public int Bidi { get; }

            /// <summary>Character flags.</summary>
            public int CharFlags { get; }

            /// <summary>Ascender.</summary>
            public double Ascender { get; }

            /// <summary>Descender.</summary>
            public double Descender { get; }

            /// <summary>Span text.</summary>
            public string Text { get; }

            /// <summary>Span characters (raw dicts only).</summary>
            public List<RawChar> Chars { get; }

            /// <summary>Build a dict span.</summary>
            public DictSpan(Rect bbox, Point origin, string font, double size, int color,
                int alpha, int flags, int bidi, int charFlags, double ascender,
                double descender, string text, List<RawChar> chars)
            {
                Bbox = bbox;
                Origin = origin;
                Font = font;
                Size = size;
                Color = color;
                Alpha = alpha;
                Flags = flags;
                Bidi = bidi;
                CharFlags = charFlags;
                Ascender = ascender;
                Descender = descender;
                Text = text;
                Chars = chars;
            }
        }

        /// <summary>One raw character: box, text, origin, and synthetic flag.</summary>
        public sealed class RawChar
        {
            /// <summary>Character box.</summary>
            public Rect Bbox { get; }

            /// <summary>Character text.</summary>
            public string Text { get; }

            /// <summary>Character origin.</summary>
            public Point Origin { get; }

            /// <summary>Whether synthetic.</summary>
            public bool Synthetic { get; }

            /// <summary>Build a raw character.</summary>
            public RawChar(Rect bbox, string text, Point origin, bool synthetic)
            {
                Bbox = bbox;
                Text = text;
                Origin = origin;
                Synthetic = synthetic;
            }
        }

        internal static TextOutput Read(IntPtr handle)
        {
            try
            {
                CompatSupport.Check(
                    NativeMethods.pdf_compat_text_output_format(handle, out int fmt), "Read");
                switch (fmt)
                {
                    case FormatWords:
                        return new TextOutput(FormatWords, null, ReadWords(handle),
                            new List<Block>(), null);
                    case FormatBlocks:
                        return new TextOutput(FormatBlocks, null, new List<Word>(),
                            ReadBlocks(handle), null);
                    case FormatDict:
                        return new TextOutput(FormatDict, null, new List<Word>(),
                            new List<Block>(), ReadDict(handle));
                    default:
                        return new TextOutput(fmt, ReadText(handle), new List<Word>(),
                            new List<Block>(), null);
                }
            }
            finally
            {
                NativeMethods.pdf_compat_text_output_free(handle);
            }
        }

        private static string ReadText(IntPtr handle)
        {
            CompatSupport.Check(
                NativeMethods.pdf_compat_text_output_text(handle, out IntPtr s), "Read");
            return CompatSupport.ReadString(s);
        }

        private static unsafe List<Word> ReadWords(IntPtr handle)
        {
            CompatSupport.Check(
                NativeMethods.pdf_compat_text_output_word_count(handle, out int n), "Read");
            var result = new List<Word>(n);
            for (int i = 0; i < n; i++)
            {
                Rect rect = default;
                CompatSupport.Check(NativeMethods.pdf_compat_text_output_word_rect(handle,
                    i, &rect), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_text_output_word_text(handle,
                    i, out IntPtr text), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_text_output_word_block(handle,
                    i, out int block), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_text_output_word_line(handle,
                    i, out int line), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_text_output_word_number(
                    handle, i, out int number), "Read");
                result.Add(new Word(rect, CompatSupport.ReadString(text), block, line,
                    number));
            }
            return result;
        }

        private static unsafe List<Block> ReadBlocks(IntPtr handle)
        {
            CompatSupport.Check(
                NativeMethods.pdf_compat_text_output_block_count(handle, out int n), "Read");
            var result = new List<Block>(n);
            for (int i = 0; i < n; i++)
            {
                Rect rect = default;
                CompatSupport.Check(NativeMethods.pdf_compat_text_output_block_rect(handle,
                    i, &rect), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_text_output_block_text(handle,
                    i, out IntPtr text), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_text_output_block_number(
                    handle, i, out int number), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_text_output_block_type(
                    handle, i, out int type), "Read");
                result.Add(new Block(rect, CompatSupport.ReadString(text), number, type));
            }
            return result;
        }

        private static Dict ReadDict(IntPtr handle)
        {
            CompatSupport.Check(
                NativeMethods.pdf_compat_text_output_dict_width(handle, out double w),
                "Read");
            CompatSupport.Check(
                NativeMethods.pdf_compat_text_output_dict_height(handle, out double h),
                "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_block_count(
                handle, out int n), "Read");
            var blocks = new List<DictBlock>(n);
            for (int i = 0; i < n; i++)
            {
                CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_block_tag(
                    handle, i, out int tag), "Read");
                if (tag == 0)
                    blocks.Add(ReadTextBlock(handle, i));
                else
                    blocks.Add(ReadImageBlock(handle, i));
            }
            return new Dict(w, h, blocks);
        }

        private static unsafe DictBlock ReadTextBlock(IntPtr handle, int i)
        {
            Rect bbox = default;
            CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_text_bbox(
                handle, i, &bbox), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_text_number(
                handle, i, out int number), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_text_flags(
                handle, i, out int flags), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_line_count(
                handle, i, out int lines), "Read");
            var result = new List<DictLine>(lines);
            for (int l = 0; l < lines; l++)
                result.Add(ReadLine(handle, i, l));
            return new DictBlock(0, bbox, number, flags, result, 0, 0);
        }

        private static unsafe DictBlock ReadImageBlock(IntPtr handle, int i)
        {
            Rect bbox = default;
            CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_image_bbox(
                handle, i, &bbox), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_image_number(
                handle, i, out int number), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_image_width(
                handle, i, out int width), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_image_height(
                handle, i, out int height), "Read");
            return new DictBlock(1, bbox, number, 0, new List<DictLine>(), width, height);
        }

        private static unsafe DictLine ReadLine(IntPtr handle, int i, int l)
        {
            Rect bbox = default;
            CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_line_bbox(
                handle, i, l, &bbox), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_line_wmode(
                handle, i, l, out int wmode), "Read");
            Point direction = default;
            CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_line_direction(
                handle, i, l, &direction), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_span_count(
                handle, i, l, out int spans), "Read");
            var result = new List<DictSpan>(spans);
            for (int s = 0; s < spans; s++)
                result.Add(ReadSpan(handle, i, l, s));
            return new DictLine(bbox, wmode, direction, result);
        }

        private static unsafe DictSpan ReadSpan(IntPtr handle, int i, int l, int s)
        {
            Rect bbox = default;
            CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_span_bbox(
                handle, i, l, s, &bbox), "Read");
            Point origin = default;
            CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_span_origin(
                handle, i, l, s, &origin), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_span_font(
                handle, i, l, s, out IntPtr font), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_span_size(
                handle, i, l, s, out double size), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_span_color(
                handle, i, l, s, out int color), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_span_alpha(
                handle, i, l, s, out int alpha), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_span_flags(
                handle, i, l, s, out int flags), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_span_bidi(
                handle, i, l, s, out int bidi), "Read");
            CompatSupport.Check(
                NativeMethods.pdf_compat_text_output_dict_span_char_flags(handle, i, l, s,
                    out int charFlags), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_span_ascender(
                handle, i, l, s, out double ascender), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_span_descender(
                handle, i, l, s, out double descender), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_span_text(
                handle, i, l, s, out IntPtr text), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_span_char_count(
                handle, i, l, s, out int chars), "Read");
            var result = new List<RawChar>(chars);
            for (int c = 0; c < chars; c++)
            {
                Rect charBox = default;
                CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_char_bbox(
                    handle, i, l, s, c, &charBox), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_char_text(
                    handle, i, l, s, c, out IntPtr charText), "Read");
                Point charOrigin = default;
                CompatSupport.Check(NativeMethods.pdf_compat_text_output_dict_char_origin(
                    handle, i, l, s, c, &charOrigin), "Read");
                CompatSupport.Check(
                    NativeMethods.pdf_compat_text_output_dict_char_synthetic(handle, i, l,
                        s, c, out int synthetic), "Read");
                result.Add(new RawChar(charBox, CompatSupport.ReadString(charText),
                    charOrigin, synthetic != 0));
            }
            return new DictSpan(bbox, origin, CompatSupport.ReadString(font), size, color,
                alpha, flags, bidi, charFlags, ascender, descender,
                CompatSupport.ReadString(text), result);
        }
    }
}
