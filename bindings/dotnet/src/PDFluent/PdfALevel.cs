// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

namespace PDFluent
{
    /// <summary>
    /// PDF/A conformance levels accepted by
    /// <see cref="PdfDocument.ValidatePdfA(PdfALevel)"/>.
    /// </summary>
    /// <remarks>
    /// Integer values mirror the C ABI <c>PdfALevel</c> enum — do not renumber.
    /// </remarks>
    public enum PdfALevel
    {
        /// <summary>PDF/A-1B — ISO 19005-1 basic (most common archival target).</summary>
        A1b = 0,

        /// <summary>PDF/A-1A — ISO 19005-1 accessible (requires tagged PDF).</summary>
        A1a = 1,

        /// <summary>PDF/A-2B — ISO 19005-2 basic.</summary>
        A2b = 2,

        /// <summary>PDF/A-2U — ISO 19005-2 with Unicode mapping.</summary>
        A2u = 3,

        /// <summary>PDF/A-2A — ISO 19005-2 accessible.</summary>
        A2a = 4,

        /// <summary>PDF/A-3B — ISO 19005-3 basic (allows embedded files).</summary>
        A3b = 5,

        /// <summary>PDF/A-3U — ISO 19005-3 with Unicode mapping.</summary>
        A3u = 6,

        /// <summary>PDF/A-3A — ISO 19005-3 accessible.</summary>
        A3a = 7,

        /// <summary>PDF/A-4 — ISO 19005-4 base level.</summary>
        A4 = 8,

        /// <summary>PDF/A-4F — allows file attachments.</summary>
        A4f = 9,

        /// <summary>PDF/A-4E — allows engineering content (3D, rich media).</summary>
        A4e = 10,
    }
}
