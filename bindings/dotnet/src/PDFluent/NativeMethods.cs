using System;
using System.Runtime.InteropServices;

namespace PDFluent
{
    /// <summary>
    /// Raw P/Invoke declarations for the pdf-capi native library.
    /// </summary>
    internal static class NativeMethods
    {
        private const string LibName = "pdf_capi";

        // ---- Library lifecycle ----

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PdfStatus pdf_init();

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void pdf_destroy();

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr pdf_version();

        // ---- Document lifecycle ----

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PdfStatus pdf_document_open_from_bytes(
            byte[] data,
            UIntPtr len,
            out IntPtr doc);

        // CharSet is intentionally omitted; LPUTF8Str on each parameter provides the
        // explicit UTF-8 marshaling that satisfies CA2101.
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PdfStatus pdf_document_open(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string path,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? password,
            out IntPtr doc);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void pdf_document_free(IntPtr doc);

        // ---- Document queries ----

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int pdf_document_page_count(IntPtr doc);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern double pdf_page_width(IntPtr doc, int pageIndex);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern double pdf_page_height(IntPtr doc, int pageIndex);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int pdf_page_rotation(IntPtr doc, int pageIndex);

        // ---- Rendering ----

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PdfStatus pdf_page_render(
            IntPtr doc,
            int pageIndex,
            double dpi,
            out uint outWidth,
            out uint outHeight,
            out IntPtr outPixels);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PdfStatus pdf_page_render_thumbnail(
            IntPtr doc,
            int pageIndex,
            uint maxDimension,
            out uint outWidth,
            out uint outHeight,
            out IntPtr outPixels);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void pdf_pixels_free(IntPtr pixels, UIntPtr len);

        // ---- Office export ----

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PdfStatus pdf_document_to_docx(
            IntPtr doc,
            out IntPtr outData,
            out UIntPtr outLen);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PdfStatus pdf_document_to_xlsx(
            IntPtr doc,
            out IntPtr outData,
            out UIntPtr outLen);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PdfStatus pdf_document_to_pptx(
            IntPtr doc,
            out IntPtr outData,
            out UIntPtr outLen);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void pdf_bytes_free(IntPtr data, UIntPtr len);

        // ---- Text extraction ----

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr pdf_page_extract_text(IntPtr doc, int pageIndex);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void pdf_string_free(IntPtr s);

        // ---- Text editing (layout-aware find & replace) ----

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PdfStatus pdf_text_editor_open_from_bytes(
            byte[] data,
            UIntPtr len,
            out IntPtr editor);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void pdf_text_editor_free(IntPtr editor);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr pdf_text_editor_page_count(IntPtr editor);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PdfStatus pdf_text_editor_find(
            IntPtr editor,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string queryJson,
            out IntPtr outJson);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PdfStatus pdf_text_editor_replace(
            IntPtr editor,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string queryJson,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string replacement,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string optionsJson,
            out IntPtr outJson);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PdfStatus pdf_text_editor_replace_matches(
            IntPtr editor,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string editsJson,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string optionsJson,
            out IntPtr outJson);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PdfStatus pdf_text_editor_to_bytes(
            IntPtr editor,
            out IntPtr outData,
            out UIntPtr outLen);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void pdf_text_editor_bytes_free(IntPtr data, UIntPtr len);

        // ---- Structured text-block extraction ----

        /// <summary>
        /// Native layout of <c>PdfTextBlock</c> — must match the C struct
        /// in <c>include/pdfluent.h</c>. Five fields: <c>(double, double,
        /// double, double, const char*)</c>. Sequential layout so P/Invoke
        /// marshals it as a packed struct over the FFI boundary.
        /// </summary>
        [StructLayout(LayoutKind.Sequential)]
        internal struct PdfTextBlockNative
        {
            public double X;
            public double Y;
            public double Width;
            public double Height;
            public IntPtr Text;   // const char* — caller does NOT free this
                                  // pointer individually; the whole array
                                  // is released via pdf_text_blocks_free.
        }

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PdfStatus pdf_page_extract_text_blocks(
            IntPtr doc,
            int pageIndex,
            out IntPtr outBlocks,
            out UIntPtr outCount);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void pdf_text_blocks_free(IntPtr blocks, UIntPtr count);

        // ---- Metadata ----

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr pdf_document_get_meta(
            IntPtr doc,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string key);

        // ---- Bookmarks ----

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int pdf_bookmark_count(IntPtr doc);

        // ---- Page geometry boxes ----

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PdfStatus pdf_page_media_box(
            IntPtr doc,
            int pageIndex,
            out double x0,
            out double y0,
            out double x1,
            out double y1);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PdfStatus pdf_page_crop_box(
            IntPtr doc,
            int pageIndex,
            out double x0,
            out double y0,
            out double x1,
            out double y1);

        // ---- Error state ----

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr pdf_get_last_error();

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void pdf_clear_error();

        // ---- Digital signing + signature verification ----

        // doc, pkcs12_path, pkcs12_password are BORROWED; *out is a NEW document
        // the caller must free with pdf_document_free.
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PdfStatus pdf_document_sign(
            IntPtr doc,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string pkcs12Path,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? pkcs12Password,
            out IntPtr outDoc);

        // Returns signature-field count (>= 0), or -1 if doc is NULL.
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int pdf_signature_count(IntPtr doc);

        // Returns 1 (valid), 0 (invalid), or -1 (unknown / error / out-of-range).
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int pdf_signature_is_valid(IntPtr doc, int index);
    }
}
