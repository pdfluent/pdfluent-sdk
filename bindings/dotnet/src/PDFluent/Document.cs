// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;

namespace PDFluent
{

    /// <summary>
    /// A compatible document: open, save, outline, and object access. Pages are addressed through Page values; the document owns the native handle and must be closed.
    /// </summary>
    public sealed class Document : IDisposable
    {
        private IntPtr _handle;

        /// <summary>
        /// Open a document from a file.
        /// </summary>
        /// <param name="filename">Path to the file.</param>
        public Document(string? filename)
            : this(OpenHandle(filename, null, null, null, 0.0, 0.0, 11.0, null))
        {
        }

        /// <summary>
        /// Open a document from bytes.
        /// </summary>
        /// <param name="data">File bytes.</param>
        public Document(byte[]? data)
            : this(OpenHandle(null, data, null, null, 0.0, 0.0, 11.0, null))
        {
        }

        /// <summary>
        /// Open a document from bytes with a file-type hint.
        /// </summary>
        /// <param name="data">File bytes.</param>
        /// <param name="filetype">File-type hint.</param>
        public Document(byte[]? data, string? filetype)
            : this(OpenHandle(null, data, filetype, null, 0.0, 0.0, 11.0, null))
        {
        }

        /// <summary>
        /// Open a fresh empty shell.
        /// </summary>
        public Document()
            : this(OpenHandle(null, null, null, null, 0.0, 0.0, 11.0, null))
        {
        }

        /// <summary>
        /// Open a document with full options.
        /// </summary>
        /// <param name="options">Open options.</param>
        public Document(DocumentOptions options)
            : this(OpenHandle(options.Filename, options.Data, options.Filetype,
                options.Rect, options.Width, options.Height, options.Fontsize,
                options.ArchiveJson))
        {
        }

        internal Document(IntPtr handle)
        {
            _handle = handle;
        }

        private static unsafe IntPtr OpenHandle(string? filename, byte[]? data,
            string? filetype, Rect? rect, double width, double height,
            double fontsize, string? archive)
        {
            Rect rectTmp = rect.GetValueOrDefault();
            Rect* rectPtr = rect.HasValue ? &rectTmp : null;
            fixed (byte* dataPtr = data)
            {
                CompatSupport.Check(NativeMethods.pdf_compat_document_open(filename,
                    dataPtr, CompatSupport.Count(data), filetype, rectPtr, width,
                    height, fontsize, archive, out IntPtr hPtr), "Open");
                return hPtr;
            }
        }

        /// <summary>
        /// Close the document. Idempotent.
        /// </summary>
        public void Close()
        {
            Dispose();
        }


        /// <summary>Unlock with a password.</summary>
        public int Authenticate(string? password)
        {
            int outValue;
            CompatSupport.Check(NativeMethods.pdf_compat_document_authenticate(RequireOpen(), password, out outValue), "Authenticate");
            return outValue;
        }

