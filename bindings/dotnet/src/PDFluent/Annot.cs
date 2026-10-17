// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;

namespace PDFluent
{

    /// <summary>
    /// A compatible annotation, addressed by document, page, and xref.
    /// </summary>
    public sealed class Annot
    {
        private readonly Document _doc;
        private readonly int _page;
        private readonly int _xref;

        /// <summary>
        /// Address an annotation.
        /// </summary>
        /// <param name="document">Owning document.</param>
        /// <param name="page">0-based page number.</param>
        /// <param name="xref">Annotation xref.</param>
        public Annot(Document document, int page, int xref)
        {
            _doc = document;
            _page = page;
            _xref = xref;
        }

        /// <summary>
        /// Push appearance changes.
        /// </summary>
        /// <param name="options">Appearance options.</param>
        public unsafe void Update(AnnotUpdateOptions options)
        {
            fixed (double* textPtr = options.TextColor)
            fixed (double* borderPtr = options.BorderColor)
            fixed (double* fillPtr = options.FillColor)
            {
                CompatSupport.Check(NativeMethods.pdf_compat_annot_update(_doc.RequireOpen(),
                    _page, _xref, options.BlendMode, options.Opacity ?? double.NaN,
                    options.Fontsize, options.Fontname, textPtr,
                    CompatSupport.Count(options.TextColor), borderPtr,
                    CompatSupport.Count(options.BorderColor), fillPtr,
                    CompatSupport.Count(options.FillColor),
                    CompatSupport.Flag(options.CrossOut), options.Rotate), "Update");
            }
        }


        /// <summary>Set stroke and fill colours.</summary>
        public void SetColors(double[]? stroke, double[]? fill)
        {
            unsafe
            {
                fixed (double* strokePtr = stroke)
                {
                    fixed (double* fillPtr = fill)
                    {
                    CompatSupport.Check(NativeMethods.pdf_compat_annot_set_colors(_doc.RequireOpen(), _page, _xref, strokePtr, CompatSupport.Count(stroke), fillPtr, CompatSupport.Count(fill)), "SetColors");
                    }
                }
            }
        }

        /// <summary>Convenience overload with defaults.</summary>
        public unsafe void Update()
        {
            Update(new AnnotUpdateOptions());
        }
    }
}
