// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;

namespace PDFluent
{
    /// <summary>Font measurement options.</summary>
    public sealed class FontMeasureOptions
    {
        /// <summary>Font size in points.</summary>
        public double Fontsize = 11.0;

        /// <summary>Language tag, if any.</summary>
        public string? Language = null;

        /// <summary>Script code.</summary>
        public int Script = 0;

        /// <summary>Writing mode.</summary>
        public int Wmode = 0;

        /// <summary>Small capitals.</summary>
        public bool SmallCaps = false;

        /// <summary>Fallback selector.</summary>
        public int Fallback = 0;

    }
}
