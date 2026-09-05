using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading.Tasks;

namespace PDFluent
{
    /// <summary>
    /// A PDF document backed by the native PDFluent engine.
    /// </summary>
    /// <remarks>
    /// <para>
    /// Open a document via the static factory methods <see cref="Open(string,string?)"/>
    /// or <see cref="Open(byte[])"/>.  Always dispose the document when done —
    /// prefer <see langword="using"/> declarations or <see langword="using"/> blocks.
    /// </para>
    /// <example>
    /// <code>
    /// using var doc = PdfDocument.Open("input.pdf");
    /// Console.WriteLine($"Pages: {doc.PageCount}");
    /// string text = doc.ExtractText(0);
    /// RenderedImage img = doc.RenderPage(0, 150.0);
    /// </code>
    /// </example>
    /// </remarks>
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
        /// <returns>A new <see cref="PdfDocument"/> instance.</returns>
        /// <exception cref="PdfluentIoException">If the file cannot be found or read.</exception>
        /// <exception cref="PdfluentParseException">If the file is not a valid PDF.</exception>
        /// <exception cref="PdfluentPermissionException">If the PDF requires a password.</exception>
        public static PdfDocument Open(string path, string? password = null)
        {
            PdfStatus status = NativeMethods.pdf_document_open(path, password, out IntPtr ptr);
            if (status != PdfStatus.Ok)
                throw ThrowForStatus(status, $"failed to open '{path}'");
            return new PdfDocument(new PdfDocumentHandle(ptr));
        }

        /// <summary>
        /// Open a PDF from raw bytes.
        /// </summary>
        /// <param name="data">PDF file contents.</param>
        /// <returns>A new <see cref="PdfDocument"/> instance.</returns>
        /// <exception cref="ArgumentNullException">If <paramref name="data"/> is <see langword="null"/>.</exception>
        /// <exception cref="PdfluentParseException">If the data cannot be parsed as a PDF.</exception>
        public static PdfDocument Open(byte[] data)
        {
            if (data == null) throw new ArgumentNullException(nameof(data));
            PdfStatus status = NativeMethods.pdf_document_open_from_bytes(
                data, (UIntPtr)data.Length, out IntPtr ptr);
            if (status != PdfStatus.Ok)
                throw ThrowForStatus(status, "failed to open PDF from bytes");
            return new PdfDocument(new PdfDocumentHandle(ptr));
        }

        /// <summary>
        /// Open a PDF from a file path asynchronously.
        /// </summary>
        /// <param name="path">Path to the PDF file.</param>
        /// <param name="password">Optional document password.</param>
        /// <returns>
        /// A <see cref="Task{TResult}"/> whose result is a new
        /// <see cref="PdfDocument"/> instance.
        /// </returns>
        /// <exception cref="PdfluentIoException">If the file cannot be found or read.</exception>
        /// <exception cref="PdfluentParseException">If the file is not a valid PDF.</exception>
        public static async Task<PdfDocument> OpenAsync(string path, string? password = null)
        {
            byte[] data = await ReadAllBytesAsync(path).ConfigureAwait(false);
            return Open(data);
        }

        // ---- Properties ----

        /// <summary>Number of pages in the document.</summary>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        public int PageCount
        {
            get
            {
                ThrowIfDisposed();
                int count = NativeMethods.pdf_document_page_count(_handle.DangerousGetHandle());
                if (count < 0)
                    throw ThrowForLastError("failed to get page count");
                return count;
            }
        }

        /// <summary>Number of top-level bookmarks in the document outline.</summary>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        public int BookmarkCount
        {
            get
            {
                ThrowIfDisposed();
                return NativeMethods.pdf_bookmark_count(_handle.DangerousGetHandle());
            }
        }

        // ---- Page queries ----

        /// <summary>Returns the width of a page in PDF points (1/72 inch).</summary>
        /// <param name="pageIndex">Zero-based page index.</param>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        public double GetPageWidth(int pageIndex)
        {
            ThrowIfDisposed();
            return NativeMethods.pdf_page_width(_handle.DangerousGetHandle(), pageIndex);
        }

