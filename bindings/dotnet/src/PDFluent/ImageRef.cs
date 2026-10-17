// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;

namespace PDFluent
{
    /// <summary>One image reference.</summary>
    public sealed class ImageRef
    {
        /// <summary>Image object number.</summary>
        public int Xref { get; }

        /// <summary>Mask object number.</summary>
        public int Mask { get; }

        /// <summary>Width in pixels.</summary>
        public int Width { get; }

        /// <summary>Height in pixels.</summary>
        public int Height { get; }

        /// <summary>Bits per component.</summary>
        public int Bits { get; }

        /// <summary>Colour-space name.</summary>
        public string ColorspaceName { get; }

        /// <summary>Alternate name.</summary>
        public string Alt { get; }

        /// <summary>Resource name.</summary>
        public string Name { get; }

        /// <summary>Filter name.</summary>
        public string Filter { get; }

        /// <summary>Build a imageRef.</summary>
        public ImageRef(int xref, int mask, int width, int height, int bits, string colorspaceName, string alt, string name, string filter)
        {
            Xref = xref;
            Mask = mask;
            Width = width;
            Height = height;
            Bits = bits;
            ColorspaceName = colorspaceName;
            Alt = alt;
            Name = name;
            Filter = filter;
        }

        internal static unsafe List<ImageRef> ReadList(IntPtr list)
        {
            CompatSupport.Check(
                NativeMethods.pdf_compat_image_ref_list_count(list, out int n), "Read");
            var result = new List<ImageRef>(n);
            for (int i = 0; i < n; i++)
            {
                CompatSupport.Check(NativeMethods.pdf_compat_image_ref_list_xref(list, i, out int vXref), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_image_ref_list_mask(list, i, out int vMask), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_image_ref_list_width(list, i, out int vWidth), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_image_ref_list_height(list, i, out int vHeight), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_image_ref_list_bits(list, i, out int vBits), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_image_ref_list_colorspace_name(list, i, out IntPtr vColorspaceName), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_image_ref_list_alt(list, i, out IntPtr vAlt), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_image_ref_list_name(list, i, out IntPtr vName), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_image_ref_list_filter(list, i, out IntPtr vFilter), "Read");
                result.Add(new ImageRef(vXref, vMask, vWidth, vHeight, vBits, CompatSupport.ReadString(vColorspaceName), CompatSupport.ReadString(vAlt), CompatSupport.ReadString(vName), CompatSupport.ReadString(vFilter)));
            }
            return result;
        }
    }
}
