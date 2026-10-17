// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;

namespace PDFluent
{
    /// <summary>Page render options.</summary>
    public sealed class RenderOptions
    {
        /// <summary>Render matrix.</summary>
        public Matrix? Matrix = new Matrix(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);

        /// <summary>Resolution, if any.</summary>
        public double? Dpi = null;

        /// <summary>Colour space, if any.</summary>
        public Colorspace? Colorspace = null;

        /// <summary>Clip box, if any.</summary>
        public Rect? Clip = null;

        /// <summary>Keep an alpha channel.</summary>
        public bool Alpha = false;

        /// <summary>Render annotations.</summary>
        public bool Annots = true;

    }
}
