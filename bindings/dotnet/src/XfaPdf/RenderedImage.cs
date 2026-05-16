namespace XfaPdf
{
    /// <summary>
    /// An RGBA-rendered page image returned by <see cref="PdfDocument.RenderPage"/> or
    /// <see cref="PdfDocument.RenderThumbnail"/>.
    /// </summary>
    /// <remarks>
    /// Pixel data is in row-major order, four bytes per pixel (R, G, B, A).
    /// Total byte count is <c>Width * Height * 4</c>.
    /// </remarks>
    public sealed class RenderedImage
    {
        /// <summary>Image width in pixels.</summary>
        public int Width { get; }

        /// <summary>Image height in pixels.</summary>
        public int Height { get; }

        /// <summary>
        /// Raw RGBA pixel data (4 bytes per pixel, row-major, top-to-bottom).
        /// </summary>
        public byte[] Pixels { get; }

        internal RenderedImage(int width, int height, byte[] pixels)
        {
            Width = width;
            Height = height;
            Pixels = pixels;
        }
    }
}
