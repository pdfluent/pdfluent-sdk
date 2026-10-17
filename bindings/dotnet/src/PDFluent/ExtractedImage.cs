// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

namespace PDFluent
{
    /// <summary>
    /// One extracted image: header facts plus encoded bytes.
    /// </summary>
    /// <remarks>
    /// JPEG payloads pass through untouched; everything else arrives decoded
    /// and re-encoded as PNG. Returned by
    /// <see cref="PdfDocument.ExtractImageData(uint)"/>.
    /// </remarks>
    public sealed class ExtractedImage
    {
        /// <summary>Image width in pixels.</summary>
        public int Width { get; }

        /// <summary>Image height in pixels.</summary>
        public int Height { get; }

        /// <summary>Bits per component.</summary>
        public int BitsPerComponent { get; }

        /// <summary>Components per pixel.</summary>
        public int Components { get; }

        /// <summary>
        /// Decoded pixel layout: <c>"DeviceGray"</c>, <c>"DeviceRGB"</c> or
        /// <c>"DeviceCMYK"</c>.
        /// </summary>
        public string ColorSpace { get; }

        /// <summary>Colour-space name recorded in the image dictionary.</summary>
        public string ColorSpaceName { get; }

        /// <summary>
        /// Encoding of <see cref="Data"/>: <c>"jpeg"</c> or <c>"png"</c>.
        /// </summary>
        public string Encoding { get; }

        /// <summary>Encoded image bytes.</summary>
        public byte[] Data { get; }

        /// <summary>Whether the image is masked.</summary>
        public bool Masked { get; }

        /// <summary>Horizontal resolution in dots per inch.</summary>
        public int Xres { get; }

        /// <summary>Vertical resolution in dots per inch.</summary>
        public int Yres { get; }

        internal ExtractedImage(
            int width,
            int height,
            int bitsPerComponent,
            int components,
            string colorSpace,
            string colorSpaceName,
            string encoding,
            byte[] data,
            bool masked,
            int xres,
            int yres)
        {
            Width = width;
            Height = height;
            BitsPerComponent = bitsPerComponent;
            Components = components;
            ColorSpace = colorSpace;
            ColorSpaceName = colorSpaceName;
            Encoding = encoding;
            Data = data;
            Masked = masked;
            Xres = xres;
            Yres = yres;
        }
    }
}
