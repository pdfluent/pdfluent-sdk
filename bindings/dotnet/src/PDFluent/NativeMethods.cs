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

        // ---- Embedded images (facade) ----

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr pdf_page_images_json(
            IntPtr doc,
            int pageIndex);

        /// <summary>
        /// One extracted image: header facts plus heap-allocated fields.
        /// Mirrors the C <c>PdfExtractedImage</c> layout field for field;
        /// the runtime marshals it as a packed struct over the FFI boundary.
        /// </summary>
        [StructLayout(LayoutKind.Sequential)]
        internal struct PdfExtractedImageNative
        {
            public uint Width;
            public uint Height;
            public uint BitsPerComponent;
            public uint Components;
            public IntPtr ColorSpace;       // char* — freed via pdf_extracted_image_free
            public IntPtr ColorSpaceName;   // char* — freed via pdf_extracted_image_free
            public IntPtr Encoding;         // char* — freed via pdf_extracted_image_free
            public IntPtr Data;             // uint8_t* — freed via pdf_extracted_image_free
            public UIntPtr Len;
            public byte Masked;             // 1 when masked, else 0
            public uint Xres;
            public uint Yres;
        }

        /// <summary>
        /// One decoded image: raw samples plus heap-allocated fields.
        /// Mirrors the C <c>PdfDecodedImage</c> layout field for field.
        /// </summary>
        [StructLayout(LayoutKind.Sequential)]
        internal struct PdfDecodedImageNative
        {
            public uint Width;
            public uint Height;
            public uint Components;
            public IntPtr ColorSpace;       // char* — freed via pdf_decoded_image_free
            public IntPtr Pixels;           // uint8_t* — freed via pdf_decoded_image_free
            public UIntPtr Len;
        }

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PdfStatus pdf_image_extract_data(
            IntPtr doc,
            uint imageId,
            out PdfExtractedImageNative image);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void pdf_extracted_image_free(IntPtr image);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PdfStatus pdf_image_decode_pixels(
            IntPtr doc,
            uint imageId,
            out PdfDecodedImageNative image);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void pdf_decoded_image_free(IntPtr image);

        // ---- Text extraction ----

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr pdf_page_extract_text(IntPtr doc, int pageIndex);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void pdf_string_free(IntPtr s);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr pdf_document_extract_markdown(
            IntPtr doc,
            int firstPage,
            int pageCount);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr pdf_document_extract_blocks_json(
            IntPtr doc,
            int firstPage,
            int pageCount);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr pdf_document_extract_docling_json(
            IntPtr doc,
            int firstPage,
            int pageCount,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? name);

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

        // ---- Document serialisation ----

        // Writes the document's current bytes. Free *outData with pdf_bytes_free.
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PdfStatus pdf_document_tobytes(
            IntPtr doc,
            out IntPtr outData,
            out UIntPtr outLen);

        // ---- AcroForm fields ----

        // Terminal-field count (>= 0), 0 when no AcroForm, -1 on null doc.
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int pdf_form_field_count(IntPtr doc);

        // Heap name string (free with pdf_string_free), or NULL.
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr pdf_form_field_name(IntPtr doc, int index);

        // Heap type string ("text"/"button"/"choice"/"signature"/"unknown"),
        // free with pdf_string_free, or NULL.
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr pdf_form_field_type(IntPtr doc, int index);

        // Heap value string (free with pdf_string_free), or NULL when the
        // field does not exist or carries no value.
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr pdf_form_field_value(
            IntPtr doc,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string name);

        // *outDoc is a NEW document the caller must free with pdf_document_free.
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PdfStatus pdf_form_set_field_value(
            IntPtr doc,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string name,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string value,
            out IntPtr outDoc);

        // ---- Annotations ----

        // Per-page count (>= 0), or -1 on error.
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int pdf_annotation_count(IntPtr doc, int pageIndex);

        // Heap subtype string (free with pdf_string_free), or NULL.
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr pdf_annotation_type(
            IntPtr doc,
            int pageIndex,
            int annotIndex);

        // Annotation bounding box in points (x0, y0, x1, y1; y up);
        // zeros when the annotation carries no usable /Rect.
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PdfStatus pdf_annotation_rect(
            IntPtr doc,
            int pageIndex,
            int annotIndex,
            out double outX0,
            out double outY0,
            out double outX1,
            out double outY1);

        // Adds a yellow highlight; *outDoc is a NEW document the caller
        // must free with pdf_document_free. x/y/w/h in points, y up.
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PdfStatus pdf_annotation_add_highlight(
            IntPtr doc,
            int pageIndex,
            double x,
            double y,
            double w,
            double h,
            out IntPtr outDoc);

        // Same, but stores contents as the /Contents string. Null contents
        // stores no /Contents entry.
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PdfStatus pdf_annotation_add_highlight_with_contents(
            IntPtr doc,
            int pageIndex,
            double x,
            double y,
            double w,
            double h,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? contents,
            out IntPtr outDoc);

        // ---- Document merge ----

        // docs is an array of `count` document pointers; *outDoc is a NEW
        // document the caller must free with pdf_document_free.
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PdfStatus pdf_documents_merge(
            [MarshalAs(UnmanagedType.LPArray)] IntPtr[] docs,
            int count,
            out IntPtr outDoc);

        // ---- Image extraction ----

        // Per-page image count (>= 0), or -1 on error.
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int pdf_page_image_count(IntPtr doc, int pageIndex);

        // Decoded image bytes; free *outData with pdf_bytes_free(*outData, *outLen).
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PdfStatus pdf_page_extract_image(
            IntPtr doc,
            int pageIndex,
            int imageIndex,
            out uint outWidth,
            out uint outHeight,
            out IntPtr outData,
            out UIntPtr outLen);

        // ---- PDF/A validation ----

        // level is a PdfALevel value (0-10); *outReport must be freed with
        // pdf_compliance_report_free. Ok means the validation RAN, not passed.
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern PdfStatus pdf_document_validate_pdfa(
            IntPtr doc,
            int level,
            out IntPtr outReport);

        // 1 when the report indicates full conformance, else 0.
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int pdf_compliance_report_is_compliant(IntPtr report);

        // Conformance-error count, or -1 on null report.
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int pdf_compliance_report_error_count(IntPtr report);

        // Warning count, or -1 on null report.
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int pdf_compliance_report_warning_count(IntPtr report);

        // Issue count (errors + warnings + info), or -1 on null report.
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int pdf_compliance_report_issue_count(IntPtr report);

        // Heap "[severity] rule: message" string (free with
        // pdf_string_free), or NULL for an out-of-range index.
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr pdf_compliance_report_issue_summary(
            IntPtr report,
            int index);

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void pdf_compliance_report_free(IntPtr report);

        // ---- Compatible API (lane L5) ----

        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int pdf_compat_last_error_kind();

        /// <summary>Snapshot count.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_annot_entry_list_count(IntPtr list, out int @out);

        /// <summary>Free a snapshot list. Null is safe.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe void pdf_compat_annot_entry_list_free(IntPtr list);

        /// <summary>Annotation box in points.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_annot_entry_list_rect(IntPtr list, int index, Rect* @out);

        /// <summary>Annotation type code.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_annot_entry_list_type_code(IntPtr list, int index, out int @out);

        /// <summary>Annotation type name.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_annot_entry_list_type_name(IntPtr list, int index, out IntPtr @out);

        /// <summary>Annotation object number.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_annot_entry_list_xref(IntPtr list, int index, out int @out);

        /// <summary>Set stroke and fill colours.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_annot_set_colors(
            IntPtr doc,
            int page,
            int xref,
            double* stroke,
            UIntPtr stroke_len,
            double* fill,
            UIntPtr fill_len
        );

        /// <summary>Push appearance changes.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_annot_update(
            IntPtr doc,
            int page,
            int xref,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? blend_mode,
            double opacity,
            double fontsize,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? fontname,
            double* text_color,
            UIntPtr text_color_len,
            double* border_color,
            UIntPtr border_color_len,
            double* fill_color,
            UIntPtr fill_color_len,
            int cross_out,
            int rotate
        );

        /// <summary>Channel count, excluding alpha.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_colorspace_n(int colorspace, out int @out);

        /// <summary>Unlock with a password.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_authenticate(IntPtr doc, [MarshalAs(UnmanagedType.LPUTF8Str)] string? password, out int @out);

        /// <summary>Flatten annotations and widgets into content.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_bake(IntPtr doc, int annots, int widgets);

        /// <summary>Close the document and free its handle.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_close(IntPtr doc);

        /// <summary>Convert a page span to PDF content.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_convert_to_pdf(IntPtr doc, int from_page, int to_page, int rotate, out IntPtr out_data, out UIntPtr out_len);

        /// <summary>Font bytes and facts.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_extract_font(
            IntPtr doc,
            int xref,
            int info_only,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? name,
            out IntPtr @out
        );

        /// <summary>Image bytes and facts for an object number.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_extract_image(IntPtr doc, int xref, out IntPtr @out);

        /// <summary>Reserve a fresh object number.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_get_new_xref(IntPtr doc, out int @out);

        /// <summary>Fonts referenced by a page.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_get_page_fonts(IntPtr doc, int page, int full, out IntPtr @out);

        /// <summary>Images referenced by a page.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_get_page_images(IntPtr doc, int page, int full, out IntPtr @out);

        /// <summary>Outline entries.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_get_toc(IntPtr doc, int simple, out IntPtr @out);

        /// <summary>Insert pages from another document.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_insert_pdf(
            IntPtr doc,
            IntPtr source,
            int from_page,
            int to_page,
            int start_at,
            int rotate,
            int links,
            int annots,
            int widgets,
            int join_duplicates,
            int show_progress,
            int finish
        );

        /// <summary>Whether this is a form PDF.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_is_form_pdf(IntPtr doc, out int @out);

        /// <summary>Whether this is a PDF document.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_is_pdf(IntPtr doc, out int @out);

        /// <summary>Undo/redo availability of the compatible journal.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_journal_can_do(IntPtr doc, out int can_undo, out int can_redo);

        /// <summary>Enable the compatible journal.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_journal_enable(IntPtr doc);

        /// <summary>Whether the compatible journal is enabled.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_journal_is_enabled(IntPtr doc, out int @out);

        /// <summary>Load a journal file over a never-enabled journal.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_journal_load(IntPtr doc, [MarshalAs(UnmanagedType.LPUTF8Str)] string? path);

        /// <summary>Journal step name.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_journal_op_name(IntPtr doc, int step, out IntPtr @out);

        /// <summary>Journal position: the applied prefix length.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_journal_position(IntPtr doc, out int @out);

        /// <summary>Redo a journal step.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_journal_redo(IntPtr doc);

        /// <summary>Save the journal file.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_journal_save(IntPtr doc, [MarshalAs(UnmanagedType.LPUTF8Str)] string? path);

        /// <summary>Start a journal step; the name may be null for an anonymous step.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_journal_start_op(IntPtr doc, [MarshalAs(UnmanagedType.LPUTF8Str)] string? name);

        /// <summary>Stop the journal step.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_journal_stop_op(IntPtr doc);

        /// <summary>Undo a journal step.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_journal_undo(IntPtr doc);

        /// <summary>Validate a page number; returns it.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_load_page(IntPtr doc, int page, out int @out);

        /// <summary>Whether the document needs a password.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_needs_pass(IntPtr doc, out int @out);

        /// <summary>Insert a fresh page; returns its number.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_new_page(
            IntPtr doc,
            int at,
            double width,
            double height,
            out int @out
        );

        /// <summary>Open a file, bytes, or a fresh shell.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_open(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? filename,
            byte* data,
            UIntPtr data_len,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? filetype,
            Rect* rect,
            double width,
            double height,
            double fontsize,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? archive,
            out IntPtr @out
        );

        /// <summary>Number of pages. Binds PdfDocument::len; the spec names no page_count (reported).</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_page_count(IntPtr doc, out int @out);

        /// <summary>The document catalog.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_pdf_catalog(IntPtr doc, out IntPtr @out);

        /// <summary>Save to a file.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_save(IntPtr doc, [MarshalAs(UnmanagedType.LPUTF8Str)] string? path, [MarshalAs(UnmanagedType.LPUTF8Str)] string? settings);

        /// <summary>Save incrementally.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_save_incr(IntPtr doc);

        /// <summary>Replace the document metadata.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_set_metadata(
            IntPtr doc,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? title,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? author,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? subject,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? keywords,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? creator,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? producer,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? creation_date,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? mod_date,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? format,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? encryption,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? trapped
        );

        /// <summary>Replace the outline; returns its size.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_set_toc(IntPtr doc, IntPtr toc, int collapse, out int @out);

        /// <summary>Serialise the document to bytes.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_tobytes(IntPtr doc, [MarshalAs(UnmanagedType.LPUTF8Str)] string? settings, out IntPtr out_data, out UIntPtr out_len);

        /// <summary>Replace one raw object.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_update_object(IntPtr doc, int xref, [MarshalAs(UnmanagedType.LPUTF8Str)] string? text, int page);

        /// <summary>Replace one stream payload.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_update_stream(
            IntPtr doc,
            int xref,
            byte* data,
            UIntPtr data_len,
            int is_new,
            int compress
        );

        /// <summary>Write the document to bytes.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_write(IntPtr doc, [MarshalAs(UnmanagedType.LPUTF8Str)] string? settings, out IntPtr out_data, out UIntPtr out_len);

        /// <summary>One object key: value and type.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_xref_get_key(
            IntPtr doc,
            int xref,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? key,
            out IntPtr @out,
            out IntPtr out2
        );

        /// <summary>Object count.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_xref_length(IntPtr doc, out int @out);

        /// <summary>One raw object.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_xref_object(
            IntPtr doc,
            int xref,
            int compressed,
            int ascii,
            out IntPtr @out
        );

        /// <summary>Set one object key.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_xref_set_key(IntPtr doc, int xref, [MarshalAs(UnmanagedType.LPUTF8Str)] string? key, [MarshalAs(UnmanagedType.LPUTF8Str)] string? value);

        /// <summary>One stream payload.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_document_xref_stream(IntPtr doc, int xref, out IntPtr out_data, out UIntPtr out_len);

        /// <summary>Close-path flag, if any.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_drawing_list_close_path(IntPtr list, int index, out int out_has, out int @out);

        /// <summary>Stroke colour, or empty.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_drawing_list_color(
            IntPtr list,
            int index,
            out int out_has,
            out IntPtr out_data,
            out UIntPtr out_len
        );

        /// <summary>Snapshot count.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_drawing_list_count(IntPtr list, out int @out);

        /// <summary>Dash pattern, if any.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_drawing_list_dashes(IntPtr list, int index, out int out_has, out IntPtr @out);

        /// <summary>Drawing operator name.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_drawing_list_drawing_type(IntPtr list, int index, out IntPtr @out);

        /// <summary>Even-odd fill rule.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_drawing_list_even_odd(IntPtr list, int index, out int @out);

        /// <summary>Fill colour, or empty.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_drawing_list_fill(
            IntPtr list,
            int index,
            out int out_has,
            out IntPtr out_data,
            out UIntPtr out_len
        );

        /// <summary>Fill opacity, if any.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_drawing_list_fill_opacity(IntPtr list, int index, out int out_has, out double @out);

        /// <summary>Free a snapshot list. Null is safe.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe void pdf_compat_drawing_list_free(IntPtr list);

        /// <summary>Path-item count.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_drawing_list_item_count(IntPtr list, int index, out int @out);

        /// <summary>Path-item operator.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_drawing_list_item_op(IntPtr list, int index, int sub, out IntPtr @out);

        /// <summary>Path-item parameters as JSON.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_drawing_list_item_params(IntPtr list, int index, int sub, out IntPtr @out);

        /// <summary>Layer name.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_drawing_list_layer(IntPtr list, int index, out IntPtr @out);

        /// <summary>Nesting level, if any.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_drawing_list_level(IntPtr list, int index, out int out_has, out int @out);

        /// <summary>Line-cap values, or empty.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_drawing_list_line_cap(
            IntPtr list,
            int index,
            out int out_has,
            out IntPtr out_data,
            out UIntPtr out_len
        );

        /// <summary>Line-join value, if any.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_drawing_list_line_join(IntPtr list, int index, out int out_has, out int @out);

        /// <summary>Drawing box in points.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_drawing_list_rect(IntPtr list, int index, Rect* @out);

        /// <summary>Sequence number.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_drawing_list_seqno(IntPtr list, int index, out int @out);

        /// <summary>Stroke opacity, if any.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_drawing_list_stroke_opacity(IntPtr list, int index, out int out_has, out double @out);

        /// <summary>Stroke width, if any.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_drawing_list_width(IntPtr list, int index, out int out_has, out double @out);

        /// <summary>Font bytes.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_font_data_data(IntPtr handle, out IntPtr out_data, out UIntPtr out_len);

        /// <summary>Free a snapshot list. Null is safe.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe void pdf_compat_font_data_free(IntPtr list);

        /// <summary>Font name.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_font_data_name(IntPtr handle, out IntPtr @out);

        /// <summary>Free a font. Null is safe.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe void pdf_compat_font_free(IntPtr handle);

        /// <summary>Snapshot count.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_font_info_list_count(IntPtr list, out int @out);

        /// <summary>Free a snapshot list. Null is safe.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe void pdf_compat_font_info_list_free(IntPtr list);

        /// <summary>Font name.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_font_info_list_name(IntPtr list, int index, out IntPtr @out);

        /// <summary>Font object number.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_font_info_list_xref(IntPtr list, int index, out int @out);

        /// <summary>Build a font from a name, file, or buffer.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_font_new(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? fontname,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? fontfile,
            byte* fontbuffer,
            UIntPtr fontbuffer_len,
            int script,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? language,
            int ordering,
            int is_bold,
            int is_italic,
            int is_serif,
            int embed,
            out IntPtr @out
        );

        /// <summary>Typeset width of a string.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_font_text_length(
            IntPtr font,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? text,
            double fontsize,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? language,
            int script,
            int wmode,
            int small_caps,
            int fallback,
            out double @out
        );

        /// <summary>Typeset width of a string in a base font.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_get_text_length(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? text,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? fontname,
            double fontsize,
            int encoding,
            out double @out
        );

        /// <summary>Bits per component.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_image_data_bits(IntPtr handle, out int @out);

        /// <summary>Colour-space code.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_image_data_colorspace(IntPtr handle, out int @out);

        /// <summary>Colour-space name.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_image_data_colorspace_name(IntPtr handle, out IntPtr @out);

        /// <summary>Raw payload bytes.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_image_data_data(IntPtr handle, out IntPtr out_data, out UIntPtr out_len);

        /// <summary>File extension.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_image_data_ext(IntPtr handle, out IntPtr @out);

        /// <summary>Free a snapshot list. Null is safe.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe void pdf_compat_image_data_free(IntPtr list);

        /// <summary>Height in pixels.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_image_data_height(IntPtr handle, out int @out);

        /// <summary>Whether masked.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_image_data_masked(IntPtr handle, out int @out);

        /// <summary>Payload size in bytes.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_image_data_size(IntPtr handle, out int @out);

        /// <summary>Width in pixels.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_image_data_width(IntPtr handle, out int @out);

        /// <summary>Horizontal resolution.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_image_data_xres(IntPtr handle, out int @out);

        /// <summary>Vertical resolution.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_image_data_yres(IntPtr handle, out int @out);

        /// <summary>Alternate name.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_image_ref_list_alt(IntPtr list, int index, out IntPtr @out);

        /// <summary>Bits per component.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_image_ref_list_bits(IntPtr list, int index, out int @out);

        /// <summary>Colour-space name.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_image_ref_list_colorspace_name(IntPtr list, int index, out IntPtr @out);

        /// <summary>Snapshot count.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_image_ref_list_count(IntPtr list, out int @out);

        /// <summary>Filter name.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_image_ref_list_filter(IntPtr list, int index, out IntPtr @out);

        /// <summary>Free a snapshot list. Null is safe.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe void pdf_compat_image_ref_list_free(IntPtr list);

        /// <summary>Height in pixels.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_image_ref_list_height(IntPtr list, int index, out int @out);

        /// <summary>Mask object number.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_image_ref_list_mask(IntPtr list, int index, out int @out);

        /// <summary>Resource name.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_image_ref_list_name(IntPtr list, int index, out IntPtr @out);

        /// <summary>Width in pixels.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_image_ref_list_width(IntPtr list, int index, out int @out);

        /// <summary>Image object number.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_image_ref_list_xref(IntPtr list, int index, out int @out);

        /// <summary>Build an integer rectangle from its four edges.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_irect_new(
            int x0,
            int y0,
            int x1,
            int y1,
            IRect* @out
        );

        /// <summary>Link border cloud effect level.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_border_clouds(IntPtr link, out int @out);

        /// <summary>Link border dashes: element count plus heap ints, freed with `pdf_bytes_free`.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_border_dashes(IntPtr link, out IntPtr out_data, out UIntPtr out_len);

        /// <summary>Link border style, if any: `has` says whether one exists.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_border_style(IntPtr link, out int out_has, out IntPtr out_style);

        /// <summary>Link border width in points.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_border_width(IntPtr link, out double @out);

        /// <summary>Clear the follow-on destination.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_clear_dest(IntPtr link);

        /// <summary>Follow-on destination page, or -1 when there is none.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_dest_page(IntPtr link, out int @out);

        /// <summary>Follow-on destination point, or the origin when there is none.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_dest_target(IntPtr link, Point* @out);

        /// <summary>Follow-on destination zoom, or 0 when there is none.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_dest_zoom(IntPtr link, out double @out);

        /// <summary>Link fill colour: element count plus heap doubles, freed with `pdf_bytes_free`.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_fill(IntPtr link, out IntPtr out_data, out UIntPtr out_len);

        /// <summary>Link flags.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_flags(IntPtr link, out int @out);

        /// <summary>Free a link. Null is safe.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe void pdf_compat_link_free(IntPtr handle);

        /// <summary>Whether the link carries a follow-on destination: 1 yes, 0 no.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_has_dest(IntPtr link, out int @out);

        /// <summary>Whether the link chains to another: 1 yes, 0 no.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_has_next(IntPtr link, out int @out);

        /// <summary>Link identifier.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_id(IntPtr link, out IntPtr @out);

        /// <summary>Link kind code.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_kind(IntPtr link, out int @out);

        /// <summary>Clone one entry into a fresh handle.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_list_at(IntPtr list, int index, out IntPtr @out);

        /// <summary>Snapshot count.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_list_count(IntPtr list, out int @out);

        /// <summary>Free a snapshot list. Null is safe.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe void pdf_compat_link_list_free(IntPtr list);

        /// <summary>Build a blank link.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_new(out IntPtr @out);

        /// <summary>Clone the chained link into a fresh handle, freed with `pdf_compat_link_free`.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_next(IntPtr link, out IntPtr @out);

        /// <summary>Link box in points.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_rect(IntPtr link, Rect* @out);

        /// <summary>Set the link border cloud effect level.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_set_border_clouds(IntPtr link, int value);

        /// <summary>Set the link border dashes.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_set_border_dashes(IntPtr link, int* values, UIntPtr len);

        /// <summary>Set the link border style; null clears it.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_set_border_style(IntPtr link, [MarshalAs(UnmanagedType.LPUTF8Str)] string? value);

        /// <summary>Set the link border width.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_set_border_width(IntPtr link, double value);

        /// <summary>Set the link fill colour.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_set_fill(IntPtr link, double* values, UIntPtr len);

        /// <summary>Set the link flags.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_set_flags(IntPtr link, int value);

        /// <summary>Set the link identifier.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_set_id(IntPtr link, [MarshalAs(UnmanagedType.LPUTF8Str)] string? value);

        /// <summary>Set the link kind code.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_set_kind(IntPtr link, int value);

        /// <summary>Chain another link; null clears the chain.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_set_next(IntPtr link, IntPtr next);

        /// <summary>Set the link box.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_set_rect(IntPtr link, Rect* value);

        /// <summary>Set the link stroke colour.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_set_stroke(IntPtr link, double* values, UIntPtr len);

        /// <summary>Set the link target URI.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_set_uri(IntPtr link, [MarshalAs(UnmanagedType.LPUTF8Str)] string? value);

        /// <summary>Set the link object number.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_set_xref(IntPtr link, int value);

        /// <summary>Link stroke colour: element count plus heap doubles, freed with `pdf_bytes_free`.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_stroke(IntPtr link, out IntPtr out_data, out UIntPtr out_len);

        /// <summary>Link target URI.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_uri(IntPtr link, out IntPtr @out);

        /// <summary>Link object number.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_link_xref(IntPtr link, out int @out);

        /// <summary>Build a matrix from its six coefficients.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_matrix_new(
            double a,
            double b,
            double c,
            double d,
            double e,
            double f,
            Matrix* @out
        );

        /// <summary>Rotate a matrix in place by a quarter-turn count; writes the new value.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_matrix_prerotate(Matrix* matrix, double theta, Matrix* @out);

        /// <summary>Highlight text; returns the annotation xref.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_add_highlight_annot(
            IntPtr doc,
            int page,
            Quad* quads,
            UIntPtr quads_len,
            Point* start,
            Point* stop,
            Rect* clip,
            out int @out
        );

        /// <summary>Add a rectangle annotation; returns its xref.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_add_rect_annot(IntPtr doc, int page, Rect* rect, out int @out);

        /// <summary>Mark a quadrilateral for redaction; returns its xref.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_add_redact_annot(
            IntPtr doc,
            int page,
            Quad* quad,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? text,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? fontname,
            double fontsize,
            int align,
            double* fill,
            UIntPtr fill_len,
            double* text_color,
            UIntPtr text_color_len,
            int cross_out,
            out int @out
        );

        /// <summary>Add a text annotation; returns its xref.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_add_text_annot(
            IntPtr doc,
            int page,
            Point* at,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? text,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? icon,
            out int @out
        );

        /// <summary>Add a form widget; returns its xref.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_add_widget(IntPtr doc, int page, IntPtr widget, out int @out);

        /// <summary>Annotations on the page.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_annots(
            IntPtr doc,
            int page,
            int* types,
            UIntPtr types_len,
            out IntPtr @out
        );

        /// <summary>Burn pending redactions in.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_apply_redactions(
            IntPtr doc,
            int page,
            int images,
            int graphics,
            int text,
            out int @out
        );

        /// <summary>Tidy the content stream.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_clean_contents(IntPtr doc, int page, int sanitize);

        /// <summary>Delete an annotation.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_delete_annot(
            IntPtr doc,
            int page,
            int xref,
            int type_code,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? type_name,
            Rect* rect
        );

        /// <summary>Stroke a Bezier curve.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_draw_bezier(
            IntPtr doc,
            int page,
            Point* p1,
            Point* p2,
            Point* p3,
            Point* p4,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? options,
            Point* @out
        );

        /// <summary>Stroke a circle.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_draw_circle(
            IntPtr doc,
            int page,
            Point* center,
            double radius,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? options,
            Point* @out
        );

        /// <summary>Stroke a line.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_draw_line(
            IntPtr doc,
            int page,
            Point* p1,
            Point* p2,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? options,
            Point* @out
        );

        /// <summary>Stroke a rectangle.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_draw_rect(
            IntPtr doc,
            int page,
            Rect* rect,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? options,
            double radius,
            Point* @out
        );

        /// <summary>Detect tables on the page.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_find_tables(IntPtr doc, int page, out IntPtr @out);

        /// <summary>First annotation, if any.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_first_annot(IntPtr doc, int page, out IntPtr @out);

        /// <summary>Content-stream object numbers.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_get_contents(IntPtr doc, int page, out IntPtr out_data, out UIntPtr out_len);

        /// <summary>Vector drawings on the page.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_get_drawings(IntPtr doc, int page, int extended, out IntPtr @out);

        /// <summary>Fonts used on the page.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_get_fonts(IntPtr doc, int page, int full, out IntPtr @out);

        /// <summary>Placed images with geometry.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_get_image_info(
            IntPtr doc,
            int page,
            int hashes,
            int xrefs,
            out IntPtr @out
        );

        /// <summary>Boxes where a named image appears.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_get_image_rects(
            IntPtr doc,
            int page,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? name,
            int transform,
            out IntPtr @out
        );

        /// <summary>Images on the page.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_get_images(IntPtr doc, int page, int full, out IntPtr @out);

        /// <summary>Links on the page.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_get_links(IntPtr doc, int page, out IntPtr @out);

        /// <summary>Render the page to a pixel buffer.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_get_pixmap(
            IntPtr doc,
            int page,
            Matrix* matrix,
            double dpi,
            int colorspace,
            Rect* clip,
            int alpha,
            int annots,
            out IntPtr @out
        );

        /// <summary>Render the page to SVG text.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_get_svg_image(
            IntPtr doc,
            int page,
            Matrix* matrix,
            int text_as_path,
            out IntPtr @out
        );

        /// <summary>Extract text in the named format.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_get_text(
            IntPtr doc,
            int page,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? format,
            Rect* clip,
            int flags,
            int sort,
            int use_textpage,
            out IntPtr @out
        );

        /// <summary>Text inside a rectangle.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_get_textbox(
            IntPtr doc,
            int page,
            Rect* rect,
            int use_textpage,
            out IntPtr @out
        );

        /// <summary>Typeset spans with glyph origins.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_get_texttrace(IntPtr doc, int page, out IntPtr @out);

        /// <summary>Register a font for the page.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_insert_font(
            IntPtr doc,
            int page,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? fontname,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? fontfile,
            byte* fontbuffer,
            UIntPtr fontbuffer_len,
            int set_simple,
            int wmode,
            int encoding,
            out int @out
        );

        /// <summary>Place an image on the page.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_insert_image(
            IntPtr doc,
            int page,
            Rect* rect,
            int alpha,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? filename,
            double height,
            int keep_proportion,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? mask,
            int oc,
            int overlay,
            IntPtr pixmap,
            int rotate,
            byte* stream,
            UIntPtr stream_len,
            double width,
            int xref,
            out int @out
        );

        /// <summary>Add a link to the page.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_insert_link(IntPtr doc, int page, IntPtr link, int mark);

        /// <summary>Insert text at a point.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_insert_text(
            IntPtr doc,
            int page,
            Point* at,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? text,
            IntPtr lines,
            UIntPtr lines_len,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? options,
            out int @out
        );

        /// <summary>Flow text into a rectangle.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_insert_textbox(
            IntPtr doc,
            int page,
            Rect* rect,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? text,
            IntPtr lines,
            UIntPtr lines_len,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? options,
            out double @out
        );

        /// <summary>Media box in points.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_mediabox(IntPtr doc, int page, Rect* @out);

        /// <summary>Open a drawing canvas on the page.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_new_shape(IntPtr doc, int page);

        /// <summary>Page box in points.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_rect(IntPtr doc, int page, Rect* @out);

        /// <summary>Page rotation in degrees.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_rotation(IntPtr doc, int page, out int @out);

        /// <summary>Search for text; rectangles or quadrilaterals.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_search_for(
            IntPtr doc,
            int page,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? needle,
            Rect* clip,
            int quads,
            int flags,
            int limit,
            int use_textpage,
            out IntPtr @out
        );

        /// <summary>Set the crop box.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_set_cropbox(IntPtr doc, int page, Rect* rect);

        /// <summary>Set the rotation.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_set_rotation(IntPtr doc, int page, int rotation);

        /// <summary>Stamp a source page onto this one.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_show_pdf_page(
            IntPtr doc,
            int page,
            Rect* rect,
            IntPtr source,
            int page_no,
            int keep_proportion,
            int overlay,
            int oc,
            int rotate,
            Rect* clip,
            out int @out
        );

        /// <summary>Form widgets on the page.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_widgets(
            IntPtr doc,
            int page,
            int* types,
            UIntPtr types_len,
            out IntPtr @out
        );

        /// <summary>Page object number.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_page_xref(IntPtr doc, int page, out int @out);

        /// <summary>Named paper size in points.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_paper_size([MarshalAs(UnmanagedType.LPUTF8Str)] string? name, out double @out, out double out2);

        /// <summary>Whether an alpha channel is present.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_pixmap_alpha(IntPtr pixmap, out int @out);

        /// <summary>Clear to a value, optionally clipped.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_pixmap_clear_with(IntPtr pixmap, int value, IRect* bbox);

        /// <summary>Colour-space code: 1 grey, 3 RGB, 4 CMYK.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_pixmap_colorspace(IntPtr pixmap, out int @out);

        /// <summary>Free a pixel buffer. Null is safe.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe void pdf_compat_pixmap_free(IntPtr handle);

        /// <summary>Height in pixels.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_pixmap_h(IntPtr pixmap, out int @out);

        /// <summary>Height in pixels.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_pixmap_height(IntPtr pixmap, out int @out);

        /// <summary>Pixel bounds.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_pixmap_irect(IntPtr pixmap, IRect* @out);

        /// <summary>Channel count, excluding alpha.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_pixmap_n(IntPtr pixmap, out int @out);

        /// <summary>Allocate a pixel buffer.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_pixmap_new(int colorspace, IRect* bounds, int alpha, out IntPtr @out);

        /// <summary>A copy of the raw samples.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_pixmap_samples(IntPtr pixmap, out IntPtr out_data, out UIntPtr out_len);

        /// <summary>Write the pixmap to a file.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_pixmap_save(IntPtr pixmap, [MarshalAs(UnmanagedType.LPUTF8Str)] string? path, [MarshalAs(UnmanagedType.LPUTF8Str)] string? output, int jpg_quality);

        /// <summary>Fill a box with a colour.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_pixmap_set_rect(IntPtr pixmap, IRect* bbox, byte* color, UIntPtr color_len);

        /// <summary>Row stride in bytes.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_pixmap_stride(IntPtr pixmap, out int @out);

        /// <summary>Encode the pixmap.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_pixmap_tobytes(
            IntPtr pixmap,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? output,
            int jpg_quality,
            out IntPtr out_data,
            out UIntPtr out_len
        );

        /// <summary>Width in pixels.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_pixmap_w(IntPtr pixmap, out int @out);

        /// <summary>Width in pixels.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_pixmap_width(IntPtr pixmap, out int @out);

        /// <summary>Placed box in points.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_placed_image_list_bbox(IntPtr list, int index, Rect* @out);

        /// <summary>Bits per component.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_placed_image_list_bits(IntPtr list, int index, out int @out);

        /// <summary>Colour-space name.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_placed_image_list_colorspace_name(IntPtr list, int index, out IntPtr @out);

        /// <summary>Snapshot count.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_placed_image_list_count(IntPtr list, out int @out);

        /// <summary>Digest bytes, or empty.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_placed_image_list_digest(IntPtr list, int index, out IntPtr out_data, out UIntPtr out_len);

        /// <summary>Free a snapshot list. Null is safe.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe void pdf_compat_placed_image_list_free(IntPtr list);

        /// <summary>Whether a digest is present.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_placed_image_list_has_digest(IntPtr list, int index, out int @out);

        /// <summary>Whether masked.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_placed_image_list_has_mask(IntPtr list, int index, out int @out);

        /// <summary>Height in pixels.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_placed_image_list_height(IntPtr list, int index, out int @out);

        /// <summary>Sequence number.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_placed_image_list_number(IntPtr list, int index, out int @out);

        /// <summary>Payload size in bytes.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_placed_image_list_size(IntPtr list, int index, out int @out);

        /// <summary>Image-to-page matrix.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_placed_image_list_transform(IntPtr list, int index, Matrix* @out);

        /// <summary>Width in pixels.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_placed_image_list_width(IntPtr list, int index, out int @out);

        /// <summary>Horizontal resolution.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_placed_image_list_xres(IntPtr list, int index, out int @out);

        /// <summary>Vertical resolution.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_placed_image_list_yres(IntPtr list, int index, out int @out);

        /// <summary>Build a point from its coordinates.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_point_new(double x, double y, Point* @out);

        /// <summary>Whether a rectangle contains another: 1 yes, 0 no.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_rect_contains(Rect* rect, Rect* other, out int @out);

        /// <summary>Rectangle area, in square points.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_rect_get_area(Rect* rect, out double @out);

        /// <summary>Rectangle height, in points (`y1 - y0`).</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_rect_get_height(Rect* rect, out double @out);

        /// <summary>Rectangle width, in points (`x1 - x0`).</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_rect_get_width(Rect* rect, out double @out);

        /// <summary>Grow a rectangle in place to include another; writes the new value back.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_rect_include_rect(Rect* rect, Rect* other, Rect* @out);

        /// <summary>Intersect a rectangle in place with another; writes the new value back.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_rect_intersect(Rect* rect, Rect* other, Rect* @out);

        /// <summary>Whether two rectangles overlap: 1 yes, 0 no.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_rect_intersects(Rect* rect, Rect* other, out int @out);

        /// <summary>Whether a rectangle is empty: 1 yes, 0 no.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_rect_is_empty(Rect* rect, out int @out);

        /// <summary>Whether a rectangle is infinite: 1 yes, 0 no.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_rect_is_infinite(Rect* rect, out int @out);

        /// <summary>Rectangle at an index.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_rect_list_at(IntPtr list, int index, Rect* @out);

        /// <summary>Snapshot count.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_rect_list_count(IntPtr list, out int @out);

        /// <summary>Free a snapshot list. Null is safe.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe void pdf_compat_rect_list_free(IntPtr list);

        /// <summary>Build a rectangle from its four edges.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_rect_new(
            double x0,
            double y0,
            double x1,
            double y1,
            Rect* @out
        );

        /// <summary>Normalize a rectangle in place; writes the new value back.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_rect_normalize(Rect* rect, Rect* @out);

        /// <summary>Hit count.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_search_hits_count(IntPtr handle, out int @out);

        /// <summary>Free search hits. Null is safe.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe void pdf_compat_search_hits_free(IntPtr handle);

        /// <summary>Whether hits are quadrilaterals (1) or rectangles (0).</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_search_hits_is_quads(IntPtr handle, out int @out);

        /// <summary>Hit quadrilateral; fails when the hits are rectangles.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_search_hits_quad_at(IntPtr handle, int index, Quad* @out);

        /// <summary>Hit rectangle; fails when the hits are quadrilaterals.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_search_hits_rect_at(IntPtr handle, int index, Rect* @out);

        /// <summary>Commit pending marks.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_shape_commit(IntPtr doc, int page, int overlay);

        /// <summary>Queue a line.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_shape_draw_line(
            IntPtr doc,
            int page,
            Point* p1,
            Point* p2,
            Point* @out
        );

        /// <summary>Queue a rectangle.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_shape_draw_rect(
            IntPtr doc,
            int page,
            Rect* rect,
            double radius,
            Point* @out
        );

        /// <summary>Stroke and fill the queued path.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_shape_finish(IntPtr doc, int page, [MarshalAs(UnmanagedType.LPUTF8Str)] string? options);

        /// <summary>Free a table finder. Null is safe.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe void pdf_compat_table_finder_free(IntPtr handle);

        /// <summary>Table box in points.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_table_finder_table_bbox(IntPtr handle, int index, Rect* @out);

        /// <summary>Table cell box in points.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_table_finder_table_cell_bbox(IntPtr handle, int index, int cell, Rect* @out);

        /// <summary>Table cell count.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_table_finder_table_cell_count(IntPtr handle, int index, out int @out);

        /// <summary>Table cell text.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_table_finder_table_cell_text(IntPtr handle, int index, int cell, out IntPtr @out);

        /// <summary>Table column count.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_table_finder_table_col_count(IntPtr handle, int index, out int @out);

        /// <summary>Table count.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_table_finder_table_count(IntPtr handle, out int @out);

        /// <summary>Table row count.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_table_finder_table_row_count(IntPtr handle, int index, out int @out);

        /// <summary>Block count for the blocks format.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_block_count(IntPtr handle, out int @out);

        /// <summary>Block number for the blocks format.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_block_number(IntPtr handle, int index, out int @out);

        /// <summary>Block box for the blocks format.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_block_rect(IntPtr handle, int index, Rect* @out);

        /// <summary>Block text for the blocks format.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_block_text(IntPtr handle, int index, out IntPtr @out);

        /// <summary>Block type for the blocks format.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_block_type(IntPtr handle, int index, out int @out);

        /// <summary>Dict block count for the dict format.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_block_count(IntPtr handle, out int @out);

        /// <summary>Dict block tag: 0 text, 1 image.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_block_tag(IntPtr handle, int index, out int @out);

        /// <summary>Dict character box.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_char_bbox(
            IntPtr handle,
            int index,
            int line,
            int span,
            int ch,
            Rect* @out
        );

        /// <summary>Dict character origin.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_char_origin(
            IntPtr handle,
            int index,
            int line,
            int span,
            int ch,
            Point* @out
        );

        /// <summary>Whether the dict character is synthetic: 1 yes, 0 no.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_char_synthetic(
            IntPtr handle,
            int index,
            int line,
            int span,
            int ch,
            out int @out
        );

        /// <summary>Dict character text (a single character).</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_char_text(
            IntPtr handle,
            int index,
            int line,
            int span,
            int ch,
            out IntPtr @out
        );

        /// <summary>Page height for the dict format.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_height(IntPtr handle, out double @out);

        /// <summary>Image dict block box.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_image_bbox(IntPtr handle, int index, Rect* @out);

        /// <summary>Image dict block height in pixels.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_image_height(IntPtr handle, int index, out int @out);

        /// <summary>Image dict block number.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_image_number(IntPtr handle, int index, out int @out);

        /// <summary>Image dict block width in pixels.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_image_width(IntPtr handle, int index, out int @out);

        /// <summary>Dict line box.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_line_bbox(IntPtr handle, int index, int line, Rect* @out);

        /// <summary>Text dict block line count.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_line_count(IntPtr handle, int index, out int @out);

        /// <summary>Dict line direction.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_line_direction(IntPtr handle, int index, int line, Point* @out);

        /// <summary>Dict line writing mode.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_line_wmode(IntPtr handle, int index, int line, out int @out);

        /// <summary>Dict span alpha.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_span_alpha(
            IntPtr handle,
            int index,
            int line,
            int span,
            out int @out
        );

        /// <summary>Dict span ascender.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_span_ascender(
            IntPtr handle,
            int index,
            int line,
            int span,
            out double @out
        );

        /// <summary>Dict span box.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_span_bbox(
            IntPtr handle,
            int index,
            int line,
            int span,
            Rect* @out
        );

        /// <summary>Dict span bidi level.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_span_bidi(
            IntPtr handle,
            int index,
            int line,
            int span,
            out int @out
        );

        /// <summary>Dict span character count (raw dicts only).</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_span_char_count(
            IntPtr handle,
            int index,
            int line,
            int span,
            out int @out
        );

        /// <summary>Dict span character flags.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_span_char_flags(
            IntPtr handle,
            int index,
            int line,
            int span,
            out int @out
        );

        /// <summary>Dict span colour.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_span_color(
            IntPtr handle,
            int index,
            int line,
            int span,
            out int @out
        );

        /// <summary>Dict line span count.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_span_count(IntPtr handle, int index, int line, out int @out);

        /// <summary>Dict span descender.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_span_descender(
            IntPtr handle,
            int index,
            int line,
            int span,
            out double @out
        );

        /// <summary>Dict span flags.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_span_flags(
            IntPtr handle,
            int index,
            int line,
            int span,
            out int @out
        );

        /// <summary>Dict span font name.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_span_font(
            IntPtr handle,
            int index,
            int line,
            int span,
            out IntPtr @out
        );

        /// <summary>Dict span origin.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_span_origin(
            IntPtr handle,
            int index,
            int line,
            int span,
            Point* @out
        );

        /// <summary>Dict span size in points.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_span_size(
            IntPtr handle,
            int index,
            int line,
            int span,
            out double @out
        );

        /// <summary>Dict span text.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_span_text(
            IntPtr handle,
            int index,
            int line,
            int span,
            out IntPtr @out
        );

        /// <summary>Text dict block box.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_text_bbox(IntPtr handle, int index, Rect* @out);

        /// <summary>Text dict block flags.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_text_flags(IntPtr handle, int index, out int @out);

        /// <summary>Text dict block number.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_text_number(IntPtr handle, int index, out int @out);

        /// <summary>Page width for the dict format.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_dict_width(IntPtr handle, out double @out);

        /// <summary>Result format tag: 0 text, 1 words, 2 blocks, 3 dict, 4 json, 5 raw json,</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_format(IntPtr handle, out int @out);

        /// <summary>Free a text result. Null is safe.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe void pdf_compat_text_output_free(IntPtr handle);

        /// <summary>Result text for the string formats (text, json, raw json, html, xhtml, xml).</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_text(IntPtr handle, out IntPtr @out);

        /// <summary>Word block number for the words format.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_word_block(IntPtr handle, int index, out int @out);

        /// <summary>Word count for the words format.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_word_count(IntPtr handle, out int @out);

        /// <summary>Word line number for the words format.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_word_line(IntPtr handle, int index, out int @out);

        /// <summary>Word number for the words format.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_word_number(IntPtr handle, int index, out int @out);

        /// <summary>Word box for the words format.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_word_rect(IntPtr handle, int index, Rect* @out);

        /// <summary>Word text for the words format.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_output_word_text(IntPtr handle, int index, out IntPtr @out);

        /// <summary>Ascender height.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_trace_list_ascender(IntPtr list, int index, out double @out);

        /// <summary>Span box in points.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_trace_list_bbox(IntPtr list, int index, Rect* @out);

        /// <summary>Writing direction.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_trace_list_bidi_dir(IntPtr list, int index, out int @out);

        /// <summary>Bidi level.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_trace_list_bidi_level(IntPtr list, int index, out int @out);

        /// <summary>Glyph box.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_trace_list_char_bbox(IntPtr list, int index, int sub, Rect* @out);

        /// <summary>Character codepoint.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_trace_list_char_codepoint(IntPtr list, int index, int sub, out int @out);

        /// <summary>Character count.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_trace_list_char_count(IntPtr list, int index, out int @out);

        /// <summary>Glyph id.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_trace_list_char_glyph(IntPtr list, int index, int sub, out int @out);

        /// <summary>Glyph origin.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_trace_list_char_origin(IntPtr list, int index, int sub, Point* @out);

        /// <summary>Snapshot count.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_trace_list_count(IntPtr list, out int @out);

        /// <summary>Free a snapshot list. Null is safe.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe void pdf_compat_text_trace_list_free(IntPtr list);

        /// <summary>Append text; returns the used box and end point.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_writer_append(
            IntPtr writer,
            Point* pos,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? text,
            IntPtr font,
            double fontsize,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? language,
            int right_to_left,
            int small_caps,
            Rect* @out,
            Point* out2
        );

        /// <summary>Free a staged-text writer. Null is safe.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe void pdf_compat_text_writer_free(IntPtr handle);

        /// <summary>Stage text for a page rectangle.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_writer_new(
            Rect* rect,
            double opacity,
            double* color,
            UIntPtr color_len,
            out IntPtr @out
        );

        /// <summary>Flush staged text onto a page.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_text_writer_write_text(
            IntPtr writer,
            IntPtr doc,
            int page,
            double* color,
            UIntPtr color_len,
            double opacity,
            int overlay,
            double* morph,
            UIntPtr morph_len,
            Matrix* matrix,
            int render_mode,
            int oc
        );

        /// <summary>Free an outline builder. Null is safe.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe void pdf_compat_toc_builder_free(IntPtr builder);

        /// <summary>Start an empty outline builder.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_toc_builder_new(out IntPtr @out);

        /// <summary>Push one outline entry: level, title, 1-based page, and destination.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_toc_builder_push(
            IntPtr builder,
            int level,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string? title,
            int page,
            int has_dest,
            int dest_kind,
            int dest_page,
            double dest_x,
            double dest_y,
            double dest_zoom
        );

        /// <summary>Snapshot count.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_toc_list_count(IntPtr list, out int @out);

        /// <summary>Destination kind, or -1.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_toc_list_dest_kind(IntPtr list, int index, out int @out);

        /// <summary>Destination page, or -1.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_toc_list_dest_page(IntPtr list, int index, out int @out);

        /// <summary>Destination point, or the origin.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_toc_list_dest_target(IntPtr list, int index, Point* @out);

        /// <summary>Destination zoom, or 0.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_toc_list_dest_zoom(IntPtr list, int index, out double @out);

        /// <summary>Free a snapshot list. Null is safe.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe void pdf_compat_toc_list_free(IntPtr list);

        /// <summary>Whether a follow-on destination exists.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_toc_list_has_dest(IntPtr list, int index, out int @out);

        /// <summary>Nesting level, from 1.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_toc_list_level(IntPtr list, int index, out int @out);

        /// <summary>1-based destination page.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_toc_list_page(IntPtr list, int index, out int @out);

        /// <summary>Entry title.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_toc_list_title(IntPtr list, int index, out IntPtr @out);

        /// <summary>Widget field name.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_widget_field_name(IntPtr widget, out IntPtr @out);

        /// <summary>Widget field-type code.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_widget_field_type(IntPtr widget, out int @out);

        /// <summary>Widget field value text.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_widget_field_value(IntPtr widget, out IntPtr @out);

        /// <summary>Free a widget. Null is safe.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe void pdf_compat_widget_free(IntPtr handle);

        /// <summary>Clone one entry into a fresh handle.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_widget_list_at(IntPtr list, int index, out IntPtr @out);

        /// <summary>Snapshot count.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_widget_list_count(IntPtr list, out int @out);

        /// <summary>Free a snapshot list. Null is safe.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe void pdf_compat_widget_list_free(IntPtr list);

        /// <summary>Build a blank widget.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_widget_new(out IntPtr @out);

        /// <summary>Widget box in points.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_widget_rect(IntPtr widget, Rect* @out);

        /// <summary>Set the widget field name.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_widget_set_field_name(IntPtr widget, [MarshalAs(UnmanagedType.LPUTF8Str)] string? value);

        /// <summary>Set the widget field-type code.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_widget_set_field_type(IntPtr widget, int value);

        /// <summary>Set the widget field value text.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_widget_set_field_value(IntPtr widget, [MarshalAs(UnmanagedType.LPUTF8Str)] string? value);

        /// <summary>Set the widget box.</summary>
        [DllImport(LibName, CallingConvention = CallingConvention.Cdecl)]
        internal static extern unsafe PdfStatus pdf_compat_widget_set_rect(IntPtr widget, Rect* value);
    }
}
