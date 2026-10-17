// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;

namespace PDFluent
{
    /// <summary>Image placement options.</summary>
    public sealed class ImageInsertOptions
    {
        /// <summary>Alpha selector.</summary>
        public int Alpha = -1;

        /// <summary>Image file, if any.</summary>
        public string? Filename = null;

        /// <summary>Display height.</summary>
        public double Height = 0.0;

        /// <summary>Keep the aspect ratio.</summary>
        public bool KeepProportion = true;

        /// <summary>Mask value as JSON, if any.</summary>
        public string? MaskJson = null;

        /// <summary>Optional-content id.</summary>
        public int Oc = 0;

        /// <summary>Draw over content.</summary>
        public bool Overlay = true;

        /// <summary>Pixel source, if any.</summary>
        public Pixmap? Pixmap = null;

        /// <summary>Rotation in degrees.</summary>
        public int Rotate = 0;

        /// <summary>Image bytes, if any.</summary>
        public byte[]? Stream = null;

        /// <summary>Display width.</summary>
        public double Width = 0.0;

        /// <summary>Image object number.</summary>
        public int Xref = 0;

    }
}
