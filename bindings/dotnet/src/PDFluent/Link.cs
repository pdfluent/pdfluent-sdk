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
    /// A compatible document link: owned data shared with insert calls.
    /// </summary>
    public sealed class Link : IDisposable
    {
        private IntPtr _handle;

        /// <summary>
        /// Build a blank link.
        /// </summary>
        public Link()
        {
            CompatSupport.Check(NativeMethods.pdf_compat_link_new(out IntPtr hPtr), "Link");
            _handle = hPtr;
        }

        internal Link(IntPtr handle)
        {
            _handle = handle;
        }

        /// <summary>Link box in points.</summary>
        public unsafe Rect GetRect()
        {
            Rect r = default;
            CompatSupport.Check(NativeMethods.pdf_compat_link_rect(RequireOpen(), &r),
                "GetRect");
            return r;
        }

        /// <summary>Set the link box.</summary>
        public unsafe void SetRect(Rect value)
        {
            Rect v = value;
            CompatSupport.Check(NativeMethods.pdf_compat_link_set_rect(RequireOpen(), &v),
                "SetRect");
        }

        /// <summary>Link kind code.</summary>
        public int GetKind()
        {
            CompatSupport.Check(NativeMethods.pdf_compat_link_kind(RequireOpen(), out int v),
                "GetKind");
            return v;
        }

        /// <summary>Set the link kind code.</summary>
        public void SetKind(int value)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_link_set_kind(RequireOpen(), value),
                "SetKind");
        }

        /// <summary>Link target URI.</summary>
        public string GetUri()
        {
            CompatSupport.Check(NativeMethods.pdf_compat_link_uri(RequireOpen(), out IntPtr s),
                "GetUri");
            return CompatSupport.ReadString(s);
        }

        /// <summary>Set the link target URI.</summary>
        public void SetUri(string? value)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_link_set_uri(RequireOpen(), value),
                "SetUri");
        }

        /// <summary>Link object number.</summary>
        public int GetXref()
        {
            CompatSupport.Check(NativeMethods.pdf_compat_link_xref(RequireOpen(), out int v),
                "GetXref");
            return v;
        }

        /// <summary>Set the link object number.</summary>
        public void SetXref(int value)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_link_set_xref(RequireOpen(), value),
                "SetXref");
        }

        /// <summary>Link identifier.</summary>
        public string GetId()
        {
            CompatSupport.Check(NativeMethods.pdf_compat_link_id(RequireOpen(), out IntPtr s),
                "GetId");
            return CompatSupport.ReadString(s);
        }

        /// <summary>Set the link identifier.</summary>
        public void SetId(string? value)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_link_set_id(RequireOpen(), value),
                "SetId");
        }

        /// <summary>Link border width in points.</summary>
        public double GetBorderWidth()
        {
            CompatSupport.Check(NativeMethods.pdf_compat_link_border_width(RequireOpen(),
                out double v), "GetBorderWidth");
            return v;
        }

        /// <summary>Set the link border width.</summary>
        public void SetBorderWidth(double value)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_link_set_border_width(RequireOpen(),
                value), "SetBorderWidth");
        }

        /// <summary>Link border style, or null when there is none.</summary>
        public string? GetBorderStyle()
        {
            CompatSupport.Check(NativeMethods.pdf_compat_link_border_style(RequireOpen(),
                out int has, out IntPtr s), "GetBorderStyle");
            string style = CompatSupport.ReadString(s);
            return has != 0 ? style : null;
        }

        /// <summary>Set the link border style; null clears it.</summary>
        public void SetBorderStyle(string? value)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_link_set_border_style(RequireOpen(),
                value), "SetBorderStyle");
        }

        /// <summary>Link border dashes.</summary>
        public int[] GetBorderDashes()
        {
            CompatSupport.Check(NativeMethods.pdf_compat_link_border_dashes(RequireOpen(),
                out IntPtr data, out UIntPtr len), "GetBorderDashes");
            return CompatSupport.ReadInts(data, len);
        }

        /// <summary>Set the link border dashes.</summary>
        public unsafe void SetBorderDashes(int[]? values)
        {
            fixed (int* p = values)
            {
                CompatSupport.Check(NativeMethods.pdf_compat_link_set_border_dashes(
                    RequireOpen(), p, CompatSupport.Count(values)), "SetBorderDashes");
            }
        }

        /// <summary>Link border cloud effect level.</summary>
        public int GetBorderClouds()
        {
            CompatSupport.Check(NativeMethods.pdf_compat_link_border_clouds(RequireOpen(),
                out int v), "GetBorderClouds");
            return v;
        }

        /// <summary>Set the link border cloud effect level.</summary>
        public void SetBorderClouds(int value)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_link_set_border_clouds(RequireOpen(),
                value), "SetBorderClouds");
        }

        /// <summary>Link stroke colour.</summary>
        public double[] GetStroke()
        {
            CompatSupport.Check(NativeMethods.pdf_compat_link_stroke(RequireOpen(),
                out IntPtr data, out UIntPtr len), "GetStroke");
            return CompatSupport.ReadDoubles(data, len);
        }

        /// <summary>Set the link stroke colour.</summary>
        public unsafe void SetStroke(double[]? values)
        {
            fixed (double* p = values)
            {
                CompatSupport.Check(NativeMethods.pdf_compat_link_set_stroke(RequireOpen(),
                    p, CompatSupport.Count(values)), "SetStroke");
            }
        }

        /// <summary>Link fill colour.</summary>
        public double[] GetFill()
        {
            CompatSupport.Check(NativeMethods.pdf_compat_link_fill(RequireOpen(),
                out IntPtr data, out UIntPtr len), "GetFill");
            return CompatSupport.ReadDoubles(data, len);
        }

        /// <summary>Set the link fill colour.</summary>
        public unsafe void SetFill(double[]? values)
        {
            fixed (double* p = values)
            {
                CompatSupport.Check(NativeMethods.pdf_compat_link_set_fill(RequireOpen(),
                    p, CompatSupport.Count(values)), "SetFill");
            }
        }

        /// <summary>Link flags.</summary>
        public int GetFlags()
        {
            CompatSupport.Check(NativeMethods.pdf_compat_link_flags(RequireOpen(), out int v),
                "GetFlags");
            return v;
        }

        /// <summary>Set the link flags.</summary>
        public void SetFlags(int value)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_link_set_flags(RequireOpen(), value),
                "SetFlags");
        }

        /// <summary>Whether the link carries a follow-on destination.</summary>
        public bool HasDest()
        {
            CompatSupport.Check(NativeMethods.pdf_compat_link_has_dest(RequireOpen(),
                out int v), "HasDest");
            return v != 0;
        }

        /// <summary>Follow-on destination page, or -1 when there is none.</summary>
        public int GetDestPage()
        {
            CompatSupport.Check(NativeMethods.pdf_compat_link_dest_page(RequireOpen(),
                out int v), "GetDestPage");
            return v;
        }

        /// <summary>Follow-on destination point, or the origin when there is none.</summary>
        public unsafe Point GetDestTarget()
        {
            Point p = default;
            CompatSupport.Check(NativeMethods.pdf_compat_link_dest_target(RequireOpen(), &p),
                "GetDestTarget");
            return p;
        }

        /// <summary>Follow-on destination zoom, or 0 when there is none.</summary>
        public double GetDestZoom()
        {
            CompatSupport.Check(NativeMethods.pdf_compat_link_dest_zoom(RequireOpen(),
                out double v), "GetDestZoom");
            return v;
        }

        /// <summary>Clear the follow-on destination.</summary>
        public void ClearDest()
        {
            CompatSupport.Check(NativeMethods.pdf_compat_link_clear_dest(RequireOpen()),
                "ClearDest");
        }

        /// <summary>Whether the link chains to another.</summary>
        public bool HasNext()
        {
            CompatSupport.Check(NativeMethods.pdf_compat_link_has_next(RequireOpen(),
                out int v), "HasNext");
            return v != 0;
        }

        /// <summary>
        /// The chained link, or null when there is none. The caller owns it.
        /// </summary>
        /// <returns>The next link, or null.</returns>
        public Link? GetNext()
        {
            if (!HasNext())
                return null;
            CompatSupport.Check(NativeMethods.pdf_compat_link_next(RequireOpen(),
                out IntPtr hPtr), "GetNext");
            return new Link(hPtr);
        }

        /// <summary>Chain another link; null clears the chain.</summary>
        public void SetNext(Link? next)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_link_set_next(RequireOpen(),
                next == null ? IntPtr.Zero : next.RequireOpen()), "SetNext");
        }

        internal static List<Link> ReadList(IntPtr list)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_link_list_count(list, out int n),
                "Read");
            var result = new List<Link>(n);
            for (int i = 0; i < n; i++)
            {
                CompatSupport.Check(NativeMethods.pdf_compat_link_list_at(list, i,
                    out IntPtr hPtr), "Read");
                result.Add(new Link(hPtr));
            }
            return result;
        }


        /// <summary>
        /// Close the handle. Idempotent.
        /// </summary>
        public void Dispose()
        {
            if (_handle != IntPtr.Zero)
            {
                NativeMethods.pdf_compat_link_free(_handle);
                _handle = IntPtr.Zero;
            }
            GC.SuppressFinalize(this);
        }

        /// <summary>Finalize the handle.</summary>
        ~Link()
        {
            if (_handle != IntPtr.Zero)
                NativeMethods.pdf_compat_link_free(_handle);
        }

        /// <summary>
        /// The native handle, failing when closed.
        /// </summary>
        /// <returns>The live handle.</returns>
        public IntPtr RequireOpen()
        {
            if (_handle == IntPtr.Zero)
                throw new PdfluentException("link is closed");
            return _handle;
        }
    }
}
