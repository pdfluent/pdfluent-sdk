// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;

namespace PDFluent
{
    /// <summary>Font registration options.</summary>
    public sealed class InsertFontOptions
    {
        /// <summary>Font name.</summary>
        public string? Fontname = "helv";

        /// <summary>Font file, if any.</summary>
        public string? Fontfile = null;

        /// <summary>Font bytes, if any.</summary>
        public byte[]? Fontbuffer = null;

        /// <summary>Use simple positioning.</summary>
        public bool SetSimple = false;

        /// <summary>Writing mode.</summary>
        public int Wmode = 0;

        /// <summary>Text encoding.</summary>
        public int Encoding = 0;

    }
}
