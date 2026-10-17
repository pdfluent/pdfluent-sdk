// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;

namespace PDFluent
{
    /// <summary>Page stamp options.</summary>
    public sealed class ShowPageOptions
    {
        /// <summary>Source page number.</summary>
        public int PageNo = 0;

        /// <summary>Keep the aspect ratio.</summary>
        public bool KeepProportion = true;

        /// <summary>Draw over content.</summary>
        public bool Overlay = true;

        /// <summary>Optional-content id.</summary>
        public int Oc = 0;

        /// <summary>Rotation in degrees.</summary>
        public int Rotate = 0;

        /// <summary>Clip box, if any.</summary>
        public Rect? Clip = null;

    }
}
