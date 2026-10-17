// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;

namespace PDFluent
{
    /// <summary>One font reference.</summary>
    public sealed class FontInfo
    {
        /// <summary>Font object number.</summary>
        public int Xref { get; }

        /// <summary>Font name.</summary>
        public string Name { get; }

        /// <summary>Build a fontInfo.</summary>
        public FontInfo(int xref, string name)
        {
            Xref = xref;
            Name = name;
        }

        internal static unsafe List<FontInfo> ReadList(IntPtr list)
        {
            CompatSupport.Check(
                NativeMethods.pdf_compat_font_info_list_count(list, out int n), "Read");
            var result = new List<FontInfo>(n);
            for (int i = 0; i < n; i++)
            {
                CompatSupport.Check(NativeMethods.pdf_compat_font_info_list_xref(list, i, out int vXref), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_font_info_list_name(list, i, out IntPtr vName), "Read");
                result.Add(new FontInfo(vXref, CompatSupport.ReadString(vName)));
            }
            return result;
        }
    }
}
