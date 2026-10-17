// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;

namespace PDFluent
{
    /// <summary>Extracted image bytes and facts.</summary>
    public sealed class ImageData
    {
        /// <summary>Width in pixels.</summary>
        public int Width { get; }

        /// <summary>Height in pixels.</summary>
        public int Height { get; }

        /// <summary>Bits per component.</summary>
        public int Bits { get; }

        /// <summary>Colour-space code.</summary>
        public int Colorspace { get; }

        /// <summary>Colour-space name.</summary>
        public string ColorspaceName { get; }

        /// <summary>File extension.</summary>
        public string Ext { get; }

        /// <summary>Payload size in bytes.</summary>
        public int Size { get; }

        /// <summary>Whether masked.</summary>
        public bool Masked { get; }

        /// <summary>Horizontal resolution.</summary>
        public int Xres { get; }

        /// <summary>Vertical resolution.</summary>
        public int Yres { get; }

        /// <summary>Raw payload bytes.</summary>
        public byte[] Data { get; }

        /// <summary>Build a imageData.</summary>
        public ImageData(int width, int height, int bits, int colorspace, string colorspaceName, string ext, int size, bool masked, int xres, int yres, byte[] data)
        {
            Width = width;
            Height = height;
            Bits = bits;
            Colorspace = colorspace;
            ColorspaceName = colorspaceName;
            Ext = ext;
            Size = size;
            Masked = masked;
            Xres = xres;
            Yres = yres;
            Data = data;
        }

        internal static unsafe ImageData Read(IntPtr list)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_image_data_width(list, out int vWidth), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_image_data_height(list, out int vHeight), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_image_data_bits(list, out int vBits), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_image_data_colorspace(list, out int vColorspace), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_image_data_colorspace_name(list, out IntPtr vColorspaceName), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_image_data_ext(list, out IntPtr vExt), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_image_data_size(list, out int vSize), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_image_data_masked(list, out int vMasked), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_image_data_xres(list, out int vXres), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_image_data_yres(list, out int vYres), "Read");
            CompatSupport.Check(NativeMethods.pdf_compat_image_data_data(list, out IntPtr vData, out UIntPtr vDataLen), "Read");
            return new ImageData(vWidth, vHeight, vBits, vColorspace, CompatSupport.ReadString(vColorspaceName), CompatSupport.ReadString(vExt), vSize, vMasked != 0, vXres, vYres, CompatSupport.ReadBytes(vData, vDataLen));
        }
    }
}
