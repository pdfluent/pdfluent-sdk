// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;

namespace PDFluent
{
    /// <summary>One vector drawing.</summary>
    public sealed class Drawing
    {
        /// <summary>Drawing box in points.</summary>
        public Rect Rect { get; }

        /// <summary>Sequence number.</summary>
        public int Seqno { get; }

        /// <summary>Drawing operator name.</summary>
        public string DrawingType { get; }

        /// <summary>Stroke colour, or empty.</summary>
        public double[] Color { get; }

        /// <summary>Fill colour, or empty.</summary>
        public double[] Fill { get; }

        /// <summary>Stroke width, if any.</summary>
        public double? Width { get; }

        /// <summary>Line-cap values, or empty.</summary>
        public int[] LineCap { get; }

        /// <summary>Line-join value, if any.</summary>
        public int? LineJoin { get; }

        /// <summary>Dash pattern, if any.</summary>
        public string? Dashes { get; }

        /// <summary>Close-path flag, if any.</summary>
        public bool? ClosePath { get; }

        /// <summary>Even-odd fill rule.</summary>
        public bool EvenOdd { get; }

        /// <summary>Fill opacity, if any.</summary>
        public double? FillOpacity { get; }

        /// <summary>Stroke opacity, if any.</summary>
        public double? StrokeOpacity { get; }

        /// <summary>Layer name.</summary>
        public string Layer { get; }

        /// <summary>Nesting level, if any.</summary>
        public int? Level { get; }

        /// <summary>Path items.</summary>
        public List<DrawingItem> Items { get; }


        /// <summary>One path item: an operator and its parameters as JSON.</summary>
        public sealed class DrawingItem
        {
            /// <summary>Path operator.</summary>
            public string Op { get; }

            /// <summary>Parameters as JSON.</summary>
            public string Params { get; }

            /// <summary>Build a path item.</summary>
            public DrawingItem(string op, string @params)
            {
                Op = op;
                Params = @params;
            }
        }

        /// <summary>Build a drawing.</summary>
        public Drawing(Rect rect, int seqno, string drawingType, double[] color, double[] fill, double? width, int[] lineCap, int? lineJoin, string? dashes, bool? closePath, bool evenOdd, double? fillOpacity, double? strokeOpacity, string layer, int? level, List<DrawingItem> items)
        {
            Rect = rect;
            Seqno = seqno;
            DrawingType = drawingType;
            Color = color;
            Fill = fill;
            Width = width;
            LineCap = lineCap;
            LineJoin = lineJoin;
            Dashes = dashes;
            ClosePath = closePath;
            EvenOdd = evenOdd;
            FillOpacity = fillOpacity;
            StrokeOpacity = strokeOpacity;
            Layer = layer;
            Level = level;
            Items = items;
        }

        internal static unsafe List<Drawing> ReadList(IntPtr list)
        {
            CompatSupport.Check(
                NativeMethods.pdf_compat_drawing_list_count(list, out int n), "Read");
            var result = new List<Drawing>(n);
            for (int i = 0; i < n; i++)
            {
                Rect vRect = default;
                CompatSupport.Check(NativeMethods.pdf_compat_drawing_list_rect(list, i, &vRect), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_drawing_list_seqno(list, i, out int vSeqno), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_drawing_list_drawing_type(list, i, out IntPtr vDrawingType), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_drawing_list_color(list, i, out int vColorHas, out IntPtr vColor, out UIntPtr vColorLen), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_drawing_list_fill(list, i, out int vFillHas, out IntPtr vFill, out UIntPtr vFillLen), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_drawing_list_width(list, i, out int vWidthHas, out double vWidth), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_drawing_list_line_cap(list, i, out int vLineCapHas, out IntPtr vLineCap, out UIntPtr vLineCapLen), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_drawing_list_line_join(list, i, out int vLineJoinHas, out int vLineJoin), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_drawing_list_dashes(list, i, out int vDashesHas, out IntPtr vDashes), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_drawing_list_close_path(list, i, out int vClosePathHas, out int vClosePath), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_drawing_list_even_odd(list, i, out int vEvenOdd), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_drawing_list_fill_opacity(list, i, out int vFillOpacityHas, out double vFillOpacity), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_drawing_list_stroke_opacity(list, i, out int vStrokeOpacityHas, out double vStrokeOpacity), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_drawing_list_layer(list, i, out IntPtr vLayer), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_drawing_list_level(list, i, out int vLevelHas, out int vLevel), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_drawing_list_item_count(list, i, out int vItemCount), "Read");
                var fItems = new List<DrawingItem>(vItemCount);
                for (int j = 0; j < vItemCount; j++)
                {
                    CompatSupport.Check(NativeMethods.pdf_compat_drawing_list_item_op(list, i, j, out IntPtr vOp), "Read");
                    CompatSupport.Check(NativeMethods.pdf_compat_drawing_list_item_params(list, i, j, out IntPtr vParams), "Read");
                    fItems.Add(new DrawingItem(CompatSupport.ReadString(vOp), CompatSupport.ReadString(vParams)));
                }
                result.Add(new Drawing(vRect, vSeqno, CompatSupport.ReadString(vDrawingType), (vColorHas != 0 ? CompatSupport.ReadDoubles(vColor, vColorLen) : Array.Empty<double>()), (vFillHas != 0 ? CompatSupport.ReadDoubles(vFill, vFillLen) : Array.Empty<double>()), (vWidthHas != 0 ? vWidth : (double?)null), (vLineCapHas != 0 ? CompatSupport.ReadInts(vLineCap, vLineCapLen) : Array.Empty<int>()), (vLineJoinHas != 0 ? vLineJoin : (int?)null), (vDashesHas != 0 ? CompatSupport.ReadString(vDashes) : null), (vClosePathHas != 0 ? (vClosePath != 0) : (bool?)null), vEvenOdd != 0, (vFillOpacityHas != 0 ? vFillOpacity : (double?)null), (vStrokeOpacityHas != 0 ? vStrokeOpacity : (double?)null), CompatSupport.ReadString(vLayer), (vLevelHas != 0 ? vLevel : (int?)null), fItems));
            }
            return result;
        }
    }
}
