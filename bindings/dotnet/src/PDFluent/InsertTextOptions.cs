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
    /// <summary>Text insertion options.</summary>
    ///
    /// <remarks>Mutable DTO with recorded defaults; serialises to the JSON
    /// the native layer overlays onto its own defaults.</remarks>
    public sealed class InsertTextOptions
    {
        /// <summary>Font size in points.</summary>
        public double Fontsize = 11.0;

        /// <summary>Line height, if any.</summary>
        public double? Lineheight = null;

        /// <summary>Base font name.</summary>
        public string? Fontname = "helv";

        /// <summary>Font file, if any.</summary>
        public string? Fontfile = null;

        /// <summary>Use simple positioning.</summary>
        public bool SetSimple = false;

        /// <summary>Text encoding.</summary>
        public int Encoding = 0;

        /// <summary>Text colour, if any.</summary>
        public double[]? Color = null;

        /// <summary>Fill colour, if any.</summary>
        public double[]? Fill = null;

        /// <summary>Border width.</summary>
        public double BorderWidth = 0.05;

        /// <summary>Miter limit.</summary>
        public double MiterLimit = 1.0;

        /// <summary>Text rendering mode.</summary>
        public int RenderMode = 0;

        /// <summary>Rotation in degrees.</summary>
        public int Rotate = 0;

        /// <summary>Morph matrix, if any.</summary>
        public double[]? Morph = null;

        /// <summary>Draw over content.</summary>
        public bool Overlay = true;

        /// <summary>Stroke opacity.</summary>
        public double StrokeOpacity = 1.0;

        /// <summary>Fill opacity.</summary>
        public double FillOpacity = 1.0;

        /// <summary>Optional-content id.</summary>
        public int Oc = 0;

        /// <summary>Serialise to the native JSON overlay.</summary>
        public string ToJson()
        {
            var out_ = new StringBuilder("{");
            bool first = true;
            first = Append(out_, first, "fontsize", Fontsize.ToString(CultureInfo.InvariantCulture));
            first = Append(out_, first, "lineheight", (Lineheight.HasValue ? Lineheight.Value.ToString(CultureInfo.InvariantCulture) : null));
            first = Append(out_, first, "fontname", (Fontname == null ? null : CompatSupport.JsonQuote(Fontname)));
            first = Append(out_, first, "fontfile", (Fontfile == null ? null : CompatSupport.JsonQuote(Fontfile)));
            first = Append(out_, first, "set_simple", (SetSimple ? "true" : "false"));
            first = Append(out_, first, "encoding", Encoding.ToString(CultureInfo.InvariantCulture));
            first = Append(out_, first, "color", (Color == null ? null : CompatSupport.JsonDoubles(Color)));
            first = Append(out_, first, "fill", (Fill == null ? null : CompatSupport.JsonDoubles(Fill)));
            first = Append(out_, first, "border_width", BorderWidth.ToString(CultureInfo.InvariantCulture));
            first = Append(out_, first, "miter_limit", MiterLimit.ToString(CultureInfo.InvariantCulture));
            first = Append(out_, first, "render_mode", RenderMode.ToString(CultureInfo.InvariantCulture));
            first = Append(out_, first, "rotate", Rotate.ToString(CultureInfo.InvariantCulture));
            first = Append(out_, first, "morph", (Morph == null ? null : CompatSupport.JsonDoubles(Morph)));
            first = Append(out_, first, "overlay", (Overlay ? "true" : "false"));
            first = Append(out_, first, "stroke_opacity", StrokeOpacity.ToString(CultureInfo.InvariantCulture));
            first = Append(out_, first, "fill_opacity", FillOpacity.ToString(CultureInfo.InvariantCulture));
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
