// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;

namespace PDFluent
{
    /// <summary>One outline entry.</summary>
    public sealed class TocEntry
    {
        /// <summary>Nesting level, from 1.</summary>
        public int Level { get; }

        /// <summary>Entry title.</summary>
        public string Title { get; }

        /// <summary>1-based destination page.</summary>
        public int Page { get; }

        /// <summary>Whether a follow-on destination exists.</summary>
        public bool HasDest { get; }

        /// <summary>Destination kind, or -1.</summary>
        public int DestKind { get; }

        /// <summary>Destination page, or -1.</summary>
        public int DestPage { get; }

        /// <summary>Destination point, or the origin.</summary>
        public Point DestTarget { get; }

        /// <summary>Destination zoom, or 0.</summary>
        public double DestZoom { get; }

        /// <summary>Build a tocEntry.</summary>
        public TocEntry(int level, string title, int page, bool hasDest, int destKind, int destPage, Point destTarget, double destZoom)
        {
            Level = level;
            Title = title;
            Page = page;
            HasDest = hasDest;
            DestKind = destKind;
            DestPage = destPage;
            DestTarget = destTarget;
            DestZoom = destZoom;
        }

        internal static unsafe List<TocEntry> ReadList(IntPtr list)
        {
            CompatSupport.Check(
                NativeMethods.pdf_compat_toc_list_count(list, out int n), "Read");
            var result = new List<TocEntry>(n);
            for (int i = 0; i < n; i++)
            {
                CompatSupport.Check(NativeMethods.pdf_compat_toc_list_level(list, i, out int vLevel), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_toc_list_title(list, i, out IntPtr vTitle), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_toc_list_page(list, i, out int vPage), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_toc_list_has_dest(list, i, out int vHasDest), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_toc_list_dest_kind(list, i, out int vDestKind), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_toc_list_dest_page(list, i, out int vDestPage), "Read");
                Point vDestTarget = default;
                CompatSupport.Check(NativeMethods.pdf_compat_toc_list_dest_target(list, i, &vDestTarget), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_toc_list_dest_zoom(list, i, out double vDestZoom), "Read");
                result.Add(new TocEntry(vLevel, CompatSupport.ReadString(vTitle), vPage, vHasDest != 0, vDestKind, vDestPage, vDestTarget, vDestZoom));
            }
            return result;
        }
    }
}
