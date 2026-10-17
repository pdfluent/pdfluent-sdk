// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;

namespace PDFluent
{
    /// <summary>One typeset span.</summary>
    public sealed class TextTrace
    {
        /// <summary>Span box in points.</summary>
        public Rect Bbox { get; }

        /// <summary>Ascender height.</summary>
        public double Ascender { get; }

        /// <summary>Writing direction.</summary>
        public int BidiDir { get; }

        /// <summary>Bidi level.</summary>
        public int BidiLevel { get; }

        /// <summary>Characters.</summary>
        public List<TraceChar> Chars { get; }


        /// <summary>One traced character.</summary>
        public sealed class TraceChar
        {
            /// <summary>Character codepoint.</summary>
            public int Codepoint { get; }

            /// <summary>Glyph id.</summary>
            public int Glyph { get; }

            /// <summary>Glyph origin.</summary>
            public Point Origin { get; }

            /// <summary>Glyph box.</summary>
            public Rect Bbox { get; }

            /// <summary>Build a traced character.</summary>
            public TraceChar(int codepoint, int glyph, Point origin, Rect bbox)
            {
                Codepoint = codepoint;
                Glyph = glyph;
                Origin = origin;
                Bbox = bbox;
            }
        }

        /// <summary>Build a textTrace.</summary>
        public TextTrace(Rect bbox, double ascender, int bidiDir, int bidiLevel, List<TraceChar> chars)
        {
            Bbox = bbox;
            Ascender = ascender;
            BidiDir = bidiDir;
            BidiLevel = bidiLevel;
            Chars = chars;
        }

        internal static unsafe List<TextTrace> ReadList(IntPtr list)
        {
            CompatSupport.Check(
                NativeMethods.pdf_compat_text_trace_list_count(list, out int n), "Read");
            var result = new List<TextTrace>(n);
            for (int i = 0; i < n; i++)
            {
                Rect vBbox = default;
                CompatSupport.Check(NativeMethods.pdf_compat_text_trace_list_bbox(list, i, &vBbox), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_text_trace_list_ascender(list, i, out double vAscender), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_text_trace_list_bidi_dir(list, i, out int vBidiDir), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_text_trace_list_bidi_level(list, i, out int vBidiLevel), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_text_trace_list_char_count(list, i, out int vCharCount), "Read");
                var fChars = new List<TraceChar>(vCharCount);
                for (int j = 0; j < vCharCount; j++)
                {
                    CompatSupport.Check(NativeMethods.pdf_compat_text_trace_list_char_codepoint(list, i, j, out int vCp), "Read");
                    CompatSupport.Check(NativeMethods.pdf_compat_text_trace_list_char_glyph(list, i, j, out int vGl), "Read");
                    Point vOr = default;
                    CompatSupport.Check(NativeMethods.pdf_compat_text_trace_list_char_origin(list, i, j, &vOr), "Read");
                    Rect vBx = default;
                    CompatSupport.Check(NativeMethods.pdf_compat_text_trace_list_char_bbox(list, i, j, &vBx), "Read");
                    fChars.Add(new TraceChar(vCp, vGl, vOr, vBx));
                }
                result.Add(new TextTrace(vBbox, vAscender, vBidiDir, vBidiLevel, fChars));
            }
            return result;
        }
    }
}
