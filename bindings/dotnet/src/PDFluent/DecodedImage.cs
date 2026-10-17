// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

namespace PDFluent
{
    /// <summary>
    /// One decoded image: raw samples, top row first.
    /// </summary>
    /// <remarks>
    /// Palettes and stencils are expanded. Returned by
    /// <see cref="PdfDocument.DecodeImagePixels(uint)"/>.
    /// </remarks>
    public sealed class DecodedImage
    {
        /// <summary>Image width in pixels.</summary>
        public int Width { get; }

        /// <summary>Image height in pixels.</summary>
        public int Height { get; }

        /// <summary>Components per pixel.</summary>
        public int Components { get; }

        /// <summary>
        /// Decoded pixel layout: <c>"DeviceGray"</c>, <c>"DeviceRGB"</c> or
        /// <c>"DeviceCMYK"</c>.
        /// </summary>
        public string ColorSpace { get; }

        /// <summary>
        /// Raw samples, <see cref="Components"/> bytes per pixel.
        /// </summary>
        public byte[] Pixels { get; }

        internal DecodedImage(int width, int height, int components, string colorSpace, byte[] pixels)
        {
            Width = width;
            Height = height;
            Components = components;
            ColorSpace = colorSpace;
            Pixels = pixels;
        }
    }
}