        /// <summary>Flatten annotations and widgets into content.</summary>
        public void Bake(bool annots, bool widgets)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_document_bake(RequireOpen(), CompatSupport.Flag(annots), CompatSupport.Flag(widgets)), "Bake");
        }

        /// <summary>Convenience overload with defaults.</summary>
        public void Bake()
        {
            Bake(true, true);
        }

        /// <summary>Convert a page span to PDF content.</summary>
        public byte[] ConvertToPdf(int from_page, int to_page, int rotate)
        {
            IntPtr dataPtr;
            UIntPtr dataLen;
            CompatSupport.Check(NativeMethods.pdf_compat_document_convert_to_pdf(RequireOpen(), from_page, to_page, rotate, out dataPtr, out dataLen), "ConvertToPdf");
            return CompatSupport.ReadBytes(dataPtr, dataLen);
        }

        /// <summary>Convenience overload with defaults.</summary>
        public byte[] ConvertToPdf()
        {
            return ConvertToPdf(0, -1, 0);
        }

        /// <summary>Font bytes and facts.</summary>
        public FontData ExtractFont(int xref, bool info_only, string? name)
        {
            IntPtr hPtr;
            CompatSupport.Check(NativeMethods.pdf_compat_document_extract_font(RequireOpen(), xref, CompatSupport.Flag(info_only), name, out hPtr), "ExtractFont");
            try
            {
                return FontData.Read(hPtr);
            }
            finally
            {
                NativeMethods.pdf_compat_font_data_free(hPtr);
            }
        }

        /// <summary>Convenience overload with defaults.</summary>
        public FontData ExtractFont(int xref)
        {
            return ExtractFont(xref, false, null);
        }

        /// <summary>Image bytes and facts for an object number.</summary>
        public ImageData ExtractImage(int xref)
        {
            IntPtr hPtr;
            CompatSupport.Check(NativeMethods.pdf_compat_document_extract_image(RequireOpen(), xref, out hPtr), "ExtractImage");
            try
            {
                return ImageData.Read(hPtr);
            }
            finally
            {
                NativeMethods.pdf_compat_image_data_free(hPtr);
            }
        }

        /// <summary>Reserve a fresh object number.</summary>
        public int GetNewXref()
        {
            int outValue;
            CompatSupport.Check(NativeMethods.pdf_compat_document_get_new_xref(RequireOpen(), out outValue), "GetNewXref");
            return outValue;
        }

        /// <summary>Fonts referenced by a page.</summary>
        public List<FontInfo> GetPageFonts(int page, bool full)
        {
            IntPtr listPtr;
            CompatSupport.Check(NativeMethods.pdf_compat_document_get_page_fonts(RequireOpen(), page, CompatSupport.Flag(full), out listPtr), "GetPageFonts");
            try
            {
                return FontInfo.ReadList(listPtr);
            }
            finally
            {
                NativeMethods.pdf_compat_font_info_list_free(listPtr);
            }
        }

        /// <summary>Convenience overload with defaults.</summary>
        public List<FontInfo> GetPageFonts(int page)
        {
            return GetPageFonts(page, false);
        }

        /// <summary>Images referenced by a page.</summary>
        public List<ImageRef> GetPageImages(int page, bool full)
        {
            IntPtr listPtr;
            CompatSupport.Check(NativeMethods.pdf_compat_document_get_page_images(RequireOpen(), page, CompatSupport.Flag(full), out listPtr), "GetPageImages");
            try
            {
                return ImageRef.ReadList(listPtr);
            }
            finally
            {
                NativeMethods.pdf_compat_image_ref_list_free(listPtr);
            }
        }

        /// <summary>Convenience overload with defaults.</summary>
        public List<ImageRef> GetPageImages(int page)
        {
            return GetPageImages(page, false);
        }

        /// <summary>Outline entries.</summary>
        public List<TocEntry> GetToc(bool simple)
        {
            IntPtr listPtr;
            CompatSupport.Check(NativeMethods.pdf_compat_document_get_toc(RequireOpen(), CompatSupport.Flag(simple), out listPtr), "GetToc");
            try
            {
                return TocEntry.ReadList(listPtr);
            }
            finally
            {
                NativeMethods.pdf_compat_toc_list_free(listPtr);
            }
        }

        /// <summary>Convenience overload with defaults.</summary>
        public List<TocEntry> GetToc()
        {
            return GetToc(true);
        }

        /// <summary>Convenience overload with defaults.</summary>
        public void InsertPdf(Document source)
        {
            InsertPdf(source, new InsertPdfOptions());
        }

        /// <summary>Whether this is a form PDF.</summary>
        public bool IsFormPdf()
        {
            int outValue;
            CompatSupport.Check(NativeMethods.pdf_compat_document_is_form_pdf(RequireOpen(), out outValue), "IsFormPdf");
            return outValue != 0;
        }

        /// <summary>Whether this is a PDF document.</summary>
        public bool IsPdf()
        {
            int outValue;
            CompatSupport.Check(NativeMethods.pdf_compat_document_is_pdf(RequireOpen(), out outValue), "IsPdf");
            return outValue != 0;
        }

        /// <summary>Undo/redo availability of the compatible journal.</summary>
        public (bool Undo, bool Redo) JournalCanDo()
        {
            int canUndo, canRedo;
            CompatSupport.Check(NativeMethods.pdf_compat_document_journal_can_do(RequireOpen(), out canUndo, out canRedo), "JournalCanDo");
            return (canUndo != 0, canRedo != 0);
        }

        /// <summary>Enable the compatible journal.</summary>
        public void JournalEnable()
        {
            CompatSupport.Check(NativeMethods.pdf_compat_document_journal_enable(RequireOpen()), "JournalEnable");
        }

        /// <summary>Whether the compatible journal is enabled.</summary>
        public bool JournalIsEnabled()
        {
            int outValue;
            CompatSupport.Check(NativeMethods.pdf_compat_document_journal_is_enabled(RequireOpen(), out outValue), "JournalIsEnabled");
            return outValue != 0;
        }

        /// <summary>Load a journal file over a never-enabled journal.</summary>
        public void JournalLoad(string? path)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_document_journal_load(RequireOpen(), path), "JournalLoad");
        }

        /// <summary>Journal step name.</summary>
        public string JournalOpName(int step)
        {
            IntPtr sPtr;
            CompatSupport.Check(NativeMethods.pdf_compat_document_journal_op_name(RequireOpen(), step, out sPtr), "JournalOpName");
            return CompatSupport.ReadString(sPtr);
        }

        /// <summary>Journal position: the applied prefix length.</summary>
        public int JournalPosition()
        {
            int outValue;
            CompatSupport.Check(NativeMethods.pdf_compat_document_journal_position(RequireOpen(), out outValue), "JournalPosition");
            return outValue;
        }

        /// <summary>Redo a journal step.</summary>
        public void JournalRedo()
        {
            CompatSupport.Check(NativeMethods.pdf_compat_document_journal_redo(RequireOpen()), "JournalRedo");
        }

        /// <summary>Save the journal file.</summary>
        public void JournalSave(string? path)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_document_journal_save(RequireOpen(), path), "JournalSave");
        }

        /// <summary>Start a journal step.</summary>
        public void JournalStartOp(string? name)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_document_journal_start_op(RequireOpen(), name), "JournalStartOp");
        }

        /// <summary>Stop the journal step.</summary>
        public void JournalStopOp()
        {
            CompatSupport.Check(NativeMethods.pdf_compat_document_journal_stop_op(RequireOpen()), "JournalStopOp");
        }

        /// <summary>Undo a journal step.</summary>
        public void JournalUndo()
        {
            CompatSupport.Check(NativeMethods.pdf_compat_document_journal_undo(RequireOpen()), "JournalUndo");
        }

        /// <summary>Whether the document needs a password.</summary>
        public int NeedsPass()
        {
            int outValue;
            CompatSupport.Check(NativeMethods.pdf_compat_document_needs_pass(RequireOpen(), out outValue), "NeedsPass");
            return outValue;
        }

        /// <summary>Convenience overload with defaults.</summary>
        public Page NewPage()
        {
            return NewPage(-1, 595.0, 842.0);
        }

        /// <summary>Number of pages. Binds PdfDocument::len; the spec names no page_count (reported).</summary>
        public int PageCount()
        {
            int outValue;
            CompatSupport.Check(NativeMethods.pdf_compat_document_page_count(RequireOpen(), out outValue), "PageCount");
            return outValue;
        }

        /// <summary>The document catalog.</summary>
        public string PdfCatalog()
        {
            IntPtr sPtr;
            CompatSupport.Check(NativeMethods.pdf_compat_document_pdf_catalog(RequireOpen(), out sPtr), "PdfCatalog");
            return CompatSupport.ReadString(sPtr);
        }

        /// <summary>Save to a file.</summary>
        public void Save(string? path, SaveOptions? settings)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_document_save(RequireOpen(), path, settings == null ? null : settings.ToJson()), "Save");
        }

        /// <summary>Convenience overload with defaults.</summary>
        public void Save(string? path)
        {
            Save(path, null);
        }

        /// <summary>Save incrementally.</summary>
        public void SaveIncr()
        {
            CompatSupport.Check(NativeMethods.pdf_compat_document_save_incr(RequireOpen()), "SaveIncr");
        }

        /// <summary>Convenience overload with defaults.</summary>
        public int SetToc(List<TocEntry> entries)
        {
            return SetToc(entries, 1);
        }

        /// <summary>Serialise the document to bytes.</summary>
        public byte[] Tobytes(SaveOptions? settings)
        {
            IntPtr dataPtr;
            UIntPtr dataLen;
            CompatSupport.Check(NativeMethods.pdf_compat_document_tobytes(RequireOpen(), settings == null ? null : settings.ToJson(), out dataPtr, out dataLen), "Tobytes");
            return CompatSupport.ReadBytes(dataPtr, dataLen);
        }

        /// <summary>Convenience overload with defaults.</summary>
        public byte[] Tobytes()
        {
            return Tobytes(null);
        }

        /// <summary>Replace one raw object.</summary>
        public void UpdateObject(int xref, string? text, int? page)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_document_update_object(RequireOpen(), xref, text, page ?? -1), "UpdateObject");
        }

        /// <summary>Convenience overload with defaults.</summary>
        public void UpdateObject(int xref, string? text)
        {
            UpdateObject(xref, text, null);
        }

        /// <summary>Replace one stream payload.</summary>
        public void UpdateStream(int xref, byte[]? data, bool is_new, bool compress)
        {
            unsafe
            {
                fixed (byte* dataPtr = data)
                {
                CompatSupport.Check(NativeMethods.pdf_compat_document_update_stream(RequireOpen(), xref, dataPtr, CompatSupport.Count(data), CompatSupport.Flag(is_new), CompatSupport.Flag(compress)), "UpdateStream");
                }
            }
        }

        /// <summary>Convenience overload with defaults.</summary>
        public void UpdateStream(int xref)
        {
            UpdateStream(xref, null, true, true);
        }

        /// <summary>Write the document to bytes.</summary>
        public byte[] Write(SaveOptions? settings)
        {
            IntPtr dataPtr;
            UIntPtr dataLen;
            CompatSupport.Check(NativeMethods.pdf_compat_document_write(RequireOpen(), settings == null ? null : settings.ToJson(), out dataPtr, out dataLen), "Write");
            return CompatSupport.ReadBytes(dataPtr, dataLen);
        }

        /// <summary>Convenience overload with defaults.</summary>
        public byte[] Write()
        {
            return Write(null);
        }

        /// <summary>Object count.</summary>
        public int XrefLength()
        {
            int outValue;
            CompatSupport.Check(NativeMethods.pdf_compat_document_xref_length(RequireOpen(), out outValue), "XrefLength");
            return outValue;
        }

        /// <summary>One raw object.</summary>
        public string XrefObject(int xref, bool compressed, bool ascii)
        {
            IntPtr sPtr;
            CompatSupport.Check(NativeMethods.pdf_compat_document_xref_object(RequireOpen(), xref, CompatSupport.Flag(compressed), CompatSupport.Flag(ascii), out sPtr), "XrefObject");
            return CompatSupport.ReadString(sPtr);
        }

        /// <summary>Convenience overload with defaults.</summary>
        public string XrefObject(int xref)
        {
            return XrefObject(xref, false, false);
        }

        /// <summary>Set one object key.</summary>
        public void XrefSetKey(int xref, string? key, string? value)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_document_xref_set_key(RequireOpen(), xref, key, value), "XrefSetKey");
        }

        /// <summary>One stream payload.</summary>
        public byte[] XrefStream(int xref)
        {
            IntPtr dataPtr;
            UIntPtr dataLen;
            CompatSupport.Check(NativeMethods.pdf_compat_document_xref_stream(RequireOpen(), xref, out dataPtr, out dataLen), "XrefStream");
            return CompatSupport.ReadBytes(dataPtr, dataLen);
        }

        /// <summary>
        /// Address a page by number.
        /// </summary>
        /// <param name="page">0-based page number.</param>
        /// <returns>The page.</returns>
        public Page LoadPage(int page)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_document_load_page(RequireOpen(),
                page, out int n), "LoadPage");
            return new Page(this, n);
        }


        /// <summary>
        /// Insert a fresh page.
        /// </summary>
        /// <param name="at">Insertion point.</param>
        /// <param name="width">Page width in points.</param>
        /// <param name="height">Page height in points.</param>
        /// <returns>The new page.</returns>
        public Page NewPage(int at, double width, double height)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_document_new_page(RequireOpen(),
                at, width, height, out int n), "NewPage");
            return new Page(this, n);
        }


        /// <summary>
        /// Replace the outline; returns its size.
        /// </summary>
        /// <param name="entries">Outline entries.</param>
        /// <param name="collapse">Collapse level.</param>
        /// <returns>The outline size.</returns>
        public int SetToc(List<TocEntry> entries, int collapse)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_toc_builder_new(out IntPtr builder),
                "SetToc");
            try
            {
                foreach (TocEntry entry in entries)
                {
                    Point target = entry.DestTarget;
                    CompatSupport.Check(NativeMethods.pdf_compat_toc_builder_push(builder,
                        entry.Level, entry.Title, entry.Page,
                        CompatSupport.Flag(entry.HasDest), entry.DestKind, entry.DestPage,
                        target.X, target.Y, entry.DestZoom), "SetToc");
                }
                CompatSupport.Check(NativeMethods.pdf_compat_document_set_toc(RequireOpen(),
                    builder, collapse, out int n), "SetToc");
                return n;
            }
            finally
            {
                NativeMethods.pdf_compat_toc_builder_free(builder);
            }
        }


        /// <summary>
        /// Replace the document metadata.
        /// </summary>
        /// <param name="metadata">New metadata, or null to clear.</param>
        public void SetMetadata(DocMetadata? metadata)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_document_set_metadata(RequireOpen(),
                metadata == null ? null : metadata.Title,
                metadata == null ? null : metadata.Author,
                metadata == null ? null : metadata.Subject,
                metadata == null ? null : metadata.Keywords,
                metadata == null ? null : metadata.Creator,
                metadata == null ? null : metadata.Producer,
                metadata == null ? null : metadata.CreationDate,
                metadata == null ? null : metadata.ModDate,
                metadata == null ? null : metadata.Format,
                metadata == null ? null : metadata.Encryption,
                metadata == null ? null : metadata.Trapped), "SetMetadata");
        }


        /// <summary>
        /// One object key: value and type name.
        /// </summary>
        /// <param name="xref">Object number.</param>
        /// <param name="key">Key name.</param>
        /// <returns>The value and its type name.</returns>
        public string[] XrefGetKey(int xref, string? key)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_document_xref_get_key(RequireOpen(),
                xref, key, out IntPtr sA, out IntPtr sB), "XrefGetKey");
            return new string[] { CompatSupport.ReadString(sA), CompatSupport.ReadString(sB) };
        }


        /// <summary>
        /// Insert pages from another document.
        /// </summary>
        /// <param name="source">Source document.</param>
        /// <param name="options">Import options.</param>
        public void InsertPdf(Document source, InsertPdfOptions options)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_document_insert_pdf(RequireOpen(),
                source.RequireOpen(), options.FromPage, options.ToPage, options.StartAt,
                options.Rotate, CompatSupport.Flag(options.Links),
                CompatSupport.Flag(options.Annots), CompatSupport.Flag(options.Widgets),
                CompatSupport.Flag(options.JoinDuplicates),
                CompatSupport.Flag(options.ShowProgress),
                CompatSupport.Flag(options.Finish)), "InsertPdf");
        }


        /// <summary>
        /// All pages, in order.
        /// </summary>
        /// <returns>One <see cref="Page"/> per page.</returns>
        public List<Page> Pages()
        {
            return Pages(null, null, null);
        }

        /// <summary>
        /// A page slice, in order.
        /// </summary>
        /// <param name="start">First page, or null for the first.</param>
        /// <param name="stop">Stop before this page, or null for the end.</param>
        /// <param name="step">Stride, or null for 1.</param>
        /// <returns>The selected pages.</returns>
        public List<Page> Pages(int? start, int? stop, int? step)
        {
            int count = PageCount();
            int from = start ?? 0;
            int to = stop ?? count;
            int stride = step ?? 1;
            var result = new List<Page>();
            for (int i = from; i < to; i += stride)
                result.Add(LoadPage(i));
            return result;
        }


        /// <summary>
        /// Close the handle. Idempotent.
        /// </summary>
        public void Dispose()
        {
            if (_handle != IntPtr.Zero)
            {
                NativeMethods.pdf_compat_document_close(_handle);
                _handle = IntPtr.Zero;
            }
            GC.SuppressFinalize(this);
        }

        /// <summary>Finalize the handle.</summary>
        ~Document()
        {
            if (_handle != IntPtr.Zero)
                NativeMethods.pdf_compat_document_close(_handle);
        }

        /// <summary>
        /// The native handle, failing when closed.
        /// </summary>
        /// <returns>The live handle.</returns>
        public IntPtr RequireOpen()
        {
            if (_handle == IntPtr.Zero)
                throw new PdfluentException("document is closed");
            return _handle;
        }
    }
}
