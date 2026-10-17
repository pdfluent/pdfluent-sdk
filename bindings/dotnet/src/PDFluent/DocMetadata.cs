// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;

namespace PDFluent
{
    /// <summary>Document metadata.</summary>
    public sealed class DocMetadata
    {
        /// <summary>Document title.</summary>
        public string? Title = null;

        /// <summary>Document author.</summary>
        public string? Author = null;

        /// <summary>Document subject.</summary>
        public string? Subject = null;

        /// <summary>Keywords.</summary>
        public string? Keywords = null;

        /// <summary>Creating application.</summary>
        public string? Creator = null;

        /// <summary>Producing application.</summary>
        public string? Producer = null;

        /// <summary>Creation date.</summary>
        public string? CreationDate = null;

        /// <summary>Modification date.</summary>
        public string? ModDate = null;

        /// <summary>File format.</summary>
        public string? Format = null;

        /// <summary>Encryption description.</summary>
        public string? Encryption = null;

        /// <summary>Trapped flag.</summary>
        public string? Trapped = null;

    }
}
