using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading.Tasks;

namespace XfaPdf
{
    /// <summary>
    /// A PDF document backed by the native XFA PDF engine.
    ///
    /// <example>
    /// <code>
    /// using var doc = PdfDocument.Open("input.pdf");
    /// Console.WriteLine($"Pages: {doc.PageCount}");
    /// string text = doc.ExtractText(0);
    /// RenderedImage img = doc.RenderPage(0, 150.0);
    /// </code>
    /// </example>
    /// </summary>
    public sealed class PdfDocument : IDisposable
    {
        private readonly PdfDocumentHandle _handle;
        private bool _disposed;

        private PdfDocument(PdfDocumentHandle handle)
        {
            _handle = handle;
        }

        // ---- Static factory methods ----

        /// <summary>
        /// Open a PDF from a file path.
        /// </summary>
        /// <param name="path">Path to the PDF file.</param>
        /// <param name="password">Optional document password.</param>
        /// <returns>A new PdfDocument instance.</returns>
        /// <exception cref="PdfException">If the file cannot be read or parsed.</exception>
        public static PdfDocument Open(string path, string? password = null)
        {
            PdfStatus status = NativeMethods.pdf_document_open(path, password, out IntPtr ptr);
            if (status != PdfStatus.Ok)
                ThrowForStatus(status, $"failed to open '{path}'");
            return new PdfDocument(new PdfDocumentHandle(ptr));
        }

        /// <summary>
        /// Open a PDF from raw bytes.
        /// </summary>
        /// <param name="data">PDF file contents.</param>
        /// <returns>A new PdfDocument instance.</returns>
        /// <exception cref="PdfException">If the data cannot be parsed.</exception>
        public static PdfDocument Open(byte[] data)
        {
            if (data == null) throw new ArgumentNullException(nameof(data));
            PdfStatus status = NativeMethods.pdf_document_open_from_bytes(
                data, (UIntPtr)data.Length, out IntPtr ptr);
            if (status != PdfStatus.Ok)
                ThrowForStatus(status, "failed to open PDF from bytes");
            return new PdfDocument(new PdfDocumentHandle(ptr));
        }

        /// <summary>
        /// Open a PDF from a file path asynchronously.
        /// </summary>
        public static async Task<PdfDocument> OpenAsync(string path, string? password = null)
        {
            byte[] data = await ReadAllBytesAsync(path).ConfigureAwait(false);
            return Open(data);
        }

        // ---- Properties ----

        /// <summary>Number of pages in the document.</summary>
        public int PageCount
        {
            get
            {
                ThrowIfDisposed();
                int count = NativeMethods.pdf_document_page_count(_handle.DangerousGetHandle());
                if (count < 0)
                    ThrowForLastError("failed to get page count");
                return count;
            }
        }

        /// <summary>Number of top-level bookmarks.</summary>
        public int BookmarkCount
        {
            get
            {
                ThrowIfDisposed();
                return NativeMethods.pdf_bookmark_count(_handle.DangerousGetHandle());
            }
        }

        // ---- Page queries ----

        /// <summary>Width of a page in PDF points (1/72 inch).</summary>
        public double GetPageWidth(int pageIndex)
        {
            ThrowIfDisposed();
            return NativeMethods.pdf_page_width(_handle.DangerousGetHandle(), pageIndex);
        }

        /// <summary>Height of a page in PDF points (1/72 inch).</summary>
        public double GetPageHeight(int pageIndex)
        {
            ThrowIfDisposed();
            return NativeMethods.pdf_page_height(_handle.DangerousGetHandle(), pageIndex);
        }

        /// <summary>Rotation of a page in degrees (0, 90, 180, 270).</summary>
        public int GetPageRotation(int pageIndex)
        {
            ThrowIfDisposed();
            return NativeMethods.pdf_page_rotation(_handle.DangerousGetHandle(), pageIndex);
        }

        /// <summary>Get the MediaBox of a page.</summary>
        public PageBox GetMediaBox(int pageIndex)
        {
            ThrowIfDisposed();
            PdfStatus status = NativeMethods.pdf_page_media_box(
                _handle.DangerousGetHandle(), pageIndex,
                out double x0, out double y0, out double x1, out double y1);
            if (status != PdfStatus.Ok)
                ThrowForStatus(status, $"failed to get media box for page {pageIndex}");
            return new PageBox(x0, y0, x1, y1);
        }

        /// <summary>Get the CropBox of a page.</summary>
        public PageBox GetCropBox(int pageIndex)
        {
            ThrowIfDisposed();
            PdfStatus status = NativeMethods.pdf_page_crop_box(
                _handle.DangerousGetHandle(), pageIndex,
                out double x0, out double y0, out double x1, out double y1);
            if (status != PdfStatus.Ok)
                ThrowForStatus(status, $"failed to get crop box for page {pageIndex}");
            return new PageBox(x0, y0, x1, y1);
        }

        // ---- Text extraction ----

