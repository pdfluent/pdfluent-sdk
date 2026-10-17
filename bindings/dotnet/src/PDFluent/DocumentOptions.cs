// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;

namespace PDFluent
{
    /// <summary>Document open options.</summary>
    public sealed class DocumentOptions
    {
        /// <summary>File path, if any.</summary>
        public string? Filename = null;

        /// <summary>File bytes, if any.</summary>
        public byte[]? Data = null;

        /// <summary>File-type hint, if any.</summary>
        public string? Filetype = null;

        /// <summary>Page box for a fresh shell, if any.</summary>
        public Rect? Rect = null;

        /// <summary>Page width for a fresh shell.</summary>
        public double Width = 0.0;

        /// <summary>Page height for a fresh shell.</summary>
        public double Height = 0.0;

        /// <summary>Default font size.</summary>
        public double Fontsize = 11.0;

        /// <summary>Archive value as JSON, if any.</summary>
        public string? ArchiveJson = null;

    }
}