        /// <summary>Returns the height of a page in PDF points (1/72 inch).</summary>
        /// <param name="pageIndex">Zero-based page index.</param>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        public double GetPageHeight(int pageIndex)
        {
            ThrowIfDisposed();
            return NativeMethods.pdf_page_height(_handle.DangerousGetHandle(), pageIndex);
        }

        /// <summary>Returns the rotation of a page in degrees (0, 90, 180, or 270).</summary>
        /// <param name="pageIndex">Zero-based page index.</param>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        public int GetPageRotation(int pageIndex)
        {
            ThrowIfDisposed();
            return NativeMethods.pdf_page_rotation(_handle.DangerousGetHandle(), pageIndex);
        }

        /// <summary>Returns the MediaBox of a page.</summary>
        /// <param name="pageIndex">Zero-based page index.</param>
        /// <returns>The page MediaBox in PDF points.</returns>
        /// <exception cref="PdfluentPageRangeException">If <paramref name="pageIndex"/> is out of range.</exception>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        public PageBox GetMediaBox(int pageIndex)
        {
            ThrowIfDisposed();
            PdfStatus status = NativeMethods.pdf_page_media_box(
                _handle.DangerousGetHandle(), pageIndex,
                out double x0, out double y0, out double x1, out double y1);
            if (status != PdfStatus.Ok)
                throw ThrowForStatus(status, $"failed to get media box for page {pageIndex}");
            return new PageBox(x0, y0, x1, y1);
        }

        /// <summary>Returns the CropBox of a page.</summary>
        /// <param name="pageIndex">Zero-based page index.</param>
        /// <returns>The page CropBox in PDF points.</returns>
        /// <exception cref="PdfluentPageRangeException">If <paramref name="pageIndex"/> is out of range.</exception>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        public PageBox GetCropBox(int pageIndex)
        {
            ThrowIfDisposed();
            PdfStatus status = NativeMethods.pdf_page_crop_box(
                _handle.DangerousGetHandle(), pageIndex,
                out double x0, out double y0, out double x1, out double y1);
            if (status != PdfStatus.Ok)
                throw ThrowForStatus(status, $"failed to get crop box for page {pageIndex}");
            return new PageBox(x0, y0, x1, y1);
        }

        // ---- Office export ----

        /// <summary>Converts this document to a Word <c>.docx</c> package.</summary>
        /// <remarks>
        /// Requires a Business licence or higher. Without one this throws with
        /// the engine's own message, which points at the free 30-day
        /// evaluation key before the price list.
        /// </remarks>
        /// <returns>The <c>.docx</c> bytes.</returns>
        /// <exception cref="PdfluentException">If the tier does not include Office export, or the conversion fails.</exception>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        public byte[] ToDocx() => OfficeExport(NativeMethods.pdf_document_to_docx, "docx");

        /// <summary>Converts this document to an Excel <c>.xlsx</c> workbook.</summary>
        /// <returns>The <c>.xlsx</c> bytes.</returns>
        /// <seealso cref="ToDocx"/>
        public byte[] ToXlsx() => OfficeExport(NativeMethods.pdf_document_to_xlsx, "xlsx");

        /// <summary>Converts this document to a PowerPoint <c>.pptx</c> deck, one slide per page.</summary>
        /// <returns>The <c>.pptx</c> bytes.</returns>
        /// <seealso cref="ToDocx"/>
        public byte[] ToPptx() => OfficeExport(NativeMethods.pdf_document_to_pptx, "pptx");

        private delegate PdfStatus OfficeCall(IntPtr doc, out IntPtr outData, out UIntPtr outLen);

        private byte[] OfficeExport(OfficeCall call, string what)
        {
            ThrowIfDisposed();
            PdfStatus status = call(_handle.DangerousGetHandle(), out IntPtr data, out UIntPtr len);
            if (status != PdfStatus.Ok)
                throw ThrowForStatus(status, $"{what} conversion failed");

            int size = checked((int)len.ToUInt64());
            try
            {
                // Copy before freeing: the array handed to the caller has to
                // outlive the native allocation.
                byte[] managed = new byte[size];
                Marshal.Copy(data, managed, 0, size);
                return managed;
            }
            finally
            {
                NativeMethods.pdf_bytes_free(data, len);
            }
        }

        // ---- Text extraction ----

