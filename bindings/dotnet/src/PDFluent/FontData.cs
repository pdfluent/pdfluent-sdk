// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;

namespace PDFluent
{
    /// <summary>Extracted font bytes and facts.</summary>
    public sealed class FontData
    {
        /// <summary>Font name.</summary>
        public string Name { get; }

        /// <summary>Font bytes.</summary>
        public byte[] Data { get; }

        /// <summary>Build a fontData.</summary>
        public FontData(string name, byte[] data)
        {
            Name = name;
            Data = data;
        }

        internal static unsafe FontData Read(IntPtr list)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_font_data_name(list, out IntPtr vName), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_font_data_data(list, out IntPtr vData, out UIntPtr vDataLen), "Read");
            return new FontData(CompatSupport.ReadString(vName), CompatSupport.ReadBytes(vData, vDataLen));
        }
    }
}
