// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;

namespace PDFluent
{
    /// <summary>One placed image.</summary>
    public sealed class PlacedImage
    {
        /// <summary>Placed box in points.</summary>
        public Rect Bbox { get; }

        /// <summary>Image-to-page matrix.</summary>
        public Matrix Transform { get; }

        /// <summary>Width in pixels.</summary>
        public int Width { get; }

        /// <summary>Height in pixels.</summary>
        public int Height { get; }

        /// <summary>Bits per component.</summary>
        public int Bits { get; }

        /// <summary>Payload size in bytes.</summary>
        public int Size { get; }

        /// <summary>Sequence number.</summary>
        public int Number { get; }

        /// <summary>Colour-space name.</summary>
        public string ColorspaceName { get; }

        /// <summary>Whether masked.</summary>
        public bool HasMask { get; }

        /// <summary>Horizontal resolution.</summary>
        public int Xres { get; }

        /// <summary>Vertical resolution.</summary>
        public int Yres { get; }

        /// <summary>Whether a digest is present.</summary>
        public bool HasDigest { get; }

        /// <summary>Digest bytes, or empty.</summary>
        public byte[] Digest { get; }

        /// <summary>Build a placedImage.</summary>
        public PlacedImage(Rect bbox, Matrix transform, int width, int height, int bits, int size, int number, string colorspaceName, bool hasMask, int xres, int yres, bool hasDigest, byte[] digest)
        {
            Bbox = bbox;
            Transform = transform;
            Width = width;
            Height = height;
            Bits = bits;
            Size = size;
            Number = number;
            ColorspaceName = colorspaceName;
            HasMask = hasMask;
            Xres = xres;
            Yres = yres;
            HasDigest = hasDigest;
            Digest = digest;
        }

        internal static unsafe List<PlacedImage> ReadList(IntPtr list)
        {
            CompatSupport.Check(
                NativeMethods.pdf_compat_placed_image_list_count(list, out int n), "Read");
            var result = new List<PlacedImage>(n);
            for (int i = 0; i < n; i++)
            {
                Rect vBbox = default;
                CompatSupport.Check(NativeMethods.pdf_compat_placed_image_list_bbox(list, i, &vBbox), "Read");
                Matrix vTransform = default;
                CompatSupport.Check(NativeMethods.pdf_compat_placed_image_list_transform(list, i, &vTransform), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_placed_image_list_width(list, i, out int vWidth), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_placed_image_list_height(list, i, out int vHeight), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_placed_image_list_bits(list, i, out int vBits), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_placed_image_list_size(list, i, out int vSize), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_placed_image_list_number(list, i, out int vNumber), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_placed_image_list_colorspace_name(list, i, out IntPtr vColorspaceName), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_placed_image_list_has_mask(list, i, out int vHasMask), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_placed_image_list_xres(list, i, out int vXres), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_placed_image_list_yres(list, i, out int vYres), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_placed_image_list_has_digest(list, i, out int vHasDigest), "Read");
                CompatSupport.Check(NativeMethods.pdf_compat_placed_image_list_digest(list, i, out IntPtr vDigest, out UIntPtr vDigestLen), "Read");
                result.Add(new PlacedImage(vBbox, vTransform, vWidth, vHeight, vBits, vSize, vNumber, CompatSupport.ReadString(vColorspaceName), vHasMask != 0, vXres, vYres, vHasDigest != 0, CompatSupport.ReadBytes(vDigest, vDigestLen)));
            }
            return result;
        }
    }
}
