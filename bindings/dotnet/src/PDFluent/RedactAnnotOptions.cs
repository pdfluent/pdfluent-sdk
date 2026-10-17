// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;

namespace PDFluent
{
    /// <summary>Redaction options.</summary>
    public sealed class RedactAnnotOptions
    {
        /// <summary>Replacement text, if any.</summary>
        public string? Text = null;

        /// <summary>Font name, if any.</summary>
        public string? Fontname = null;

        /// <summary>Font size.</summary>
        public double Fontsize = 11.0;

        /// <summary>Text alignment.</summary>
        public int Align = 0;

        /// <summary>Fill colour, if any.</summary>
        public double[]? Fill = null;

        /// <summary>Text colour, if any.</summary>
        public double[]? TextColor = null;

        /// <summary>Cross out instead of covering.</summary>
        public bool CrossOut = true;

    }
}