        /// <summary>
        /// Extract text from a page.
        /// </summary>
        /// <param name="pageIndex">Zero-based page index.</param>
        /// <returns>The extracted text, or empty string if no text.</returns>
        /// <exception cref="PdfException">If the page index is out of range.</exception>
        public string ExtractText(int pageIndex)
        {
            ThrowIfDisposed();
            IntPtr ptr = NativeMethods.pdf_page_extract_text(
                _handle.DangerousGetHandle(), pageIndex);
            if (ptr == IntPtr.Zero)
            {
                string? err = GetLastErrorMessage();
                if (err != null)
                    throw new PdfException(PdfStatus.ErrorPageRange, err);
                return string.Empty;
            }
            try
            {
                return MarshalUtf8String(ptr);
            }
            finally
            {
                NativeMethods.pdf_string_free(ptr);
            }
        }

        /// <summary>Extract text from a page asynchronously.</summary>
        public Task<string> ExtractTextAsync(int pageIndex)
        {
            return Task.Run(() => ExtractText(pageIndex));
        }

        // ---- Rendering ----

        /// <summary>
        /// Render a page to RGBA pixels at the specified DPI.
        /// </summary>
        /// <param name="pageIndex">Zero-based page index.</param>
        /// <param name="dpi">Dots per inch (72 = 1:1, 150 = standard, 300 = high quality).</param>
        /// <returns>Rendered image with RGBA pixel data.</returns>
        public RenderedImage RenderPage(int pageIndex, double dpi)
        {
            ThrowIfDisposed();
            PdfStatus status = NativeMethods.pdf_page_render(
                _handle.DangerousGetHandle(), pageIndex, dpi,
                out uint width, out uint height, out IntPtr pixels);
            if (status != PdfStatus.Ok)
                ThrowForStatus(status, $"failed to render page {pageIndex}");
            return CopyAndFreePixels(width, height, pixels);
        }

        /// <summary>Render a page asynchronously.</summary>
        public Task<RenderedImage> RenderPageAsync(int pageIndex, double dpi)
        {
            return Task.Run(() => RenderPage(pageIndex, dpi));
        }

        /// <summary>
        /// Render a thumbnail constrained to a maximum dimension.
        /// </summary>
        /// <param name="pageIndex">Zero-based page index.</param>
        /// <param name="maxDimension">Maximum width or height in pixels.</param>
        public RenderedImage RenderThumbnail(int pageIndex, int maxDimension)
        {
            ThrowIfDisposed();
            PdfStatus status = NativeMethods.pdf_page_render_thumbnail(
                _handle.DangerousGetHandle(), pageIndex, (uint)maxDimension,
                out uint width, out uint height, out IntPtr pixels);
            if (status != PdfStatus.Ok)
                ThrowForStatus(status, $"failed to render thumbnail for page {pageIndex}");
            return CopyAndFreePixels(width, height, pixels);
        }

        // ---- Metadata ----

        /// <summary>
        /// Get a metadata value.
        /// </summary>
        /// <param name="key">One of: "Title", "Author", "Subject", "Keywords", "Creator", "Producer".</param>
        /// <returns>The metadata value, or null if not set.</returns>
        public string? GetMetadata(string key)
        {
            ThrowIfDisposed();
            IntPtr ptr = NativeMethods.pdf_document_get_meta(
                _handle.DangerousGetHandle(), key);
            if (ptr == IntPtr.Zero)
                return null;
            try
            {
                return MarshalUtf8String(ptr);
            }
            finally
            {
                NativeMethods.pdf_string_free(ptr);
            }
        }

        // ---- IDisposable ----

        /// <summary>Whether the document has been disposed.</summary>
        public bool IsDisposed => _disposed;

        public void Dispose()
        {
            if (!_disposed)
            {
                _handle.Dispose();
                _disposed = true;
            }
        }

        // ---- Private helpers ----

        private void ThrowIfDisposed()
        {
            if (_disposed)
                throw new ObjectDisposedException(nameof(PdfDocument));
        }

        private static RenderedImage CopyAndFreePixels(uint width, uint height, IntPtr pixels)
        {
            int len = (int)(width * height * 4);
            byte[] managed = new byte[len];
            Marshal.Copy(pixels, managed, 0, len);
            NativeMethods.pdf_pixels_free(pixels, (UIntPtr)len);
            return new RenderedImage((int)width, (int)height, managed);
        }

        private static string MarshalUtf8String(IntPtr ptr)
        {
#if NETSTANDARD2_0
            // netstandard2.0: no PtrToStringUTF8, use ANSI (safe for ASCII/Latin-1)
            return Marshal.PtrToStringAnsi(ptr) ?? string.Empty;
#else
            return Marshal.PtrToStringUTF8(ptr) ?? string.Empty;
#endif
        }

        private static string? GetLastErrorMessage()
        {
            IntPtr ptr = NativeMethods.pdf_get_last_error();
            if (ptr == IntPtr.Zero)
                return null;
            return MarshalUtf8String(ptr);
        }

        private static void ThrowForStatus(PdfStatus status, string fallback)
        {
            string? nativeMsg = GetLastErrorMessage();
            throw new PdfException(status, nativeMsg ?? fallback);
        }

        private static void ThrowForLastError(string fallback)
        {
            string? msg = GetLastErrorMessage();
            throw new PdfException(msg ?? fallback);
        }

        private static async Task<byte[]> ReadAllBytesAsync(string path)
        {
#if NETSTANDARD2_0
            return await Task.Run(() => File.ReadAllBytes(path)).ConfigureAwait(false);
#else
            return await File.ReadAllBytesAsync(path).ConfigureAwait(false);
#endif
        }
    }
}
