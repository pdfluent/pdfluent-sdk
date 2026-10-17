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
    /// Text-search hits: rectangles, or quadrilaterals when requested.
    /// </summary>
    public sealed class SearchHits
    {
        /// <summary>Whether hits are quadrilaterals rather than rectangles.</summary>
        public bool IsQuads { get; }

        /// <summary>Hit rectangles; empty when hits are quadrilaterals.</summary>
        public List<Rect> Rects { get; }

        /// <summary>Hit quadrilaterals; empty when hits are rectangles.</summary>
        public List<Quad> Quads { get; }

        internal SearchHits(bool quads, List<Rect> rects, List<Quad> quadList)
        {
            IsQuads = quads;
            Rects = rects;
            Quads = quadList;
        }

        /// <summary>Hit count.</summary>
        public int Count => IsQuads ? Quads.Count : Rects.Count;

        internal static unsafe SearchHits Read(IntPtr handle)
        {
            try
            {
                CompatSupport.Check(
                    NativeMethods.pdf_compat_search_hits_is_quads(handle, out int q), "Read");
                CompatSupport.Check(
                    NativeMethods.pdf_compat_search_hits_count(handle, out int n), "Read");
                var rects = new List<Rect>();
                var quadList = new List<Quad>();
                if (q != 0)
                {
                    for (int i = 0; i < n; i++)
                    {
                        Quad quad = default;
                        CompatSupport.Check(NativeMethods.pdf_compat_search_hits_quad_at(
                            handle, i, &quad), "Read");
                        quadList.Add(quad);
                    }
                    return new SearchHits(true, rects, quadList);
                }
                for (int i = 0; i < n; i++)
                {
                    Rect rect = default;
                    CompatSupport.Check(NativeMethods.pdf_compat_search_hits_rect_at(
                        handle, i, &rect), "Read");
                    rects.Add(rect);
                }
                return new SearchHits(false, rects, quadList);
            }
            finally
            {
                NativeMethods.pdf_compat_search_hits_free(handle);
            }
        }
    }
}
