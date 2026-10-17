// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Globalization;
using System.Text;

namespace PDFluent
{
    /// <summary>Shape finish options.</summary>
    ///
    /// <remarks>Mutable DTO with recorded defaults; serialises to the JSON
    /// the native layer overlays onto its own defaults.</remarks>
    public sealed class ShapeFinishOptions
    {
        /// <summary>Stroke width in points.</summary>
        public double Width = 1.0;

        /// <summary>Stroke colour.</summary>
        public double[]? Color = new double[] { 0.0 };

        /// <summary>Fill colour, if any.</summary>
        public double[]? Fill = null;

        /// <summary>Line-cap style.</summary>
        public int LineCap = 0;

        /// <summary>Line-join style.</summary>
        public int LineJoin = 0;

        /// <summary>Dash pattern, if any.</summary>
        public string? Dashes = null;

        /// <summary>Even-odd fill rule.</summary>
        public bool EvenOdd = false;

        /// <summary>Morph matrix, if any.</summary>
        public double[]? Morph = null;

        /// <summary>Close the path.</summary>
        public bool ClosePath = true;

        /// <summary>Fill opacity.</summary>
        public double FillOpacity = 1.0;

        /// <summary>Stroke opacity.</summary>
        public double StrokeOpacity = 1.0;

        /// <summary>Optional-content id.</summary>
        public int Oc = 0;

        /// <summary>Serialise to the native JSON overlay.</summary>
        public string ToJson()
        {
            var out_ = new StringBuilder("{");
            bool first = true;
            first = Append(out_, first, "width", Width.ToString(CultureInfo.InvariantCulture));
            first = Append(out_, first, "color", (Color == null ? null : CompatSupport.JsonDoubles(Color)));
            first = Append(out_, first, "fill", (Fill == null ? null : CompatSupport.JsonDoubles(Fill)));
            first = Append(out_, first, "line_cap", LineCap.ToString(CultureInfo.InvariantCulture));
            first = Append(out_, first, "line_join", LineJoin.ToString(CultureInfo.InvariantCulture));
            first = Append(out_, first, "dashes", (Dashes == null ? null : CompatSupport.JsonQuote(Dashes)));
            first = Append(out_, first, "even_odd", (EvenOdd ? "true" : "false"));
            first = Append(out_, first, "morph", (Morph == null ? null : CompatSupport.JsonDoubles(Morph)));
            first = Append(out_, first, "close_path", (ClosePath ? "true" : "false"));
            first = Append(out_, first, "fill_opacity", FillOpacity.ToString(CultureInfo.InvariantCulture));
            first = Append(out_, first, "stroke_opacity", StrokeOpacity.ToString(CultureInfo.InvariantCulture));
            first = Append(out_, first, "oc", Oc.ToString(CultureInfo.InvariantCulture));
            return out_.Append("}").ToString();
        }

        private static bool Append(StringBuilder out_, bool first, string key, string? value)
        {
            if (value == null)
                return first;
            if (!first)
                out_.Append(",");
            out_.Append(CompatSupport.JsonQuote(key)).Append(":").Append(value);
            return false;
        }
    }
}
