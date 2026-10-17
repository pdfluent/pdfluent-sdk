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
    /// <summary>Textbox layout options.</summary>
    ///
    /// <remarks>Mutable DTO with recorded defaults; serialises to the JSON
    /// the native layer overlays onto its own defaults.</remarks>
    public sealed class TextBoxOptions
    {
        /// <summary>Text alignment.</summary>
        public int Align = 0;

        /// <summary>Tab expansion width.</summary>
        public int Expandtabs = 1;

        /// <summary>Serialise to the native JSON overlay.</summary>
        public string ToJson()
        {
            var out_ = new StringBuilder("{");
            bool first = true;
            first = Append(out_, first, "align", Align.ToString(CultureInfo.InvariantCulture));
            first = Append(out_, first, "expandtabs", Expandtabs.ToString(CultureInfo.InvariantCulture));
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
