// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

namespace PDFluent
{
    /// <summary>
    /// One image extracted from a page, as raw samples, by
    /// <see cref="PdfDocument.ExtractImages(int)"/>.
    /// </summary>
    /// <remarks>
    /// <see cref="Data"/> holds the decoded sample bytes, not an encoded
    /// file: there is no PNG/JPEG header to sniff. Use the page's image
    /// metadata (see <see cref="ImageRef"/>) for colour space and depth.
    /// </remarks>
    public sealed class RawImage
    {
        /// <summary>Image width in pixels.</summary>
        public int Width { get; }

        /// <summary>Image height in pixels.</summary>
        public int Height { get; }

        /// <summary>Decoded image bytes.</summary>
        public byte[] Data { get; }

        internal RawImage(int width, int height, byte[] data)
        {
            Width = width;
            Height = height;
            Data = data;
        }
    }
}
