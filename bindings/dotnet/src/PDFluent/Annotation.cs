// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

namespace PDFluent
{
    /// <summary>
    /// One annotation on a page, as returned by
    /// <see cref="PdfDocument.GetAnnotations(int)"/>.
    /// </summary>
    public sealed class Annotation
    {
        /// <summary>Annotation subtype name (e.g. <c>"Highlight"</c>).</summary>
        public string AnnotType { get; }

        /// <summary>Bounding box in PDF user-space points.</summary>
        public Rect Rect { get; }

        internal Annotation(string annotType, Rect rect)
        {
            AnnotType = annotType;
            Rect = rect;
        }
    }
}
