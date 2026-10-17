// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;

namespace PDFluent
{
    /// <summary>Font build options.</summary>
    public sealed class FontOptions
    {
        /// <summary>Font name, if any.</summary>
        public string? Fontname = null;

        /// <summary>Font file, if any.</summary>
        public string? Fontfile = null;

        /// <summary>Font bytes, if any.</summary>
        public byte[]? Fontbuffer = null;

        /// <summary>Script code.</summary>
        public int Script = 0;

        /// <summary>Language tag, if any.</summary>
        public string? Language = null;

        /// <summary>Character collection.</summary>
        public int Ordering = -1;

        /// <summary>Bold face.</summary>
        public bool IsBold = false;

        /// <summary>Italic face.</summary>
        public bool IsItalic = false;

        /// <summary>Serif face.</summary>
        public bool IsSerif = false;

        /// <summary>Embed the font.</summary>
        public bool Embed = true;

    }
}
