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
        // Not readonly: in-place mutations (SetFieldValue,
        // AddHighlightAnnotation) swap the native document underneath.
        private PdfDocumentHandle _handle;
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
        /// The document as markdown, in reading order.
        /// </summary>
        /// <remarks>
        /// <para>Preview: output quality is still improving and the API may change in a minor release.</para>
        /// Headings become <c>#</c> to <c>###</c>, list items <c>-</c> and
        /// <c>1.</c>, a table a pipe table or an HTML one where markdown
        /// cannot hold its shape, and a change of page an HTML comment. The
        /// running heads a document repeats are not in it; they are in
        /// <see cref="ExtractBlocks(int, int)"/>.
        /// <para>
        /// The document is read whole whatever the window is, because what a
        /// block IS is decided against the rest of the document: a size is a
        /// heading against what the body is set in, and a line is a running
        /// head because three pages repeat it.
        /// </para>
        /// </remarks>
        /// <param name="firstPage">Zero-based first page of the window.</param>
        /// <param name="pageCount">
        /// Pages from <paramref name="firstPage"/>; negative for the rest of
        /// the document. A window past the last page is an empty string, not
        /// an error.
        /// </param>
        /// <returns>The window as markdown.</returns>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        /// <exception cref="PdfluentException">For engine extraction failures.</exception>
        public string ExtractMarkdown(int firstPage = 0, int pageCount = -1)
        {
            ThrowIfDisposed();
            IntPtr ptr = NativeMethods.pdf_document_extract_markdown(
                _handle.DangerousGetHandle(), firstPage, pageCount);
            if (ptr == IntPtr.Zero)
            {
                throw new PdfluentException(
                    PdfStatus.ErrorExtract,
                    GetLastErrorMessage() ?? "markdown extraction failed");
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

        /// <summary>
        /// The document's structured blocks, in reading order, as JSON.
        /// </summary>
        /// <remarks>
        /// <para>Preview: output quality is still improving and the API may change in a minor release.</para>
        /// <c>{"blocks": [...]}</c>, one entry per paragraph, heading, list
        /// item, table cell and running head, each with its kind, page, box,
        /// text, spans and place in the reading order.
        /// <c>docs/STRUCTURED_EXTRACTION.md</c> is the key to every field.
        /// <para>
        /// Structured results cross the native boundary as JSON, so this
        /// returns a JSON string, exactly as <see cref="TextEditor"/> does and
        /// for the same reason: this package carries no dependencies, and
        /// .NET's own JSON reader is not part of netstandard2.1. Deserialize
        /// it with whichever JSON library the consuming project already uses.
        /// </para>
        /// </remarks>
        /// <param name="firstPage">Zero-based first page of the window.</param>
        /// <param name="pageCount">
        /// Pages from <paramref name="firstPage"/>; negative for the rest of
        /// the document. A window past the last page is an empty
        /// <c>blocks</c> array, not an error.
        /// </param>
        /// <returns>The blocks as JSON.</returns>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        /// <exception cref="PdfluentException">For engine extraction failures.</exception>
        public string ExtractBlocks(int firstPage = 0, int pageCount = -1)
        {
            ThrowIfDisposed();
            IntPtr ptr = NativeMethods.pdf_document_extract_blocks_json(
                _handle.DangerousGetHandle(), firstPage, pageCount);
            if (ptr == IntPtr.Zero)
            {
                throw new PdfluentException(
                    PdfStatus.ErrorExtract,
                    GetLastErrorMessage() ?? "block extraction failed");
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

        /// <summary>
        /// The document as a Docling <c>DoclingDocument</c>, as JSON.
        /// </summary>
        /// <remarks>
        /// <para>Preview: output quality is still improving and the API may change in a minor release.</para>
        /// The schema version the engine pins, which docling-core's
        /// <c>DoclingDocument.model_validate_json</c> loads as it is, so a
        /// pipeline built on Docling can read this document unchanged.
        /// Headings, lists, tables and running heads map onto Docling's
        /// labels; <c>docs/STRUCTURED_EXTRACTION.md</c> lists what has no
        /// mapping.
        /// </remarks>
        /// <param name="firstPage">Zero-based first page of the window.</param>
        /// <param name="pageCount">
        /// Pages from <paramref name="firstPage"/>; negative for the rest of
        /// the document.
        /// </param>
        /// <param name="name">
        /// <c>DoclingDocument.name</c>, which Docling fills with the file's
        /// stem; <c>null</c> means <c>"document"</c>.
        /// </param>
        /// <returns>The window as <c>DoclingDocument</c> JSON.</returns>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        /// <exception cref="PdfluentException">For engine extraction failures.</exception>
        public string ExtractDocling(int firstPage = 0, int pageCount = -1, string? name = null)
        {
            ThrowIfDisposed();
            IntPtr ptr = NativeMethods.pdf_document_extract_docling_json(
                _handle.DangerousGetHandle(), firstPage, pageCount, name);
            if (ptr == IntPtr.Zero)
            {
                throw new PdfluentException(
                    PdfStatus.ErrorExtract,
                    GetLastErrorMessage() ?? "Docling extraction failed");
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

        // ---- Embedded images ----

        /// <summary>
        /// Lists the images stored on one page, as JSON.
        /// </summary>
        /// <remarks>
        /// <c>[{"id":…,"width":…,"height":…,"bits_per_component":…,"components":…,
        /// "color_space":…,"color_space_name":…,"filter":…,"name":…,"has_mask":…}]</c>,
        /// one entry per image XObject in the page's resources, in resource order.
        /// <para>
        /// Structured results cross the native boundary as JSON, so this
        /// returns a JSON string, exactly as <see cref="ExtractBlocks(int,int)"/>
        /// does and for the same reason: this package carries no dependencies,
        /// and .NET's own JSON reader is not part of netstandard2.1. Deserialize
        /// it with whichever JSON library the consuming project already uses.
        /// </para>
        /// </remarks>
        /// <param name="pageIndex">Zero-based page index.</param>
        /// <returns>The page's images as JSON.</returns>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        /// <exception cref="PdfluentException">For engine extraction failures.</exception>
        public string PageImages(int pageIndex)
        {
            ThrowIfDisposed();
            IntPtr ptr = NativeMethods.pdf_page_images_json(
                _handle.DangerousGetHandle(), pageIndex);
            if (ptr == IntPtr.Zero)
            {
                throw new PdfluentException(
                    PdfStatus.ErrorExtract,
                    GetLastErrorMessage() ?? "image listing failed");
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

        /// <summary>
        /// Extracts one stored image: header facts plus encoded bytes.
        /// </summary>
        /// <remarks>
        /// JPEG payloads pass through untouched; everything else arrives decoded
        /// and re-encoded as PNG. The native fields are freed automatically
        /// inside this method; the returned <see cref="ExtractedImage"/> is a
        /// pure managed object.
        /// </remarks>
        /// <param name="imageId">
        /// The <c>id</c> a <see cref="PageImages(int)"/> entry reported.
        /// </param>
        /// <returns>The extracted image.</returns>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        /// <exception cref="PdfluentException">For unknown ids and engine extraction failures.</exception>
        public ExtractedImage ExtractImageData(uint imageId)
        {
            ThrowIfDisposed();
            NativeMethods.PdfExtractedImageNative native;
            PdfStatus rc = NativeMethods.pdf_image_extract_data(
                _handle.DangerousGetHandle(), imageId, out native);
            if (rc != PdfStatus.Ok)
            {
                throw ThrowForStatus(rc, "image extraction failed");
            }
            try
            {
                int len = checked((int)(ulong)native.Len);
                byte[] data = new byte[len];
                if (len > 0)
                {
                    Marshal.Copy(native.Data, data, 0, len);
                }
                return new ExtractedImage(
                    checked((int)native.Width),
                    checked((int)native.Height),
                    checked((int)native.BitsPerComponent),
                    checked((int)native.Components),
                    MarshalUtf8String(native.ColorSpace),
                    MarshalUtf8String(native.ColorSpaceName),
                    MarshalUtf8String(native.Encoding),
                    data,
                    native.Masked != 0,
                    checked((int)native.Xres),
                    checked((int)native.Yres));
            }
            finally
            {
                FreeExtractedImage(ref native);
            }
        }

        /// <summary>
        /// Decodes one stored image to raw samples, top row first.
        /// </summary>
        /// <remarks>
        /// Palettes and stencils are expanded. The native fields are freed
        /// automatically inside this method; the returned
        /// <see cref="DecodedImage"/> is a pure managed object.
        /// </remarks>
        /// <param name="imageId">
        /// The <c>id</c> a <see cref="PageImages(int)"/> entry reported.
        /// </param>
        /// <returns>The decoded image.</returns>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        /// <exception cref="PdfluentException">For unknown ids and engine extraction failures.</exception>
        public DecodedImage DecodeImagePixels(uint imageId)
        {
            ThrowIfDisposed();
            NativeMethods.PdfDecodedImageNative native;
            PdfStatus rc = NativeMethods.pdf_image_decode_pixels(
                _handle.DangerousGetHandle(), imageId, out native);
            if (rc != PdfStatus.Ok)
            {
                throw ThrowForStatus(rc, "image decoding failed");
            }
            try
            {
                int len = checked((int)(ulong)native.Len);
                byte[] pixels = new byte[len];
                if (len > 0)
                {
                    Marshal.Copy(native.Pixels, pixels, 0, len);
                }
                return new DecodedImage(
                    checked((int)native.Width),
                    checked((int)native.Height),
                    checked((int)native.Components),
                    MarshalUtf8String(native.ColorSpace),
                    pixels);
            }
            finally
            {
                FreeDecodedImage(ref native);
            }
        }

        /// <summary>
        /// Frees a <c>PdfExtractedImageNative</c> through its documented free
        /// function. The struct is copied to native memory first: the free
        /// function takes the struct by pointer and reads the heap fields
        /// out of it.
        /// </summary>
        private static void FreeExtractedImage(ref NativeMethods.PdfExtractedImageNative native)
        {
            IntPtr slot = Marshal.AllocHGlobal(Marshal.SizeOf<NativeMethods.PdfExtractedImageNative>());
            try
            {
                Marshal.StructureToPtr(native, slot, false);
                NativeMethods.pdf_extracted_image_free(slot);
            }
            finally
            {
                Marshal.FreeHGlobal(slot);
            }
        }

        /// <summary>
        /// Frees a <c>PdfDecodedImageNative</c> through its documented free
        /// function. See <see cref="FreeExtractedImage"/>.
        /// </summary>
        private static void FreeDecodedImage(ref NativeMethods.PdfDecodedImageNative native)
        {
            IntPtr slot = Marshal.AllocHGlobal(Marshal.SizeOf<NativeMethods.PdfDecodedImageNative>());
            try
            {
                Marshal.StructureToPtr(native, slot, false);
                NativeMethods.pdf_decoded_image_free(slot);
            }
            finally
            {
                Marshal.FreeHGlobal(slot);
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

        // ---- AcroForm fields ----

        /// <summary>
        /// Returns the fully qualified names of every terminal AcroForm field.
        /// </summary>
        /// <returns>
        /// Field names in document order; empty when the document has no
        /// AcroForm. Never <see langword="null"/>.
        /// </returns>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        /// <exception cref="PdfluentException">If the field list cannot be read.</exception>
        public IReadOnlyList<string> GetFormFieldNames()
        {
            ThrowIfDisposed();
            IntPtr docPtr = _handle.DangerousGetHandle();
            int count = NativeMethods.pdf_form_field_count(docPtr);
            if (count < 0)
                throw ThrowForLastError("failed to read form field count");
            if (count == 0)
                return Array.Empty<string>();
            var result = new string[count];
            for (int i = 0; i < count; i++)
            {
                IntPtr ptr = NativeMethods.pdf_form_field_name(docPtr, i);
                if (ptr == IntPtr.Zero)
                    throw ThrowForLastError($"failed to read name of form field {i}");
                try
                {
                    result[i] = MarshalUtf8String(ptr);
                }
                finally
                {
                    NativeMethods.pdf_string_free(ptr);
                }
            }
            return result;
        }

        /// <summary>
        /// Returns every terminal AcroForm field with its type and value.
        /// </summary>
        /// <returns>
        /// Fields in document order; empty when the document has no
        /// AcroForm. Never <see langword="null"/>.
        /// </returns>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        /// <exception cref="PdfluentException">If the field list cannot be read.</exception>
        public IReadOnlyList<FormField> GetFormFields()
        {
            ThrowIfDisposed();
            IntPtr docPtr = _handle.DangerousGetHandle();
            int count = NativeMethods.pdf_form_field_count(docPtr);
            if (count < 0)
                throw ThrowForLastError("failed to read form field count");
            if (count == 0)
                return Array.Empty<FormField>();
            var result = new FormField[count];
            for (int i = 0; i < count; i++)
            {
                IntPtr namePtr = NativeMethods.pdf_form_field_name(docPtr, i);
                if (namePtr == IntPtr.Zero)
                    throw ThrowForLastError($"failed to read name of form field {i}");
                string name;
                try
                {
                    name = MarshalUtf8String(namePtr);
                }
                finally
                {
                    NativeMethods.pdf_string_free(namePtr);
                }
                IntPtr typePtr = NativeMethods.pdf_form_field_type(docPtr, i);
                if (typePtr == IntPtr.Zero)
                    throw ThrowForLastError($"failed to read type of form field {i}");
                string fieldType;
                try
                {
                    fieldType = MarshalUtf8String(typePtr);
                }
                finally
                {
                    NativeMethods.pdf_string_free(typePtr);
                }
                result[i] = new FormField(name, fieldType, GetFieldValue(name));
            }
            return result;
        }

        /// <summary>
        /// Returns the current value of an AcroForm field.
        /// </summary>
        /// <param name="name">Fully qualified field name, as returned by <see cref="GetFormFieldNames"/>.</param>
        /// <returns>
        /// The field value, or <see langword="null"/> when the field does not
        /// exist or carries no value.
        /// </returns>
        /// <exception cref="ArgumentNullException">If <paramref name="name"/> is <see langword="null"/>.</exception>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        public string? GetFieldValue(string name)
        {
            ThrowIfDisposed();
            if (name is null) throw new ArgumentNullException(nameof(name));
            IntPtr ptr = NativeMethods.pdf_form_field_value(
                _handle.DangerousGetHandle(), name);
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

        /// <summary>
        /// Sets the value of an AcroForm field, in place.
        /// </summary>
        /// <remarks>
        /// Text fields take the value as-is. Checkbox fields take
        /// <c>"true"</c>/<c>"false"</c> (also <c>"on"</c>/<c>"off"</c>,
        /// <c>"yes"</c>/<c>"no"</c>, <c>"1"</c>/<c>"0"</c>); radio and choice
        /// fields resolve the value against their on-state names and options.
        /// Persist with <see cref="Save"/>.
        /// </remarks>
        /// <param name="name">Fully qualified field name, as returned by <see cref="GetFormFieldNames"/>.</param>
        /// <param name="value">Value to set.</param>
        /// <exception cref="ArgumentNullException">If <paramref name="name"/> or <paramref name="value"/> is <see langword="null"/>.</exception>
        /// <exception cref="PdfluentValidationException">If the field does not exist, is read-only, or rejects the value.</exception>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        public void SetFieldValue(string name, string value)
        {
            ThrowIfDisposed();
            if (name is null) throw new ArgumentNullException(nameof(name));
            if (value is null) throw new ArgumentNullException(nameof(value));
            PdfStatus status = NativeMethods.pdf_form_set_field_value(
                _handle.DangerousGetHandle(), name, value, out IntPtr updated);
            if (status != PdfStatus.Ok)
                throw ThrowForStatus(status, $"failed to set field '{name}'");
            AdoptNewHandle(updated);
        }

        /// <summary>
        /// Sets the value of an AcroForm field, in place.
        /// </summary>
        /// <remarks>
        /// This is the cross-binding spelling of <see cref="SetFieldValue"/>:
        /// same write, with a <see langword="true"/> receipt on success so
        /// callers can assert the write happened. Persist with
        /// <see cref="Save()"/>.
        /// </remarks>
        /// <param name="name">Fully qualified field name, as returned by <see cref="GetFormFieldNames"/>.</param>
        /// <param name="value">Value to set.</param>
        /// <returns>Always <see langword="true"/>; failures throw.</returns>
        /// <exception cref="ArgumentNullException">If <paramref name="name"/> or <paramref name="value"/> is <see langword="null"/>.</exception>
        /// <exception cref="PdfluentValidationException">If the field does not exist, is read-only, or rejects the value.</exception>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        public bool SetFormField(string name, string value)
        {
            SetFieldValue(name, value);
            return true;
        }

        // ---- Serialisation ----

        /// <summary>
        /// Serialises the document to bytes in its current state.
        /// </summary>
        /// <remarks>
        /// This is how in-place mutations (<see cref="SetFieldValue"/>,
        /// <see cref="AddHighlightAnnotation"/>) are persisted: the returned
        /// bytes reopen to a document that carries them.
        /// </remarks>
        /// <returns>The document as a PDF byte array.</returns>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        /// <exception cref="PdfluentException">If serialisation fails.</exception>
        public byte[] Save()
        {
            ThrowIfDisposed();
            PdfStatus status = NativeMethods.pdf_document_tobytes(
                _handle.DangerousGetHandle(), out IntPtr data, out UIntPtr len);
            if (status != PdfStatus.Ok)
                throw ThrowForStatus(status, "failed to serialise document");
            int size = checked((int)len.ToUInt64());
            try
            {
                byte[] managed = new byte[size];
                if (size > 0)
                {
                    if (data == IntPtr.Zero)
                        throw ThrowForLastError("failed to serialise document");
                    Marshal.Copy(data, managed, 0, size);
                }
                return managed;
            }
            finally
            {
                NativeMethods.pdf_bytes_free(data, len);
            }
        }

        /// <summary>
        /// Serialises the document to a file in its current state.
        /// </summary>
        /// <param name="path">Destination file path; overwritten when it exists.</param>
        /// <exception cref="ArgumentNullException">If <paramref name="path"/> is <see langword="null"/>.</exception>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        /// <exception cref="PdfluentException">If serialisation fails.</exception>
        /// <exception cref="IOException">If the file cannot be written.</exception>
        public void Save(string path)
        {
            if (path is null) throw new ArgumentNullException(nameof(path));
            File.WriteAllBytes(path, Save());
        }

        // ---- Annotations ----

        /// <summary>
        /// Returns the subtype of every annotation on a page.
        /// </summary>
        /// <param name="pageIndex">Zero-based page index.</param>
        /// <returns>
        /// Subtype names (e.g. <c>"Highlight"</c>, <c>"Text"</c>) in document
        /// order; empty when the page carries no annotations. Never
        /// <see langword="null"/>.
        /// </returns>
        /// <exception cref="PdfluentValidationException">If <paramref name="pageIndex"/> is negative.</exception>
        /// <exception cref="PdfluentException">If the annotations cannot be read.</exception>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        public IReadOnlyList<string> GetAnnotationTypes(int pageIndex)
        {
            ThrowIfDisposed();
            if (pageIndex < 0)
                throw new PdfluentValidationException(
                    PdfStatus.ErrorInvalidArgument, $"invalid page index {pageIndex}");
            IntPtr docPtr = _handle.DangerousGetHandle();
            int count = NativeMethods.pdf_annotation_count(docPtr, pageIndex);
            if (count < 0)
                throw ThrowForLastError($"failed to read annotations of page {pageIndex}");
            if (count == 0)
                return Array.Empty<string>();
            var result = new string[count];
            for (int i = 0; i < count; i++)
            {
                IntPtr ptr = NativeMethods.pdf_annotation_type(docPtr, pageIndex, i);
                if (ptr == IntPtr.Zero)
                    throw ThrowForLastError(
                        $"failed to read type of annotation {i} on page {pageIndex}");
                try
                {
                    result[i] = MarshalUtf8String(ptr);
                }
                finally
                {
                    NativeMethods.pdf_string_free(ptr);
                }
            }
            return result;
        }

        /// <summary>
        /// Returns every annotation on a page with its subtype and box.
        /// </summary>
        /// <param name="pageIndex">Zero-based page index.</param>
        /// <returns>
        /// Annotations in document order; empty when the page carries none.
        /// Never <see langword="null"/>.
        /// </returns>
        /// <exception cref="PdfluentValidationException">If <paramref name="pageIndex"/> is negative.</exception>
        /// <exception cref="PdfluentException">If the annotations cannot be read.</exception>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        public IReadOnlyList<Annotation> GetAnnotations(int pageIndex)
        {
            ThrowIfDisposed();
            if (pageIndex < 0)
                throw new PdfluentValidationException(
                    PdfStatus.ErrorInvalidArgument, $"invalid page index {pageIndex}");
            IntPtr docPtr = _handle.DangerousGetHandle();
            int count = NativeMethods.pdf_annotation_count(docPtr, pageIndex);
            if (count < 0)
                throw ThrowForLastError($"failed to read annotations of page {pageIndex}");
            if (count == 0)
                return Array.Empty<Annotation>();
            var result = new Annotation[count];
            for (int i = 0; i < count; i++)
            {
                IntPtr ptr = NativeMethods.pdf_annotation_type(docPtr, pageIndex, i);
                if (ptr == IntPtr.Zero)
                    throw ThrowForLastError(
                        $"failed to read type of annotation {i} on page {pageIndex}");
                string annotType;
                try
                {
                    annotType = MarshalUtf8String(ptr);
                }
                finally
                {
                    NativeMethods.pdf_string_free(ptr);
                }
                PdfStatus status = NativeMethods.pdf_annotation_rect(
                    docPtr, pageIndex, i,
                    out double x0, out double y0, out double x1, out double y1);
                if (status != PdfStatus.Ok)
                    throw ThrowForStatus(status,
                        $"failed to read box of annotation {i} on page {pageIndex}");
                result[i] = new Annotation(annotType, new Rect(x0, y0, x1, y1));
            }
            return result;
        }

        /// <summary>
        /// Adds a yellow highlight annotation to a page, in place.
        /// </summary>
        /// <param name="pageIndex">Zero-based page index.</param>
        /// <param name="x">Left edge in PDF points.</param>
        /// <param name="y">Bottom edge in PDF points (y grows upward).</param>
        /// <param name="width">Width in PDF points.</param>
        /// <param name="height">Height in PDF points.</param>
        /// <exception cref="PdfluentException">If the annotation cannot be added.</exception>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        public void AddHighlightAnnotation(
            int pageIndex, double x, double y, double width, double height)
        {
            ThrowIfDisposed();
            PdfStatus status = NativeMethods.pdf_annotation_add_highlight(
                _handle.DangerousGetHandle(), pageIndex, x, y, width, height,
                out IntPtr updated);
            if (status != PdfStatus.Ok)
                throw ThrowForStatus(status, $"failed to highlight page {pageIndex}");
            AdoptNewHandle(updated);
        }

        /// <summary>
        /// Adds an annotation to a page, in place.
        /// </summary>
        /// <remarks>
        /// This is the cross-binding spelling of the typed add calls.
        /// <paramref name="rect"/> edges map positionally onto the native
        /// box (<c>X0</c>/<c>Y0</c>/<c>X1</c>/<c>Y1</c>); the highlight text
        /// is stored as the annotation contents. Persist with
        /// <see cref="Save()"/>.
        /// </remarks>
        /// <param name="pageIndex">Zero-based page index.</param>
        /// <param name="annotType">Annotation type; currently only <c>"highlight"</c> (case-insensitive).</param>
        /// <param name="rect">Bounding box in PDF user-space points.</param>
        /// <param name="text">Annotation contents.</param>
        /// <exception cref="ArgumentNullException">If <paramref name="annotType"/> is <see langword="null"/>.</exception>
        /// <exception cref="PdfluentValidationException">If <paramref name="annotType"/> names an unsupported annotation.</exception>
        /// <exception cref="PdfluentException">If the annotation cannot be added.</exception>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        public void AddAnnotation(int pageIndex, string annotType, Rect rect, string text)
        {
            ThrowIfDisposed();
            if (annotType is null) throw new ArgumentNullException(nameof(annotType));
            if (!annotType.Equals("highlight", StringComparison.OrdinalIgnoreCase))
                throw new PdfluentValidationException(
                    PdfStatus.ErrorInvalidArgument,
                    $"unsupported annotation type '{annotType}'; use 'highlight'");
            PdfStatus status = NativeMethods.pdf_annotation_add_highlight_with_contents(
                _handle.DangerousGetHandle(), pageIndex,
                rect.X0, rect.Y0, rect.X1 - rect.X0, rect.Y1 - rect.Y0,
                text,
                out IntPtr updated);
            if (status != PdfStatus.Ok)
                throw ThrowForStatus(status, $"failed to annotate page {pageIndex}");
            AdoptNewHandle(updated);
        }

        // ---- PDF/A validation ----

        /// <summary>
        /// Validates the document against a PDF/A conformance level.
        /// </summary>
        /// <param name="level">Conformance level to validate against.</param>
        /// <returns>A <see cref="PdfAReport"/> that is never <see langword="null"/>.</returns>
        /// <exception cref="PdfluentValidationException">If <paramref name="level"/> is not a defined <see cref="PdfALevel"/> value.</exception>
        /// <exception cref="PdfluentException">If validation fails to run.</exception>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        public PdfAReport ValidatePdfA(PdfALevel level)
        {
            ThrowIfDisposed();
            if (level < PdfALevel.A1b || level > PdfALevel.A4e)
                throw new PdfluentValidationException(
                    PdfStatus.ErrorInvalidArgument, $"unknown PDF/A level {(int)level}");
            PdfStatus status = NativeMethods.pdf_document_validate_pdfa(
                _handle.DangerousGetHandle(), (int)level, out IntPtr report);
            if (status != PdfStatus.Ok)
                throw ThrowForStatus(status, $"PDF/A validation ({level}) failed to run");
            try
            {
                bool compliant =
                    NativeMethods.pdf_compliance_report_is_compliant(report) == 1;
                int errors =
                    NativeMethods.pdf_compliance_report_error_count(report);
                int warnings =
                    NativeMethods.pdf_compliance_report_warning_count(report);
                int issueCount =
                    NativeMethods.pdf_compliance_report_issue_count(report);
                int total = issueCount < 0 ? 0 : issueCount;
                var issues = new List<string>(total);
                for (int i = 0; i < total; i++)
                {
                    IntPtr ptr = NativeMethods.pdf_compliance_report_issue_summary(report, i);
                    if (ptr == IntPtr.Zero)
                        throw ThrowForLastError($"failed to read PDF/A issue {i}");
                    try
                    {
                        issues.Add(MarshalUtf8String(ptr));
                    }
                    finally
                    {
                        NativeMethods.pdf_string_free(ptr);
                    }
                }
                return new PdfAReport(
                    compliant,
                    errors < 0 ? 0 : errors,
                    warnings < 0 ? 0 : warnings,
                    issues);
            }
            finally
            {
                NativeMethods.pdf_compliance_report_free(report);
            }
        }

        /// <summary>
        /// Validates PDF bytes against PDF/A-2b conformance.
        /// </summary>
        /// <remarks>
        /// This is the cross-binding spelling of the instance
        /// <see cref="ValidatePdfA(PdfALevel)"/> checks: it opens
        /// <paramref name="data"/> and validates at PDF/A-2b, the same
        /// default the Python binding falls back to when the document
        /// declares no level.
        /// </remarks>
        /// <param name="data">PDF bytes to validate.</param>
        /// <returns>A <see cref="PdfAReport"/> that is never <see langword="null"/>.</returns>
        /// <exception cref="ArgumentNullException">If <paramref name="data"/> is <see langword="null"/>.</exception>
        /// <exception cref="PdfluentParseException">If <paramref name="data"/> is not a valid PDF.</exception>
        /// <exception cref="PdfluentException">If validation fails to run.</exception>
        public static PdfAReport ValidatePdfA(byte[] data)
        {
            if (data is null) throw new ArgumentNullException(nameof(data));
            using var doc = Open(data);
            return doc.ValidatePdfA(PdfALevel.A2b);
        }

        /// <summary>
        /// Validates the document against a PDF/A conformance level.
        /// </summary>
        /// <param name="level">
        /// Short level name: <c>"1b"</c>, <c>"1a"</c>, <c>"2b"</c>,
        /// <c>"2u"</c>, <c>"2a"</c>, <c>"3b"</c>, <c>"3u"</c>, <c>"3a"</c>,
        /// <c>"4"</c>, <c>"4f"</c> or <c>"4e"</c> (case-insensitive; an
        /// optional <c>"pdfa"</c> or <c>"pdf/a"</c> prefix and <c>"-"</c>
        /// separators are ignored, so <c>"PDF/A-2b"</c> works too).
        /// </param>
        /// <returns>A <see cref="PdfAReport"/> that is never <see langword="null"/>.</returns>
        /// <exception cref="ArgumentNullException">If <paramref name="level"/> is <see langword="null"/>.</exception>
        /// <exception cref="PdfluentValidationException">If <paramref name="level"/> names no known level.</exception>
        /// <exception cref="PdfluentException">If validation fails to run.</exception>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        public PdfAReport ValidatePdfA(string level)
        {
            if (level is null) throw new ArgumentNullException(nameof(level));
            return ValidatePdfA(ParsePdfALevel(level));
        }

        // ---- Merge ----

        /// <summary>
        /// Merges two PDFs into one, concatenating their pages.
        /// </summary>
        /// <param name="first">First PDF bytes.</param>
        /// <param name="second">Second PDF bytes.</param>
        /// <returns>The merged PDF bytes.</returns>
        /// <exception cref="ArgumentNullException">If <paramref name="first"/> or <paramref name="second"/> is <see langword="null"/>.</exception>
        /// <exception cref="PdfluentException">If either input is not a valid PDF, or the merge fails.</exception>
        public static byte[] Merge(byte[] first, byte[] second)
        {
            if (first is null) throw new ArgumentNullException(nameof(first));
            if (second is null) throw new ArgumentNullException(nameof(second));
            using var docA = Open(first);
            using var docB = Open(second);
            IntPtr[] docs =
            {
                docA._handle.DangerousGetHandle(),
                docB._handle.DangerousGetHandle(),
            };
            PdfStatus status = NativeMethods.pdf_documents_merge(
                docs, docs.Length, out IntPtr merged);
            if (status != PdfStatus.Ok)
                throw ThrowForStatus(status, "failed to merge documents");
            try
            {
                using var mergedDoc = new PdfDocument(new PdfDocumentHandle(merged));
                merged = IntPtr.Zero; // ownership moved to mergedDoc
                return mergedDoc.Save();
            }
            finally
            {
                if (merged != IntPtr.Zero)
                    NativeMethods.pdf_document_free(merged);
            }
        }

        // ---- Image extraction ----

        /// <summary>
        /// Extracts the images embedded on a page.
        /// </summary>
        /// <param name="pageIndex">Zero-based page index.</param>
        /// <returns>
        /// The page's images in document order; empty when the page embeds
        /// none. Never <see langword="null"/>.
        /// </returns>
        /// <exception cref="PdfluentValidationException">If <paramref name="pageIndex"/> is negative.</exception>
        /// <exception cref="PdfluentException">If extraction fails.</exception>
        /// <exception cref="ObjectDisposedException">If the document has been disposed.</exception>
        public IReadOnlyList<RawImage> ExtractImages(int pageIndex)
        {
            ThrowIfDisposed();
            if (pageIndex < 0)
                throw new PdfluentValidationException(
                    PdfStatus.ErrorInvalidArgument, $"invalid page index {pageIndex}");
            IntPtr docPtr = _handle.DangerousGetHandle();
            int count = NativeMethods.pdf_page_image_count(docPtr, pageIndex);
            if (count < 0)
                throw ThrowForLastError($"failed to count images on page {pageIndex}");
            if (count == 0)
                return Array.Empty<RawImage>();
            var result = new RawImage[count];
            for (int i = 0; i < count; i++)
            {
                PdfStatus status = NativeMethods.pdf_page_extract_image(
                    docPtr, pageIndex, i,
                    out uint width, out uint height,
                    out IntPtr data, out UIntPtr len);
                if (status != PdfStatus.Ok)
                    throw ThrowForStatus(status,
                        $"failed to extract image {i} on page {pageIndex}");
                int size = checked((int)len.ToUInt64());
                try
                {
                    byte[] managed = new byte[size];
                    if (size > 0)
                    {
                        if (data == IntPtr.Zero)
                            throw ThrowForLastError(
                                $"failed to extract image {i} on page {pageIndex}");
                        Marshal.Copy(data, managed, 0, size);
                    }
                    result[i] = new RawImage(
                        checked((int)width), checked((int)height), managed);
                }
                finally
                {
                    NativeMethods.pdf_bytes_free(data, len);
                }
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

        // In-place mutations receive a NEW native document from the engine;
        // retire the old handle and carry on under the new one, so the
        // caller's PdfDocument instance keeps working unchanged.
        private void AdoptNewHandle(IntPtr updated)
        {
            var fresh = new PdfDocumentHandle(updated);
            _handle.Dispose();
            _handle = fresh;
        }

        private static PdfALevel ParsePdfALevel(string level)
        {
            string key = level.Trim().ToLowerInvariant()
                .Replace("pdf/a", string.Empty)
                .Replace("pdfa", string.Empty)
                .Replace("-", string.Empty)
                .Replace("_", string.Empty)
                .Replace(" ", string.Empty);
            return key switch
            {
                "1b" => PdfALevel.A1b,
                "1a" => PdfALevel.A1a,
                "2b" => PdfALevel.A2b,
                "2u" => PdfALevel.A2u,
                "2a" => PdfALevel.A2a,
                "3b" => PdfALevel.A3b,
                "3u" => PdfALevel.A3u,
                "3a" => PdfALevel.A3a,
                "4" => PdfALevel.A4,
                "4f" => PdfALevel.A4f,
                "4e" => PdfALevel.A4e,
                _ => throw new PdfluentValidationException(
                    PdfStatus.ErrorInvalidArgument,
                    $"unknown PDF/A level '{level}': want one of 1b, 1a, 2b, 2u, 2a, 3b, 3u, 3a, 4, 4f, 4e"),
            };
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
