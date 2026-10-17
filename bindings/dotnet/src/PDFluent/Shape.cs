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
    /// A compatible drawing canvas over one page.
    /// </summary>
    public sealed class Shape
    {
        private readonly Document _doc;
        private readonly int _page;

        /// <summary>
        /// Address a shape canvas.
        /// </summary>
        /// <param name="document">Owning document.</param>
        /// <param name="page">0-based page number.</param>
        public Shape(Document document, int page)
        {
            _doc = document;
            _page = page;
        }

        /// <summary>Commit pending marks.</summary>
        public void Commit(bool overlay)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_shape_commit(_doc.RequireOpen(), _page, CompatSupport.Flag(overlay)), "Commit");
        }

        /// <summary>Convenience overload with defaults.</summary>
        public void Commit()
        {
            Commit(true);
        }

        /// <summary>Queue a line.</summary>
        public unsafe Point DrawLine(Point p1, Point p2)
        {
            Point p1Tmp = p1;
            Point p2Tmp = p2;
            Point outValue = default;
            CompatSupport.Check(NativeMethods.pdf_compat_shape_draw_line(_doc.RequireOpen(), _page, &p1Tmp, &p2Tmp, &outValue), "DrawLine");
            return outValue;
        }

        /// <summary>Queue a rectangle.</summary>
        public unsafe Point DrawRect(Rect rect, double? radius)
        {
            Rect rectTmp = rect;
            Point outValue = default;
            CompatSupport.Check(NativeMethods.pdf_compat_shape_draw_rect(_doc.RequireOpen(), _page, &rectTmp, radius ?? double.NaN, &outValue), "DrawRect");
            return outValue;
        }

        /// <summary>Convenience overload with defaults.</summary>
        public unsafe Point DrawRect(Rect rect)
        {
            return DrawRect(rect, null);
        }

        /// <summary>Stroke and fill the queued path.</summary>
        public void Finish(ShapeFinishOptions? options)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_shape_finish(_doc.RequireOpen(), _page, options == null ? null : options.ToJson()), "Finish");
        }

        /// <summary>Convenience overload with defaults.</summary>
        public void Finish()
        {
            Finish(null);
        }
    }
}
