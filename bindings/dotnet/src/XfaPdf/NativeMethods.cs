using System;
using System.Runtime.InteropServices;

namespace XfaPdf
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

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl, CharSet = CharSet.Ansi)]
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

        // ---- Text extraction ----

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr pdf_page_extract_text(IntPtr doc, int pageIndex);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void pdf_string_free(IntPtr s);

        // ---- Metadata ----

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl, CharSet = CharSet.Ansi)]
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
    }
}
