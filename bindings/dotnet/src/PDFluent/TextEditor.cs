using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Threading.Tasks;

namespace PDFluent
{
    /// <summary>
    /// Finds and replaces text in a PDF while preserving fonts, positioning and
    /// the surrounding page content.
    /// </summary>
    /// <remarks>
    /// <para>
    /// Matches are addressed individually: <see cref="FindText"/> returns an
    /// opaque id per occurrence, so the second of two identical headers can be
    /// replaced without touching the first. Ids survive being written to a queue
    /// or a file, which is what makes the find-now/apply-later workflow work —
    /// send the found text out for translation, then apply the results by id.
    /// </para>
    /// <para>
    /// Structured results cross the native boundary as JSON, so this type
    /// returns JSON strings. Deserialize them with whichever JSON library the
    /// consuming project already uses.
    /// </para>
    /// <para>
    /// Trial-tier edits stamp a small "PDFluent trial" notice on each modified
    /// page; licensed tiers edit without it. Searching never modifies anything.
    /// </para>
    /// </remarks>
    /// <example>
    /// <code>
    /// using var editor = TextEditor.Open(File.ReadAllBytes("contract.pdf"));
    /// string matches = editor.FindText("{\"text\": \"Acme B.V.\"}");
    /// // …pick an id out of the JSON, translate the text, then:
    /// string report = editor.ReplaceMatches(
    ///     "[{\"id\": \"" + id + "\", \"text\": \"Example B.V.\"}]");
    /// File.WriteAllBytes("out.pdf", editor.ToBytes());
    /// </code>
    /// </example>
    public sealed class TextEditor : IDisposable
    {
        private IntPtr _handle;
        private bool _disposed;

        private TextEditor(IntPtr handle)
        {
            _handle = handle;
        }

        /// <summary>Opens a PDF from raw bytes for text editing.</summary>
        /// <param name="data">The PDF file contents.</param>
        /// <returns>A new <see cref="TextEditor"/>.</returns>
        /// <exception cref="ArgumentNullException">If <paramref name="data"/> is null.</exception>
        /// <exception cref="PdfluentException">If the PDF cannot be opened.</exception>
        public static TextEditor Open(byte[] data)
        {
            if (data is null)
                throw new ArgumentNullException(nameof(data));

            PdfStatus status = NativeMethods.pdf_text_editor_open_from_bytes(
                data, (UIntPtr)data.Length, out IntPtr handle);
            if (status != PdfStatus.Ok || handle == IntPtr.Zero)
                throw ThrowForStatus(status, "cannot open PDF for text editing");

            return new TextEditor(handle);
        }

        /// <summary>Number of pages in the document.</summary>
        public int PageCount
        {
            get
            {
                ThrowIfDisposed();
                return (int)NativeMethods.pdf_text_editor_page_count(_handle);
            }
        }

        /// <summary>
        /// Finds text occurrences. Returns a JSON array of matches.
        /// </summary>
        /// <param name="queryJson">
        /// A JSON object: <c>{"text": "…", "caseInsensitive": false,
        /// "pages": [1, 5], "region": {"page": 1, "rect": [x0, y0, x1, y1],
        /// "relation": "intersects"}, "limit": 100}</c>. Only <c>text</c> is
        /// required.
        /// </param>
        /// <returns>
        /// JSON array. Each match carries an opaque <c>id</c>, valid until the
        /// next applied edit, and an <c>editable</c> flag — matches inside Form
        /// XObjects, style-mixed spans or <c>/ActualText</c> regions are
        /// reported rather than silently omitted.
        /// </returns>
        /// <exception cref="PdfluentException">If the query is malformed or the search fails.</exception>
        public string FindText(string queryJson)
        {
            ThrowIfDisposed();
            if (queryJson is null)
                throw new ArgumentNullException(nameof(queryJson));

            PdfStatus status = NativeMethods.pdf_text_editor_find(
                _handle, queryJson, out IntPtr ptr);
            return TakeJsonResult(status, ptr, "text search failed");
        }

        /// <summary>
        /// Finds and replaces in one call. Returns the JSON replacement report.
        /// </summary>
        /// <param name="queryJson">See <see cref="FindText"/>.</param>
        /// <param name="replacement">The replacement text.</param>
        /// <param name="optionsJson">
        /// Optional JSON: <c>{"fontFallback": "deny" | "injectStandard" |
        /// {"explicit": "F1"}, "signaturePolicy": "reject" |
        /// "allowPostSignatureChange", "commitPolicy": "allOrNothing" |
        /// "bestEffort"}</c>. Pass <c>"{}"</c> for defaults.
        /// </param>
        /// <returns>
        /// JSON report. Every occurrence found is accounted for — applied, or
        /// failed with a reason. Nothing is skipped silently.
        /// </returns>
        /// <exception cref="PdfluentException">If the replacement fails.</exception>
        public string ReplaceText(string queryJson, string replacement, string optionsJson = "{}")
        {
            ThrowIfDisposed();
            if (queryJson is null)
                throw new ArgumentNullException(nameof(queryJson));
            if (replacement is null)
                throw new ArgumentNullException(nameof(replacement));

            PdfStatus status = NativeMethods.pdf_text_editor_replace(
                _handle, queryJson, replacement, optionsJson ?? "{}", out IntPtr ptr);
            return TakeJsonResult(status, ptr, "text replacement failed");
        }

