// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;

namespace PDFluent
{
    /// <summary>Text flush options.</summary>
    public sealed class FlushOptions
    {
        /// <summary>Text colour, if any.</summary>
        public double[]? Color = null;

        /// <summary>Opacity.</summary>
        public double Opacity = -1.0;

        /// <summary>Draw over content.</summary>
        public bool Overlay = true;

        /// <summary>Morph matrix, if any.</summary>
        public double[]? Morph = null;

        /// <summary>Placement matrix, if any.</summary>
        public Matrix? Matrix = null;

        /// <summary>Text rendering mode.</summary>
        public int RenderMode = 0;

        /// <summary>Optional-content id.</summary>
        public int Oc = 0;

    }
}
