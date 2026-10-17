// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;

namespace PDFluent
{
    /// <summary>PDF import options.</summary>
    public sealed class InsertPdfOptions
    {
        /// <summary>First source page.</summary>
        public int FromPage = -1;

        /// <summary>Last source page.</summary>
        public int ToPage = -1;

        /// <summary>Insertion point.</summary>
        public int StartAt = -1;

        /// <summary>Rotation.</summary>
        public int Rotate = -1;

        /// <summary>Copy links.</summary>
        public bool Links = true;

        /// <summary>Copy annotations.</summary>
        public bool Annots = true;

        /// <summary>Copy widgets.</summary>
        public bool Widgets = true;

        /// <summary>Join duplicate resources.</summary>
        public bool JoinDuplicates = false;

        /// <summary>Report progress.</summary>
        public bool ShowProgress = false;

        /// <summary>Finish the import.</summary>
        public bool Finish = true;

    }
}