        /// <summary>
        /// Applies replacements to matches located earlier with <see cref="FindText"/>.
        /// </summary>
        /// <param name="editsJson">
        /// A JSON array of <c>{"id": "…", "text": "…"}</c>. The edits commit as
        /// one transaction; by default any invalid edit aborts the whole batch
        /// and the document stays untouched.
        /// </param>
        /// <param name="optionsJson">See <see cref="ReplaceText"/>.</param>
        /// <returns>JSON report.</returns>
        /// <exception cref="PdfluentException">If the transaction fails.</exception>
        public string ReplaceMatches(string editsJson, string optionsJson = "{}")
        {
            ThrowIfDisposed();
            if (editsJson is null)
                throw new ArgumentNullException(nameof(editsJson));

            PdfStatus status = NativeMethods.pdf_text_editor_replace_matches(
                _handle, editsJson, optionsJson ?? "{}", out IntPtr ptr);
            return TakeJsonResult(status, ptr, "text replacement failed");
        }

        /// <summary>Applies replacements asynchronously.</summary>
        /// <param name="editsJson">See <see cref="ReplaceMatches"/>.</param>
        /// <param name="optionsJson">See <see cref="ReplaceText"/>.</param>
        /// <returns>A task whose result is the JSON report.</returns>
        public Task<string> ReplaceMatchesAsync(string editsJson, string optionsJson = "{}")
        {
            return Task.Run(() => ReplaceMatches(editsJson, optionsJson));
        }

        /// <summary>Serializes the (possibly edited) document.</summary>
        /// <returns>The PDF file contents.</returns>
        /// <exception cref="PdfluentException">If serialization fails.</exception>
        public byte[] ToBytes()
        {
            ThrowIfDisposed();
            PdfStatus status = NativeMethods.pdf_text_editor_to_bytes(
                _handle, out IntPtr data, out UIntPtr len);
            if (status != PdfStatus.Ok || data == IntPtr.Zero)
                throw ThrowForStatus(status, "cannot serialize document");

            try
            {
                var bytes = new byte[(int)len];
                Marshal.Copy(data, bytes, 0, bytes.Length);
                return bytes;
            }
            finally
            {
                NativeMethods.pdf_text_editor_bytes_free(data, len);
            }
        }

        /// <summary>Releases the native editor handle.</summary>
        public void Dispose()
        {
            if (_disposed)
                return;

            if (_handle != IntPtr.Zero)
            {
                NativeMethods.pdf_text_editor_free(_handle);
                _handle = IntPtr.Zero;
            }
            _disposed = true;
            GC.SuppressFinalize(this);
        }

        /// <summary>Releases the native editor handle if Dispose was not called.</summary>
        ~TextEditor()
        {
            if (_handle != IntPtr.Zero)
            {
                NativeMethods.pdf_text_editor_free(_handle);
                _handle = IntPtr.Zero;
            }
        }

        // ---- Internals ----

        private static string TakeJsonResult(PdfStatus status, IntPtr ptr, string fallback)
        {
            if (status != PdfStatus.Ok || ptr == IntPtr.Zero)
                throw ThrowForStatus(status, fallback);

            try
            {
                return MarshalUtf8String(ptr);
            }
            finally
            {
                NativeMethods.pdf_string_free(ptr);
            }
        }

        private static string MarshalUtf8String(IntPtr ptr)
        {
#if NET8_0_OR_GREATER
            return Marshal.PtrToStringUTF8(ptr) ?? string.Empty;
#else
            int len = 0;
            while (Marshal.ReadByte(ptr, len) != 0) len++;
            if (len == 0) return string.Empty;
            byte[] bytes = new byte[len];
            Marshal.Copy(ptr, bytes, 0, len);
            return System.Text.Encoding.UTF8.GetString(bytes);
#endif
        }

        private static PdfluentException ThrowForStatus(PdfStatus status, string fallback)
        {
            IntPtr errPtr = NativeMethods.pdf_get_last_error();
            string? nativeMsg = errPtr == IntPtr.Zero ? null : MarshalUtf8String(errPtr);
            return PdfluentException.FromStatus(status, nativeMsg ?? fallback);
        }

        private void ThrowIfDisposed()
        {
            if (_disposed)
                throw new ObjectDisposedException(nameof(TextEditor));
        }
    }
}
