// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;

namespace PDFluent
{
    /// <summary>Text append options.</summary>
    public sealed class AppendOptions
    {
        /// <summary>Font, if any.</summary>
        public Font? Font = null;

        /// <summary>Font size in points.</summary>
        public double Fontsize = 11.0;

        /// <summary>Language tag, if any.</summary>
        public string? Language = null;

        /// <summary>Right-to-left text.</summary>
        public bool RightToLeft = false;

        /// <summary>Small capitals.</summary>
        public bool SmallCaps = false;

    }
}