        /// <summary>
        /// Extracts text from a page.
        /// </summary>
        /// <param name="pageIndex">Zero-based page index.</param>
        /// <returns>The extracted text, or an empty string if no text is present.</returns>
        /// <exception cref="PdfluentPageRangeException">If <paramref name="pageIndex"/> is out of range.</exception>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        public string ExtractText(int pageIndex)
        {
            ThrowIfDisposed();
            IntPtr ptr = NativeMethods.pdf_page_extract_text(
                _handle.DangerousGetHandle(), pageIndex);
            if (ptr == IntPtr.Zero)
            {
                string? err = GetLastErrorMessage();
                if (err != null)
                    throw new PdfluentPageRangeException(PdfStatus.ErrorPageRange, err);
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

        /// <summary>Extracts text from a page asynchronously.</summary>
        /// <param name="pageIndex">Zero-based page index.</param>
        /// <returns>A <see cref="Task{TResult}"/> whose result is the extracted text.</returns>
        /// <exception cref="PdfluentPageRangeException">If <paramref name="pageIndex"/> is out of range.</exception>
        public Task<string> ExtractTextAsync(int pageIndex)
        {
            return Task.Run(() => ExtractText(pageIndex));
        }

        /// <summary>
        /// Extracts the structured text blocks for a single page.
        /// </summary>
        /// <remarks>
        /// Each block carries its bounding box in PDF user-space points
        /// (origin = bottom-left) plus the concatenated UTF-8 text. The
        /// native array is freed automatically inside this method; the
        /// returned <see cref="TextBlock"/> values are pure managed
        /// objects and remain valid for the lifetime of the GC.
        /// </remarks>
        /// <param name="pageIndex">Zero-based page index.</param>
        /// <returns>An ordered list of <see cref="TextBlock"/> for the page.</returns>
        /// <exception cref="PdfluentValidationException">If a null pointer is passed (defensive).</exception>
        /// <exception cref="PdfluentPageRangeException">If <paramref name="pageIndex"/> is outside [0, page_count).</exception>
        /// <exception cref="PdfluentException">For engine extraction failures.</exception>
        public IReadOnlyList<TextBlock> ExtractTextBlocks(int pageIndex)
        {
            ThrowIfDisposed();
            IntPtr blocksPtr;
            UIntPtr count;
            PdfStatus rc = NativeMethods.pdf_page_extract_text_blocks(
                _handle.DangerousGetHandle(),
                pageIndex,
                out blocksPtr,
                out count);
            if (rc != PdfStatus.Ok)
            {
                string err = GetLastErrorMessage() ?? "text-block extraction failed";
                throw PdfluentException.FromStatus(rc, err);
            }
            ulong n = (ulong)count;
            if (n == 0UL || blocksPtr == IntPtr.Zero)
            {
                return Array.Empty<TextBlock>();
            }
            try
            {
                var result = new TextBlock[(int)n];
                int stride = Marshal.SizeOf<NativeMethods.PdfTextBlockNative>();
                for (int i = 0; i < (int)n; i++)
                {
                    IntPtr slot = IntPtr.Add(blocksPtr, i * stride);
                    NativeMethods.PdfTextBlockNative native =
                        Marshal.PtrToStructure<NativeMethods.PdfTextBlockNative>(slot);
                    string text = native.Text == IntPtr.Zero
                        ? string.Empty
                        : MarshalUtf8String(native.Text);
                    result[i] = new TextBlock(native.X, native.Y, native.Width, native.Height, text);
                }
                return result;
            }
            finally
            {
                NativeMethods.pdf_text_blocks_free(blocksPtr, count);
            }
        }

        // ---- Rendering ----

        /// <summary>
        /// Renders a page to RGBA pixels at the specified DPI.
        /// </summary>
        /// <param name="pageIndex">Zero-based page index.</param>
        /// <param name="dpi">Dots per inch (72 = 1:1, 150 = standard, 300 = high quality).</param>
        /// <returns>Rendered image with RGBA pixel data.</returns>
        /// <exception cref="PdfluentRenderException">If rendering fails.</exception>
        /// <exception cref="PdfluentPageRangeException">If <paramref name="pageIndex"/> is out of range.</exception>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        public RenderedImage RenderPage(int pageIndex, double dpi)
        {
            ThrowIfDisposed();
            PdfStatus status = NativeMethods.pdf_page_render(
                _handle.DangerousGetHandle(), pageIndex, dpi,
                out uint width, out uint height, out IntPtr pixels);
            if (status != PdfStatus.Ok)
                throw ThrowForStatus(status, $"failed to render page {pageIndex}");
            return CopyAndFreePixels(width, height, pixels);
        }

        /// <summary>Renders a page asynchronously.</summary>
        /// <param name="pageIndex">Zero-based page index.</param>
        /// <param name="dpi">Dots per inch.</param>
        /// <returns>A <see cref="Task{TResult}"/> whose result is the rendered image.</returns>
        public Task<RenderedImage> RenderPageAsync(int pageIndex, double dpi)
        {
            return Task.Run(() => RenderPage(pageIndex, dpi));
        }

        /// <summary>
        /// Renders a thumbnail constrained to a maximum dimension.
        /// </summary>
        /// <param name="pageIndex">Zero-based page index.</param>
        /// <param name="maxDimension">Maximum width or height in pixels.</param>
        /// <returns>Rendered thumbnail with RGBA pixel data.</returns>
        /// <exception cref="PdfluentRenderException">If rendering fails.</exception>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        public RenderedImage RenderThumbnail(int pageIndex, int maxDimension)
        {
            ThrowIfDisposed();
            PdfStatus status = NativeMethods.pdf_page_render_thumbnail(
                _handle.DangerousGetHandle(), pageIndex, (uint)maxDimension,
                out uint width, out uint height, out IntPtr pixels);
            if (status != PdfStatus.Ok)
                throw ThrowForStatus(status, $"failed to render thumbnail for page {pageIndex}");
            return CopyAndFreePixels(width, height, pixels);
        }

        // ---- Metadata ----

        /// <summary>
        /// Gets a document metadata value.
        /// </summary>
        /// <param name="key">
        /// One of: <c>"Title"</c>, <c>"Author"</c>, <c>"Subject"</c>,
        /// <c>"Keywords"</c>, <c>"Creator"</c>, <c>"Producer"</c>.
        /// </param>
        /// <returns>The metadata value, or <see langword="null"/> if not set.</returns>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
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

        // ---- Digital signing ----

        /// <summary>
        /// Digitally signs the document with a PKCS#12 (.p12/.pfx) credential
        /// bundle and returns a new, signed document.
        /// </summary>
        /// <remarks>
        /// The source document is not modified; signing produces a brand-new
        /// <see cref="PdfDocument"/> that the caller owns and must dispose.
        /// </remarks>
        /// <param name="pkcs12Path">Path to the <c>.p12</c>/<c>.pfx</c> bundle.</param>
        /// <param name="pkcs12Password">Password for the bundle, or
        /// <see langword="null"/> for a password-less bundle.</param>
        /// <returns>A new signed <see cref="PdfDocument"/>.</returns>
        /// <exception cref="ArgumentNullException">If <paramref name="pkcs12Path"/> is <see langword="null"/>.</exception>
        /// <exception cref="PdfluentIoException">If the PKCS#12 file cannot be found.</exception>
        /// <exception cref="PdfluentValidationException">If an argument is structurally invalid.</exception>
        /// <exception cref="PdfluentException">If PKCS#12 loading, signing, or the PDF write-back fails (<see cref="PdfStatus.ErrorSign"/>).</exception>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        public PdfDocument Sign(string pkcs12Path, string? pkcs12Password = null)
        {
            ThrowIfDisposed();
            if (pkcs12Path is null) throw new ArgumentNullException(nameof(pkcs12Path));
            PdfStatus status = NativeMethods.pdf_document_sign(
                _handle.DangerousGetHandle(), pkcs12Path, pkcs12Password, out IntPtr ptr);
            if (status != PdfStatus.Ok)
                throw ThrowForStatus(status, $"failed to sign with '{pkcs12Path}'");
            return new PdfDocument(new PdfDocumentHandle(ptr));
        }

        // ---- Signature verification ----

        /// <summary>
        /// Returns the number of signature fields present in the document.
        /// </summary>
        /// <returns>The signature-field count (≥ 0).</returns>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        public int SignatureCount()
        {
            ThrowIfDisposed();
            int count = NativeMethods.pdf_signature_count(_handle.DangerousGetHandle());
            return count < 0 ? 0 : count;
        }

        /// <summary>
        /// Validates the digital signature at <paramref name="index"/>.
        /// </summary>
        /// <param name="index">Zero-based signature index.</param>
        /// <returns>
        /// <see langword="true"/> if the signature is cryptographically valid,
        /// <see langword="false"/> if it is invalid, tampered, or the validity
        /// could not be determined (unknown / out-of-range).
        /// </returns>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        public bool IsSignatureValid(int index)
        {
            ThrowIfDisposed();
            return NativeMethods.pdf_signature_is_valid(
                _handle.DangerousGetHandle(), index) == 1;
        }

        /// <summary>
        /// Validates every digital signature in the document and returns a
        /// structured result per signature field.
        /// </summary>
        /// <remarks>
        /// The result list has one <see cref="SignatureValidation"/> entry per
        /// signature field, ordered by index. An empty list means the document
        /// carries no signatures. Use <see cref="SignatureValidation.AllValid"/>
        /// on the returned collection, or inspect each entry's
        /// <see cref="SignatureValidation.Status"/>.
        /// </remarks>
        /// <returns>An ordered, read-only list of per-signature validations.</returns>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        public IReadOnlyList<SignatureValidation> VerifySignatures()
        {
            ThrowIfDisposed();
            IntPtr docPtr = _handle.DangerousGetHandle();
            int count = NativeMethods.pdf_signature_count(docPtr);
            if (count <= 0)
                return Array.Empty<SignatureValidation>();

            var result = new SignatureValidation[count];
            for (int i = 0; i < count; i++)
            {
                int raw = NativeMethods.pdf_signature_is_valid(docPtr, i);
                SignatureStatus status = raw switch
                {
                    1 => SignatureStatus.Valid,
                    0 => SignatureStatus.Invalid,
                    _ => SignatureStatus.Unknown,
                };
                result[i] = new SignatureValidation(i, status);
            }
            return result;
        }

        // ---- IDisposable ----

        /// <summary>
        /// Whether the document has been disposed and can no longer be used.
        /// </summary>
        public bool IsDisposed => _disposed;

        /// <summary>
        /// Releases all resources held by this document. Safe to call multiple times.
        /// </summary>
        public void Dispose()
        {
            Dispose(disposing: true);
            GC.SuppressFinalize(this);
        }

        private void Dispose(bool disposing)
        {
            if (_disposed) return;
            if (disposing)
            {
                _handle.Dispose();
            }
            _disposed = true;
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
#if NET8_0_OR_GREATER
            return Marshal.PtrToStringUTF8(ptr) ?? string.Empty;
#else
            // netstandard2.1: manual null-terminated UTF-8 decode.
            int len = 0;
            while (Marshal.ReadByte(ptr, len) != 0) len++;
            if (len == 0) return string.Empty;
            byte[] bytes = new byte[len];
            Marshal.Copy(ptr, bytes, 0, len);
            return System.Text.Encoding.UTF8.GetString(bytes);
#endif
        }

        private static string? GetLastErrorMessage()
        {
            IntPtr ptr = NativeMethods.pdf_get_last_error();
            if (ptr == IntPtr.Zero)
                return null;
            return MarshalUtf8String(ptr);
        }

        private static PdfluentException ThrowForStatus(PdfStatus status, string fallback)
        {
            string? nativeMsg = GetLastErrorMessage();
            return PdfluentException.FromStatus(status, nativeMsg ?? fallback);
        }

        private static PdfluentException ThrowForLastError(string fallback)
        {
            string? msg = GetLastErrorMessage();
            return new PdfluentException(msg ?? fallback);
        }

        private static async Task<byte[]> ReadAllBytesAsync(string path)
        {
#if NET8_0_OR_GREATER
            return await File.ReadAllBytesAsync(path).ConfigureAwait(false);
#else
            return await Task.Run(() => File.ReadAllBytes(path)).ConfigureAwait(false);
#endif
        }
    }
}
