// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System.Collections.Generic;

namespace PDFluent
{
    /// <summary>
    /// The outcome of a PDF/A validation run, as returned by
    /// <see cref="PdfDocument.ValidatePdfA(PdfALevel)"/>.
    /// </summary>
    public sealed class PdfAReport
    {
        /// <summary>
        /// Whether the document fully conforms to the requested level.
        /// </summary>
        public bool IsCompliant { get; }

        /// <summary>
        /// Number of conformance errors found. Zero when
        /// <see cref="IsCompliant"/> is <see langword="true"/>.
        /// </summary>
        public int ErrorCount { get; }

        /// <summary>
        /// Number of warnings found. Warnings do not affect
        /// <see cref="IsCompliant"/>.
        /// </summary>
        public int WarningCount { get; }

        /// <summary>
        /// One-line issue summaries (<c>"[severity] rule: message"</c>),
        /// in report order. Never <see langword="null"/>.
        /// </summary>
        public IReadOnlyList<string> Issues { get; }

        internal PdfAReport(
            bool isCompliant,
            int errorCount,
            int warningCount,
            IReadOnlyList<string> issues)
        {
            IsCompliant = isCompliant;
            ErrorCount = errorCount;
            WarningCount = warningCount;
            Issues = issues;
        }
    }
}
