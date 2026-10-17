// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;

namespace PDFluent
{
    /// <summary>
    /// Compatible error: the named input does not exist.
    /// </summary>
    public sealed class FileNotFoundError : PdfluentException
    {
        /// <inheritdoc cref="PdfluentException(string)"/>
        public FileNotFoundError(string message) : base(message) { }

        /// <inheritdoc cref="PdfluentException(string, Exception)"/>
        public FileNotFoundError(string message, Exception inner) : base(message, inner) { }
    }
}
