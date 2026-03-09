using System;

namespace XfaPdf
{
    /// <summary>
    /// An RGBA-rendered page image.
    /// </summary>
    public sealed class RenderedImage
    {
        /// <summary>Image width in pixels.</summary>
        public int Width { get; }

        /// <summary>Image height in pixels.</summary>
        public int Height { get; }

        /// <summary>Raw RGBA pixel data (4 bytes per pixel).</summary>
        public byte[] Pixels { get; }

        internal RenderedImage(int width, int height, byte[] pixels)
        {
            Width = width;
            Height = height;
            Pixels = pixels;
        }
    }
}
