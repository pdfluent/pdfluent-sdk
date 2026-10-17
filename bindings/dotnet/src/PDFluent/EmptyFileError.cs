// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;

namespace PDFluent
{
    /// <summary>
    /// Compatible error: the input has no bytes.
    /// </summary>
    public sealed class EmptyFileError : PdfluentException
    {
        /// <inheritdoc cref="PdfluentException(string)"/>
        public EmptyFileError(string message) : base(message) { }

        /// <inheritdoc cref="PdfluentException(string, Exception)"/>
        public EmptyFileError(string message, Exception inner) : base(message, inner) { }
    }
}
