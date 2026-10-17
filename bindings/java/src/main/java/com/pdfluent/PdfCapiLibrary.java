package com.pdfluent;

import com.sun.jna.Library;
import com.sun.jna.NativeLong;
import com.sun.jna.Pointer;
import com.sun.jna.Structure;
import com.sun.jna.ptr.DoubleByReference;
import com.sun.jna.ptr.IntByReference;
import com.sun.jna.ptr.LongByReference;
import com.sun.jna.ptr.NativeLongByReference;
import com.sun.jna.ptr.PointerByReference;

import java.util.Arrays;
import java.util.List;

/**
 * JNA mapping for the PDFluent C ABI ({@code libpdf_capi}).
 *
 * <p>Functions documented here mirror declarations in {@code pdf_capi.h}.
 * Memory ownership: pointers returned from this library are owned by the
 * library and must be freed with the appropriate {@code *_free} call.
 */
interface PdfCapiLibrary extends Library {

    // ---- Library lifecycle ---------------------------------------------------

    /** Library version string (static; do not free). */
    String pdf_version();

    /** Initialise the library; returns 0 (OK). Currently a no-op. */
    int pdf_init();

    /** Shut down the library. Currently a no-op. */
    void pdf_destroy();

    // ---- Document lifecycle -------------------------------------------------

    /**
     * Open a PDF from raw bytes.
     * On success writes a handle to {@code out}; returns 0 (OK) or non-zero status.
     */
    int pdf_document_open_from_bytes(byte[] data, NativeLong len, PointerByReference out);

    /**
     * Open a PDF from a null-terminated UTF-8 file path.
     * {@code password} may be null.
     */
    int pdf_document_open(byte[] path, Pointer password, PointerByReference out);

    /** Free a document handle. Null is safe. */
    void pdf_document_free(Pointer doc);

    // ---- Document queries ---------------------------------------------------

    /** Number of pages; -1 if doc is null. */
    int pdf_document_page_count(Pointer doc);

    // ---- Digital signing & signature verification ---------------------------

    /**
     * Sign a document with a PKCS#12 identity bundle, producing a new document.
     *
     * <p>C ABI: {@code PdfStatus pdf_document_sign(const PdfDocument *doc,
     * const char *pkcs12_path, const char *pkcs12_password, PdfDocument
     * **out)}. The source {@code doc} is BORROWED and not modified. On success
     * {@code out} receives a NEW heap document handle the caller must free with
     * {@link #pdf_document_free}. {@code pkcs12Password} may be {@code null} for
     * password-less bundles.
     *
     * @return 0 OK · 1 ErrorInvalidArgument · 2 ErrorFileNotFound ·
     *         9 ErrorSign.
     */
    int pdf_document_sign(Pointer doc, String pkcs12Path, String pkcs12Password, PointerByReference out);

    /** Number of signature fields in the document; -1 if {@code doc} is null. */
    int pdf_signature_count(Pointer doc);

    /**
     * Validate the digital signature at zero-based {@code index}.
     *
     * @return 1 valid · 0 invalid · -1 unknown / error (null doc or
     *         out-of-range index).
     */
    int pdf_signature_is_valid(Pointer doc, int index);

    // ---- Content ------------------------------------------------------------

    /**
     * Extract text from a page as a heap-allocated C string.
     * Caller must free with {@link #pdf_string_free(Pointer)}.
     */
    Pointer pdf_page_extract_text(Pointer doc, int pageIndex);

    /** Free a C string returned by the library. Null is safe. */
    void pdf_string_free(Pointer s);

    // ---- Error state --------------------------------------------------------

    /** Thread-local error message (static, do not free). Returns "" if no error. */
    String pdf_get_last_error();

    void pdf_clear_error();

    // ---- Structured text-block extraction ----------------------------------

    /**
     * Native layout of {@code PdfTextBlock} — must match the C struct in
     * {@code include/pdfluent.h}. Five fields: {@code (double, double,
     * double, double, const char*)}.
     *
     * <p>The {@code text} pointer points into Rust-owned memory that is
     * released together with the parent array via
     * {@link #pdf_text_blocks_free}; do <b>not</b> free it individually.
     */
    class PdfTextBlock extends Structure {
        public double x;
        public double y;
        public double width;
        public double height;
        public Pointer text;

        public PdfTextBlock() {}

        public PdfTextBlock(Pointer p) {
            super(p);
            read();
        }

        @Override
        protected List<String> getFieldOrder() {
            return Arrays.asList("x", "y", "width", "height", "text");
        }

        public static class ByReference extends PdfTextBlock implements Structure.ByReference {}
    }

    /**
     * Extract structured text blocks for a page.
     *
     * <p>On success: writes a heap-allocated PdfTextBlock array pointer
     * to {@code outBlocks} and the element count to {@code outCount}.
     * The caller MUST release the array via {@link #pdf_text_blocks_free}
     * passing the same pointer + count.
     *
     * @return 0 OK · 1 ErrorInvalidArgument · 5 ErrorPageRange · 12 ErrorExtract.
     */
    int pdf_page_extract_text_blocks(
        Pointer doc,
        int pageIndex,
        PointerByReference outBlocks,
        com.sun.jna.ptr.NativeLongByReference outCount);

    /**
     * Free an array previously returned by
     * {@link #pdf_page_extract_text_blocks}.
     *
     * <p>{@code pdf_text_blocks_free(null, 0)} is a no-op.
     */
    void pdf_text_blocks_free(Pointer blocks, NativeLong count);

    // ---- Compatible geometry values (lane L5) --------------------------------

    /** Native layout of {@code PdfCompatPoint}: two doubles. */
    class PdfCompatPoint extends Structure {
        public double x;
        public double y;

        public PdfCompatPoint() {}

        @Override
        protected List<String> getFieldOrder() {
            return Arrays.asList("x", "y");
        }
    }

    /** Native layout of {@code PdfCompatRect}: four doubles. */
    class PdfCompatRect extends Structure {
        public double x0;
        public double y0;
        public double x1;
        public double y1;

        public PdfCompatRect() {}

        @Override
        protected List<String> getFieldOrder() {
            return Arrays.asList("x0", "y0", "x1", "y1");
        }
    }

    /** Native layout of {@code PdfCompatIRect}: four ints. */
    class PdfCompatIRect extends Structure {
        public int x0;
        public int y0;
        public int x1;
        public int y1;

        public PdfCompatIRect() {}

        @Override
        protected List<String> getFieldOrder() {
            return Arrays.asList("x0", "y0", "x1", "y1");
        }
    }

    /** Native layout of {@code PdfCompatMatrix}: six doubles. */
    class PdfCompatMatrix extends Structure {
        public double a;
        public double b;
        public double c;
        public double d;
        public double e;
        public double f;

        public PdfCompatMatrix() {}

        @Override
        protected List<String> getFieldOrder() {
            return Arrays.asList("a", "b", "c", "d", "e", "f");
        }
    }

    /** Native layout of {@code PdfCompatQuad}: four inline points. */
    class PdfCompatQuad extends Structure {
        public PdfCompatPoint ul = new PdfCompatPoint();
        public PdfCompatPoint ur = new PdfCompatPoint();
        public PdfCompatPoint ll = new PdfCompatPoint();
        public PdfCompatPoint lr = new PdfCompatPoint();

        public PdfCompatQuad() {}

        @Override
        protected List<String> getFieldOrder() {
            return Arrays.asList("ul", "ur", "ll", "lr");
        }
    }

    // ---- Compatible API (lane L5) ------------------------------------------

    /** Compat failure kind behind the last error (0 when none). */
    int pdf_compat_last_error_kind();

    /** Free heap bytes or numeric arrays. */
    void pdf_bytes_free(Pointer data, NativeLong len);

    /** Free rendered pixels. */
    void pdf_pixels_free(Pointer pixels, NativeLong len);

    /** Snapshot count. */
    int pdf_compat_annot_entry_list_count(Pointer list, IntByReference out);

    /** Free a snapshot list. Null is safe. */
    void pdf_compat_annot_entry_list_free(Pointer list);

    /** Annotation box in points. */
    int pdf_compat_annot_entry_list_rect(Pointer list, int index, PdfCompatRect out);

    /** Annotation type code. */
    int pdf_compat_annot_entry_list_type_code(Pointer list, int index, IntByReference out);

    /** Annotation type name. */
    int pdf_compat_annot_entry_list_type_name(Pointer list, int index, PointerByReference out);

    /** Annotation object number. */
    int pdf_compat_annot_entry_list_xref(Pointer list, int index, IntByReference out);

    /** Set stroke and fill colours. */
    int pdf_compat_annot_set_colors(
            Pointer doc,
            int page,
            int xref,
            double[] stroke,
            NativeLong stroke_len,
            double[] fill,
            NativeLong fill_len
    );

    /** Push appearance changes. */
    int pdf_compat_annot_update(
            Pointer doc,
            int page,
            int xref,
            String blend_mode,
            double opacity,
            double fontsize,
            String fontname,
            double[] text_color,
            NativeLong text_color_len,
            double[] border_color,
            NativeLong border_color_len,
            double[] fill_color,
            NativeLong fill_color_len,
            int cross_out,
            int rotate
    );

    /** Channel count, excluding alpha. */
    int pdf_compat_colorspace_n(int colorspace, IntByReference out);

    /** Unlock with a password. */
    int pdf_compat_document_authenticate(Pointer doc, String password, IntByReference out);

    /** Flatten annotations and widgets into content. */
    int pdf_compat_document_bake(Pointer doc, int annots, int widgets);

    /** Close the document and free its handle. */
    int pdf_compat_document_close(Pointer doc);

    /** Convert a page span to PDF content. */
    int pdf_compat_document_convert_to_pdf(Pointer doc, int from_page, int to_page, int rotate, PointerByReference out_data, NativeLongByReference out_len);

    /** Font bytes and facts. */
    int pdf_compat_document_extract_font(
            Pointer doc,
            int xref,
            int info_only,
            String name,
            PointerByReference out
    );

    /** Image bytes and facts for an object number. */
    int pdf_compat_document_extract_image(Pointer doc, int xref, PointerByReference out);

    /** Reserve a fresh object number. */
    int pdf_compat_document_get_new_xref(Pointer doc, IntByReference out);

    /** Fonts referenced by a page. */
    int pdf_compat_document_get_page_fonts(Pointer doc, int page, int full, PointerByReference out);

    /** Images referenced by a page. */
    int pdf_compat_document_get_page_images(Pointer doc, int page, int full, PointerByReference out);

    /** Outline entries. */
    int pdf_compat_document_get_toc(Pointer doc, int simple, PointerByReference out);

    /** Insert pages from another document. */
    int pdf_compat_document_insert_pdf(
            Pointer doc,
            Pointer source,
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

    /** Whether this is a form PDF. */
    int pdf_compat_document_is_form_pdf(Pointer doc, IntByReference out);

    /** Whether this is a PDF document. */
    int pdf_compat_document_is_pdf(Pointer doc, IntByReference out);

    /** Undo/redo availability of the compatible journal. */
    int pdf_compat_document_journal_can_do(Pointer doc, IntByReference canUndo, IntByReference canRedo);

    /** Enable the compatible journal. */
    int pdf_compat_document_journal_enable(Pointer doc);

    /** Whether the compatible journal is enabled. */
    int pdf_compat_document_journal_is_enabled(Pointer doc, IntByReference out);

    /** Load a journal file over a never-enabled journal. */
    int pdf_compat_document_journal_load(Pointer doc, String path);

    /** Journal step name. */
    int pdf_compat_document_journal_op_name(Pointer doc, int step, PointerByReference out);

    /** Journal position: the applied prefix length. */
    int pdf_compat_document_journal_position(Pointer doc, IntByReference out);

    /** Redo a journal step. */
    int pdf_compat_document_journal_redo(Pointer doc);

    /** Save the journal file. */
    int pdf_compat_document_journal_save(Pointer doc, String path);

    /** Start a journal step; the name may be null for an anonymous step. */
    int pdf_compat_document_journal_start_op(Pointer doc, String name);

    /** Stop the journal step. */
    int pdf_compat_document_journal_stop_op(Pointer doc);

    /** Undo a journal step. */
    int pdf_compat_document_journal_undo(Pointer doc);

    /** Validate a page number; returns it. */
    int pdf_compat_document_load_page(Pointer doc, int page, IntByReference out);

    /** Whether the document needs a password. */
    int pdf_compat_document_needs_pass(Pointer doc, IntByReference out);

    /** Insert a fresh page; returns its number. */
    int pdf_compat_document_new_page(
            Pointer doc,
            int at,
            double width,
            double height,
            IntByReference out
    );

    /** Open a file, bytes, or a fresh shell. */
    int pdf_compat_document_open(
            String filename,
            byte[] data,
            NativeLong data_len,
            String filetype,
            PdfCompatRect rect,
            double width,
            double height,
            double fontsize,
            String archive,
            PointerByReference out
    );

    /** Number of pages. Binds PdfDocument::len; the spec names no page_count (reported). */
    int pdf_compat_document_page_count(Pointer doc, IntByReference out);

    /** The document catalog. */
    int pdf_compat_document_pdf_catalog(Pointer doc, PointerByReference out);

    /** Save to a file. */
    int pdf_compat_document_save(Pointer doc, String path, String settings);

    /** Save incrementally. */
    int pdf_compat_document_save_incr(Pointer doc);

    /** Replace the document metadata. */
    int pdf_compat_document_set_metadata(
            Pointer doc,
            String title,
            String author,
            String subject,
            String keywords,
            String creator,
            String producer,
            String creation_date,
            String mod_date,
            String format,
            String encryption,
            String trapped
    );

    /** Replace the outline; returns its size. */
    int pdf_compat_document_set_toc(Pointer doc, Pointer toc, int collapse, IntByReference out);

    /** Serialise the document to bytes. */
    int pdf_compat_document_tobytes(Pointer doc, String settings, PointerByReference out_data, NativeLongByReference out_len);

    /** Replace one raw object. */
    int pdf_compat_document_update_object(Pointer doc, int xref, String text, int page);

    /** Replace one stream payload. */
    int pdf_compat_document_update_stream(
            Pointer doc,
            int xref,
            byte[] data,
            NativeLong data_len,
            int is_new,
            int compress
    );

    /** Write the document to bytes. */
    int pdf_compat_document_write(Pointer doc, String settings, PointerByReference out_data, NativeLongByReference out_len);

    /** One object key: value and type. */
    int pdf_compat_document_xref_get_key(
            Pointer doc,
            int xref,
            String key,
            PointerByReference out,
            PointerByReference out2
    );

    /** Object count. */
    int pdf_compat_document_xref_length(Pointer doc, IntByReference out);

    /** One raw object. */
    int pdf_compat_document_xref_object(
            Pointer doc,
            int xref,
            int compressed,
            int ascii,
            PointerByReference out
    );

    /** Set one object key. */
    int pdf_compat_document_xref_set_key(Pointer doc, int xref, String key, String value);

    /** One stream payload. */
    int pdf_compat_document_xref_stream(Pointer doc, int xref, PointerByReference out_data, NativeLongByReference out_len);

    /** Close-path flag, if any. */
    int pdf_compat_drawing_list_close_path(Pointer list, int index, IntByReference out_has, IntByReference out);

    /** Stroke colour, or empty. */
    int pdf_compat_drawing_list_color(
            Pointer list,
            int index,
            IntByReference out_has,
            PointerByReference out_data,
            NativeLongByReference out_len
    );

    /** Snapshot count. */
    int pdf_compat_drawing_list_count(Pointer list, IntByReference out);

    /** Dash pattern, if any. */
    int pdf_compat_drawing_list_dashes(Pointer list, int index, IntByReference out_has, PointerByReference out);

    /** Drawing operator name. */
    int pdf_compat_drawing_list_drawing_type(Pointer list, int index, PointerByReference out);

    /** Even-odd fill rule. */
    int pdf_compat_drawing_list_even_odd(Pointer list, int index, IntByReference out);

    /** Fill colour, or empty. */
    int pdf_compat_drawing_list_fill(
            Pointer list,
            int index,
            IntByReference out_has,
            PointerByReference out_data,
            NativeLongByReference out_len
    );

    /** Fill opacity, if any. */
    int pdf_compat_drawing_list_fill_opacity(Pointer list, int index, IntByReference out_has, DoubleByReference out);

    /** Free a snapshot list. Null is safe. */
    void pdf_compat_drawing_list_free(Pointer list);

    /** Path-item count. */
    int pdf_compat_drawing_list_item_count(Pointer list, int index, IntByReference out);

    /** Path-item operator. */
    int pdf_compat_drawing_list_item_op(Pointer list, int index, int sub, PointerByReference out);

    /** Path-item parameters as JSON. */
    int pdf_compat_drawing_list_item_params(Pointer list, int index, int sub, PointerByReference out);

    /** Layer name. */
    int pdf_compat_drawing_list_layer(Pointer list, int index, PointerByReference out);

    /** Nesting level, if any. */
    int pdf_compat_drawing_list_level(Pointer list, int index, IntByReference out_has, IntByReference out);

    /** Line-cap values, or empty. */
    int pdf_compat_drawing_list_line_cap(
            Pointer list,
            int index,
            IntByReference out_has,
            PointerByReference out_data,
            NativeLongByReference out_len
    );

    /** Line-join value, if any. */
    int pdf_compat_drawing_list_line_join(Pointer list, int index, IntByReference out_has, IntByReference out);

    /** Drawing box in points. */
    int pdf_compat_drawing_list_rect(Pointer list, int index, PdfCompatRect out);

    /** Sequence number. */
    int pdf_compat_drawing_list_seqno(Pointer list, int index, IntByReference out);

    /** Stroke opacity, if any. */
    int pdf_compat_drawing_list_stroke_opacity(Pointer list, int index, IntByReference out_has, DoubleByReference out);

    /** Stroke width, if any. */
    int pdf_compat_drawing_list_width(Pointer list, int index, IntByReference out_has, DoubleByReference out);

    /** Font bytes. */
    int pdf_compat_font_data_data(Pointer handle, PointerByReference out_data, NativeLongByReference out_len);

    /** Free a snapshot list. Null is safe. */
    void pdf_compat_font_data_free(Pointer list);

    /** Font name. */
    int pdf_compat_font_data_name(Pointer handle, PointerByReference out);

    /** Free a font. Null is safe. */
    void pdf_compat_font_free(Pointer handle);

    /** Snapshot count. */
    int pdf_compat_font_info_list_count(Pointer list, IntByReference out);

    /** Free a snapshot list. Null is safe. */
    void pdf_compat_font_info_list_free(Pointer list);

    /** Font name. */
    int pdf_compat_font_info_list_name(Pointer list, int index, PointerByReference out);

    /** Font object number. */
    int pdf_compat_font_info_list_xref(Pointer list, int index, IntByReference out);

    /** Build a font from a name, file, or buffer. */
    int pdf_compat_font_new(
            String fontname,
            String fontfile,
            byte[] fontbuffer,
            NativeLong fontbuffer_len,
            int script,
            String language,
            int ordering,
            int is_bold,
            int is_italic,
            int is_serif,
            int embed,
            PointerByReference out
    );

    /** Typeset width of a string. */
    int pdf_compat_font_text_length(
            Pointer font,
            String text,
            double fontsize,
            String language,
            int script,
            int wmode,
            int small_caps,
            int fallback,
            DoubleByReference out
    );

    /** Typeset width of a string in a base font. */
    int pdf_compat_get_text_length(
            String text,
            String fontname,
            double fontsize,
            int encoding,
            DoubleByReference out
    );

    /** Bits per component. */
    int pdf_compat_image_data_bits(Pointer handle, IntByReference out);

    /** Colour-space code. */
    int pdf_compat_image_data_colorspace(Pointer handle, IntByReference out);

    /** Colour-space name. */
    int pdf_compat_image_data_colorspace_name(Pointer handle, PointerByReference out);

    /** Raw payload bytes. */
    int pdf_compat_image_data_data(Pointer handle, PointerByReference out_data, NativeLongByReference out_len);

    /** File extension. */
    int pdf_compat_image_data_ext(Pointer handle, PointerByReference out);

    /** Free a snapshot list. Null is safe. */
    void pdf_compat_image_data_free(Pointer list);

    /** Height in pixels. */
    int pdf_compat_image_data_height(Pointer handle, IntByReference out);

    /** Whether masked. */
    int pdf_compat_image_data_masked(Pointer handle, IntByReference out);

    /** Payload size in bytes. */
    int pdf_compat_image_data_size(Pointer handle, IntByReference out);

    /** Width in pixels. */
    int pdf_compat_image_data_width(Pointer handle, IntByReference out);

    /** Horizontal resolution. */
    int pdf_compat_image_data_xres(Pointer handle, IntByReference out);

    /** Vertical resolution. */
    int pdf_compat_image_data_yres(Pointer handle, IntByReference out);

    /** Alternate name. */
    int pdf_compat_image_ref_list_alt(Pointer list, int index, PointerByReference out);

    /** Bits per component. */
    int pdf_compat_image_ref_list_bits(Pointer list, int index, IntByReference out);

    /** Colour-space name. */
    int pdf_compat_image_ref_list_colorspace_name(Pointer list, int index, PointerByReference out);

    /** Snapshot count. */
    int pdf_compat_image_ref_list_count(Pointer list, IntByReference out);

    /** Filter name. */
    int pdf_compat_image_ref_list_filter(Pointer list, int index, PointerByReference out);

    /** Free a snapshot list. Null is safe. */
    void pdf_compat_image_ref_list_free(Pointer list);

    /** Height in pixels. */
    int pdf_compat_image_ref_list_height(Pointer list, int index, IntByReference out);

    /** Mask object number. */
    int pdf_compat_image_ref_list_mask(Pointer list, int index, IntByReference out);

    /** Resource name. */
    int pdf_compat_image_ref_list_name(Pointer list, int index, PointerByReference out);

    /** Width in pixels. */
    int pdf_compat_image_ref_list_width(Pointer list, int index, IntByReference out);

    /** Image object number. */
    int pdf_compat_image_ref_list_xref(Pointer list, int index, IntByReference out);

    /** Build an integer rectangle from its four edges. */
    int pdf_compat_irect_new(
            int x0,
            int y0,
            int x1,
            int y1,
            PdfCompatIRect out
    );

    /** Link border cloud effect level. */
    int pdf_compat_link_border_clouds(Pointer link, IntByReference out);

    /** Link border dashes: element count plus heap ints, freed with `pdf_bytes_free`. */
    int pdf_compat_link_border_dashes(Pointer link, PointerByReference out_data, NativeLongByReference out_len);

    /** Link border style, if any: `has` says whether one exists. */
    int pdf_compat_link_border_style(Pointer link, IntByReference out_has, PointerByReference out_style);

    /** Link border width in points. */
    int pdf_compat_link_border_width(Pointer link, DoubleByReference out);

    /** Clear the follow-on destination. */
    int pdf_compat_link_clear_dest(Pointer link);

    /** Follow-on destination page, or -1 when there is none. */
    int pdf_compat_link_dest_page(Pointer link, IntByReference out);

    /** Follow-on destination point, or the origin when there is none. */
    int pdf_compat_link_dest_target(Pointer link, PdfCompatPoint out);

    /** Follow-on destination zoom, or 0 when there is none. */
    int pdf_compat_link_dest_zoom(Pointer link, DoubleByReference out);

    /** Link fill colour: element count plus heap doubles, freed with `pdf_bytes_free`. */
    int pdf_compat_link_fill(Pointer link, PointerByReference out_data, NativeLongByReference out_len);

    /** Link flags. */
    int pdf_compat_link_flags(Pointer link, IntByReference out);

    /** Free a link. Null is safe. */
    void pdf_compat_link_free(Pointer handle);

    /** Whether the link carries a follow-on destination: 1 yes, 0 no. */
    int pdf_compat_link_has_dest(Pointer link, IntByReference out);

    /** Whether the link chains to another: 1 yes, 0 no. */
    int pdf_compat_link_has_next(Pointer link, IntByReference out);

    /** Link identifier. */
    int pdf_compat_link_id(Pointer link, PointerByReference out);

    /** Link kind code. */
    int pdf_compat_link_kind(Pointer link, IntByReference out);

    /** Clone one entry into a fresh handle. */
    int pdf_compat_link_list_at(Pointer list, int index, PointerByReference out);

    /** Snapshot count. */
    int pdf_compat_link_list_count(Pointer list, IntByReference out);

    /** Free a snapshot list. Null is safe. */
    void pdf_compat_link_list_free(Pointer list);

    /** Build a blank link. */
    int pdf_compat_link_new(PointerByReference out);

    /** Clone the chained link into a fresh handle, freed with `pdf_compat_link_free`. */
    int pdf_compat_link_next(Pointer link, PointerByReference out);

    /** Link box in points. */
    int pdf_compat_link_rect(Pointer link, PdfCompatRect out);

    /** Set the link border cloud effect level. */
    int pdf_compat_link_set_border_clouds(Pointer link, int value);

    /** Set the link border dashes. */
    int pdf_compat_link_set_border_dashes(Pointer link, int[] values, NativeLong len);

    /** Set the link border style; null clears it. */
    int pdf_compat_link_set_border_style(Pointer link, String value);

    /** Set the link border width. */
    int pdf_compat_link_set_border_width(Pointer link, double value);

    /** Set the link fill colour. */
    int pdf_compat_link_set_fill(Pointer link, double[] values, NativeLong len);

    /** Set the link flags. */
    int pdf_compat_link_set_flags(Pointer link, int value);

    /** Set the link identifier. */
    int pdf_compat_link_set_id(Pointer link, String value);

    /** Set the link kind code. */
    int pdf_compat_link_set_kind(Pointer link, int value);

    /** Chain another link; null clears the chain. */
    int pdf_compat_link_set_next(Pointer link, Pointer next);

    /** Set the link box. */
    int pdf_compat_link_set_rect(Pointer link, PdfCompatRect value);

    /** Set the link stroke colour. */
    int pdf_compat_link_set_stroke(Pointer link, double[] values, NativeLong len);

    /** Set the link target URI. */
    int pdf_compat_link_set_uri(Pointer link, String value);

    /** Set the link object number. */
    int pdf_compat_link_set_xref(Pointer link, int value);

    /** Link stroke colour: element count plus heap doubles, freed with `pdf_bytes_free`. */
    int pdf_compat_link_stroke(Pointer link, PointerByReference out_data, NativeLongByReference out_len);

    /** Link target URI. */
    int pdf_compat_link_uri(Pointer link, PointerByReference out);

    /** Link object number. */
    int pdf_compat_link_xref(Pointer link, IntByReference out);

    /** Build a matrix from its six coefficients. */
    int pdf_compat_matrix_new(
            double a,
            double b,
            double c,
            double d,
            double e,
            double f,
            PdfCompatMatrix out
    );

    /** Rotate a matrix in place by a quarter-turn count; writes the new value. */
    int pdf_compat_matrix_prerotate(PdfCompatMatrix matrix, double theta, PdfCompatMatrix out);

    /** Highlight text; returns the annotation xref. */
    int pdf_compat_page_add_highlight_annot(
            Pointer doc,
            int page,
            PdfCompatQuad[] quads,
            NativeLong quads_len,
            PdfCompatPoint start,
            PdfCompatPoint stop,
            PdfCompatRect clip,
            IntByReference out
    );

    /** Add a rectangle annotation; returns its xref. */
    int pdf_compat_page_add_rect_annot(Pointer doc, int page, PdfCompatRect rect, IntByReference out);

    /** Mark a quadrilateral for redaction; returns its xref. */
    int pdf_compat_page_add_redact_annot(
            Pointer doc,
            int page,
            PdfCompatQuad quad,
            String text,
            String fontname,
            double fontsize,
            int align,
            double[] fill,
            NativeLong fill_len,
            double[] text_color,
            NativeLong text_color_len,
            int cross_out,
            IntByReference out
    );

    /** Add a text annotation; returns its xref. */
    int pdf_compat_page_add_text_annot(
            Pointer doc,
            int page,
            PdfCompatPoint at,
            String text,
            String icon,
            IntByReference out
    );

    /** Add a form widget; returns its xref. */
    int pdf_compat_page_add_widget(Pointer doc, int page, Pointer widget, IntByReference out);

    /** Annotations on the page. */
    int pdf_compat_page_annots(
            Pointer doc,
            int page,
            int[] types,
            NativeLong types_len,
            PointerByReference out
    );

    /** Burn pending redactions in. */
    int pdf_compat_page_apply_redactions(
            Pointer doc,
            int page,
            int images,
            int graphics,
            int text,
            IntByReference out
    );

    /** Tidy the content stream. */
    int pdf_compat_page_clean_contents(Pointer doc, int page, int sanitize);

    /** Delete an annotation. */
    int pdf_compat_page_delete_annot(
            Pointer doc,
            int page,
            int xref,
            int type_code,
            String type_name,
            PdfCompatRect rect
    );

    /** Stroke a Bezier curve. */
    int pdf_compat_page_draw_bezier(
            Pointer doc,
            int page,
            PdfCompatPoint p1,
            PdfCompatPoint p2,
            PdfCompatPoint p3,
            PdfCompatPoint p4,
            String options,
            PdfCompatPoint out
    );

    /** Stroke a circle. */
    int pdf_compat_page_draw_circle(
            Pointer doc,
            int page,
            PdfCompatPoint center,
            double radius,
            String options,
            PdfCompatPoint out
    );

    /** Stroke a line. */
    int pdf_compat_page_draw_line(
            Pointer doc,
            int page,
            PdfCompatPoint p1,
            PdfCompatPoint p2,
            String options,
            PdfCompatPoint out
    );

    /** Stroke a rectangle. */
    int pdf_compat_page_draw_rect(
            Pointer doc,
            int page,
            PdfCompatRect rect,
            String options,
            double radius,
            PdfCompatPoint out
    );

    /** Detect tables on the page. */
    int pdf_compat_page_find_tables(Pointer doc, int page, PointerByReference out);

    /** First annotation, if any. */
    int pdf_compat_page_first_annot(Pointer doc, int page, PointerByReference out);

    /** Content-stream object numbers. */
    int pdf_compat_page_get_contents(Pointer doc, int page, PointerByReference out_data, NativeLongByReference out_len);

    /** Vector drawings on the page. */
    int pdf_compat_page_get_drawings(Pointer doc, int page, int extended, PointerByReference out);

    /** Fonts used on the page. */
    int pdf_compat_page_get_fonts(Pointer doc, int page, int full, PointerByReference out);

    /** Placed images with geometry. */
    int pdf_compat_page_get_image_info(
            Pointer doc,
            int page,
            int hashes,
            int xrefs,
            PointerByReference out
    );

    /** Boxes where a named image appears. */
    int pdf_compat_page_get_image_rects(
            Pointer doc,
            int page,
            String name,
            int transform,
            PointerByReference out
    );

    /** Images on the page. */
    int pdf_compat_page_get_images(Pointer doc, int page, int full, PointerByReference out);

    /** Links on the page. */
    int pdf_compat_page_get_links(Pointer doc, int page, PointerByReference out);

    /** Render the page to a pixel buffer. */
    int pdf_compat_page_get_pixmap(
            Pointer doc,
            int page,
            PdfCompatMatrix matrix,
            double dpi,
            int colorspace,
            PdfCompatRect clip,
            int alpha,
            int annots,
            PointerByReference out
    );

    /** Render the page to SVG text. */
    int pdf_compat_page_get_svg_image(
            Pointer doc,
            int page,
            PdfCompatMatrix matrix,
            int text_as_path,
            PointerByReference out
    );

    /** Extract text in the named format. */
    int pdf_compat_page_get_text(
            Pointer doc,
            int page,
            String format,
            PdfCompatRect clip,
            int flags,
            int sort,
            int use_textpage,
            PointerByReference out
    );

    /** Text inside a rectangle. */
    int pdf_compat_page_get_textbox(
            Pointer doc,
            int page,
            PdfCompatRect rect,
            int use_textpage,
            PointerByReference out
    );

    /** Typeset spans with glyph origins. */
    int pdf_compat_page_get_texttrace(Pointer doc, int page, PointerByReference out);

    /** Register a font for the page. */
    int pdf_compat_page_insert_font(
            Pointer doc,
            int page,
            String fontname,
            String fontfile,
            byte[] fontbuffer,
            NativeLong fontbuffer_len,
            int set_simple,
            int wmode,
            int encoding,
            IntByReference out
    );

    /** Place an image on the page. */
    int pdf_compat_page_insert_image(
            Pointer doc,
            int page,
            PdfCompatRect rect,
            int alpha,
            String filename,
            double height,
            int keep_proportion,
            String mask,
            int oc,
            int overlay,
            Pointer pixmap,
            int rotate,
            byte[] stream,
            NativeLong stream_len,
            double width,
            int xref,
            IntByReference out
    );

    /** Add a link to the page. */
    int pdf_compat_page_insert_link(Pointer doc, int page, Pointer link, int mark);

    /** Insert text at a point. */
    int pdf_compat_page_insert_text(
            Pointer doc,
            int page,
            PdfCompatPoint at,
            String text,
            String[] lines,
            NativeLong lines_len,
            String options,
            IntByReference out
    );

    /** Flow text into a rectangle. */
    int pdf_compat_page_insert_textbox(
            Pointer doc,
            int page,
            PdfCompatRect rect,
            String text,
            String[] lines,
            NativeLong lines_len,
            String options,
            DoubleByReference out
    );

    /** Media box in points. */
    int pdf_compat_page_mediabox(Pointer doc, int page, PdfCompatRect out);

    /** Open a drawing canvas on the page. */
    int pdf_compat_page_new_shape(Pointer doc, int page);

    /** Page box in points. */
    int pdf_compat_page_rect(Pointer doc, int page, PdfCompatRect out);

    /** Page rotation in degrees. */
    int pdf_compat_page_rotation(Pointer doc, int page, IntByReference out);

    /** Search for text; rectangles or quadrilaterals. */
    int pdf_compat_page_search_for(
            Pointer doc,
            int page,
            String needle,
            PdfCompatRect clip,
            int quads,
            int flags,
            int limit,
            int use_textpage,
            PointerByReference out
    );

    /** Set the crop box. */
    int pdf_compat_page_set_cropbox(Pointer doc, int page, PdfCompatRect rect);

    /** Set the rotation. */
    int pdf_compat_page_set_rotation(Pointer doc, int page, int rotation);

    /** Stamp a source page onto this one. */
    int pdf_compat_page_show_pdf_page(
            Pointer doc,
            int page,
            PdfCompatRect rect,
            Pointer source,
            int page_no,
            int keep_proportion,
            int overlay,
            int oc,
            int rotate,
            PdfCompatRect clip,
            IntByReference out
    );

    /** Form widgets on the page. */
    int pdf_compat_page_widgets(
            Pointer doc,
            int page,
            int[] types,
            NativeLong types_len,
            PointerByReference out
    );

    /** Page object number. */
    int pdf_compat_page_xref(Pointer doc, int page, IntByReference out);

    /** Named paper size in points. */
    int pdf_compat_paper_size(String name, DoubleByReference out, DoubleByReference out2);

    /** Whether an alpha channel is present. */
    int pdf_compat_pixmap_alpha(Pointer pixmap, IntByReference out);

    /** Clear to a value, optionally clipped. */
    int pdf_compat_pixmap_clear_with(Pointer pixmap, int value, PdfCompatIRect bbox);

    /** Colour-space code: 1 grey, 3 RGB, 4 CMYK. */
    int pdf_compat_pixmap_colorspace(Pointer pixmap, IntByReference out);

    /** Free a pixel buffer. Null is safe. */
    void pdf_compat_pixmap_free(Pointer handle);

    /** Height in pixels. */
    int pdf_compat_pixmap_h(Pointer pixmap, IntByReference out);

    /** Height in pixels. */
    int pdf_compat_pixmap_height(Pointer pixmap, IntByReference out);

    /** Pixel bounds. */
    int pdf_compat_pixmap_irect(Pointer pixmap, PdfCompatIRect out);

    /** Channel count, excluding alpha. */
    int pdf_compat_pixmap_n(Pointer pixmap, IntByReference out);

    /** Allocate a pixel buffer. */
    int pdf_compat_pixmap_new(int colorspace, PdfCompatIRect bounds, int alpha, PointerByReference out);

    /** A copy of the raw samples. */
    int pdf_compat_pixmap_samples(Pointer pixmap, PointerByReference out_data, NativeLongByReference out_len);

    /** Write the pixmap to a file. */
    int pdf_compat_pixmap_save(Pointer pixmap, String path, String output, int jpg_quality);

    /** Fill a box with a colour. */
    int pdf_compat_pixmap_set_rect(Pointer pixmap, PdfCompatIRect bbox, byte[] color, NativeLong color_len);

    /** Row stride in bytes. */
    int pdf_compat_pixmap_stride(Pointer pixmap, IntByReference out);

    /** Encode the pixmap. */
    int pdf_compat_pixmap_tobytes(
            Pointer pixmap,
            String output,
            int jpg_quality,
            PointerByReference out_data,
            NativeLongByReference out_len
    );

    /** Width in pixels. */
    int pdf_compat_pixmap_w(Pointer pixmap, IntByReference out);

    /** Width in pixels. */
    int pdf_compat_pixmap_width(Pointer pixmap, IntByReference out);

    /** Placed box in points. */
    int pdf_compat_placed_image_list_bbox(Pointer list, int index, PdfCompatRect out);

    /** Bits per component. */
    int pdf_compat_placed_image_list_bits(Pointer list, int index, IntByReference out);

    /** Colour-space name. */
    int pdf_compat_placed_image_list_colorspace_name(Pointer list, int index, PointerByReference out);

    /** Snapshot count. */
    int pdf_compat_placed_image_list_count(Pointer list, IntByReference out);

    /** Digest bytes, or empty. */
    int pdf_compat_placed_image_list_digest(Pointer list, int index, PointerByReference out_data, NativeLongByReference out_len);

    /** Free a snapshot list. Null is safe. */
    void pdf_compat_placed_image_list_free(Pointer list);

    /** Whether a digest is present. */
    int pdf_compat_placed_image_list_has_digest(Pointer list, int index, IntByReference out);

    /** Whether masked. */
    int pdf_compat_placed_image_list_has_mask(Pointer list, int index, IntByReference out);

    /** Height in pixels. */
    int pdf_compat_placed_image_list_height(Pointer list, int index, IntByReference out);

    /** Sequence number. */
    int pdf_compat_placed_image_list_number(Pointer list, int index, IntByReference out);

    /** Payload size in bytes. */
    int pdf_compat_placed_image_list_size(Pointer list, int index, IntByReference out);

    /** Image-to-page matrix. */
    int pdf_compat_placed_image_list_transform(Pointer list, int index, PdfCompatMatrix out);

    /** Width in pixels. */
    int pdf_compat_placed_image_list_width(Pointer list, int index, IntByReference out);

    /** Horizontal resolution. */
    int pdf_compat_placed_image_list_xres(Pointer list, int index, IntByReference out);

    /** Vertical resolution. */
    int pdf_compat_placed_image_list_yres(Pointer list, int index, IntByReference out);

    /** Build a point from its coordinates. */
    int pdf_compat_point_new(double x, double y, PdfCompatPoint out);

    /** Whether a rectangle contains another: 1 yes, 0 no. */
    int pdf_compat_rect_contains(PdfCompatRect rect, PdfCompatRect other, IntByReference out);

    /** Rectangle area, in square points. */
    int pdf_compat_rect_get_area(PdfCompatRect rect, DoubleByReference out);

    /** Rectangle height, in points (`y1 - y0`). */
    int pdf_compat_rect_get_height(PdfCompatRect rect, DoubleByReference out);

    /** Rectangle width, in points (`x1 - x0`). */
    int pdf_compat_rect_get_width(PdfCompatRect rect, DoubleByReference out);

    /** Grow a rectangle in place to include another; writes the new value back. */
    int pdf_compat_rect_include_rect(PdfCompatRect rect, PdfCompatRect other, PdfCompatRect out);

    /** Intersect a rectangle in place with another; writes the new value back. */
    int pdf_compat_rect_intersect(PdfCompatRect rect, PdfCompatRect other, PdfCompatRect out);

    /** Whether two rectangles overlap: 1 yes, 0 no. */
    int pdf_compat_rect_intersects(PdfCompatRect rect, PdfCompatRect other, IntByReference out);

    /** Whether a rectangle is empty: 1 yes, 0 no. */
    int pdf_compat_rect_is_empty(PdfCompatRect rect, IntByReference out);

    /** Whether a rectangle is infinite: 1 yes, 0 no. */
    int pdf_compat_rect_is_infinite(PdfCompatRect rect, IntByReference out);

    /** Rectangle at an index. */
    int pdf_compat_rect_list_at(Pointer list, int index, PdfCompatRect out);

    /** Snapshot count. */
    int pdf_compat_rect_list_count(Pointer list, IntByReference out);

    /** Free a snapshot list. Null is safe. */
    void pdf_compat_rect_list_free(Pointer list);

    /** Build a rectangle from its four edges. */
    int pdf_compat_rect_new(
            double x0,
            double y0,
            double x1,
            double y1,
            PdfCompatRect out
    );

    /** Normalize a rectangle in place; writes the new value back. */
    int pdf_compat_rect_normalize(PdfCompatRect rect, PdfCompatRect out);

    /** Hit count. */
    int pdf_compat_search_hits_count(Pointer handle, IntByReference out);

    /** Free search hits. Null is safe. */
    void pdf_compat_search_hits_free(Pointer handle);

    /** Whether hits are quadrilaterals (1) or rectangles (0). */
    int pdf_compat_search_hits_is_quads(Pointer handle, IntByReference out);

    /** Hit quadrilateral; fails when the hits are rectangles. */
    int pdf_compat_search_hits_quad_at(Pointer handle, int index, PdfCompatQuad out);

    /** Hit rectangle; fails when the hits are quadrilaterals. */
    int pdf_compat_search_hits_rect_at(Pointer handle, int index, PdfCompatRect out);

    /** Commit pending marks. */
    int pdf_compat_shape_commit(Pointer doc, int page, int overlay);

    /** Queue a line. */
    int pdf_compat_shape_draw_line(
            Pointer doc,
            int page,
            PdfCompatPoint p1,
            PdfCompatPoint p2,
            PdfCompatPoint out
    );

    /** Queue a rectangle. */
    int pdf_compat_shape_draw_rect(
            Pointer doc,
            int page,
            PdfCompatRect rect,
            double radius,
            PdfCompatPoint out
    );

    /** Stroke and fill the queued path. */
    int pdf_compat_shape_finish(Pointer doc, int page, String options);

    /** Free a table finder. Null is safe. */
    void pdf_compat_table_finder_free(Pointer handle);

    /** Table box in points. */
    int pdf_compat_table_finder_table_bbox(Pointer handle, int index, PdfCompatRect out);

    /** Table cell box in points. */
    int pdf_compat_table_finder_table_cell_bbox(Pointer handle, int index, int cell, PdfCompatRect out);

    /** Table cell count. */
    int pdf_compat_table_finder_table_cell_count(Pointer handle, int index, IntByReference out);

    /** Table cell text. */
    int pdf_compat_table_finder_table_cell_text(Pointer handle, int index, int cell, PointerByReference out);

    /** Table column count. */
    int pdf_compat_table_finder_table_col_count(Pointer handle, int index, IntByReference out);

    /** Table count. */
    int pdf_compat_table_finder_table_count(Pointer handle, IntByReference out);

    /** Table row count. */
    int pdf_compat_table_finder_table_row_count(Pointer handle, int index, IntByReference out);

    /** Block count for the blocks format. */
    int pdf_compat_text_output_block_count(Pointer handle, IntByReference out);

    /** Block number for the blocks format. */
    int pdf_compat_text_output_block_number(Pointer handle, int index, IntByReference out);

    /** Block box for the blocks format. */
    int pdf_compat_text_output_block_rect(Pointer handle, int index, PdfCompatRect out);

    /** Block text for the blocks format. */
    int pdf_compat_text_output_block_text(Pointer handle, int index, PointerByReference out);

    /** Block type for the blocks format. */
    int pdf_compat_text_output_block_type(Pointer handle, int index, IntByReference out);

    /** Dict block count for the dict format. */
    int pdf_compat_text_output_dict_block_count(Pointer handle, IntByReference out);

    /** Dict block tag: 0 text, 1 image. */
    int pdf_compat_text_output_dict_block_tag(Pointer handle, int index, IntByReference out);

    /** Dict character box. */
    int pdf_compat_text_output_dict_char_bbox(
            Pointer handle,
            int index,
            int line,
            int span,
            int ch,
            PdfCompatRect out
    );

    /** Dict character origin. */
    int pdf_compat_text_output_dict_char_origin(
            Pointer handle,
            int index,
            int line,
            int span,
            int ch,
            PdfCompatPoint out
    );

    /** Whether the dict character is synthetic: 1 yes, 0 no. */
    int pdf_compat_text_output_dict_char_synthetic(
            Pointer handle,
            int index,
            int line,
            int span,
            int ch,
            IntByReference out
    );

    /** Dict character text (a single character). */
    int pdf_compat_text_output_dict_char_text(
            Pointer handle,
            int index,
            int line,
            int span,
            int ch,
            PointerByReference out
    );

    /** Page height for the dict format. */
    int pdf_compat_text_output_dict_height(Pointer handle, DoubleByReference out);

    /** Image dict block box. */
    int pdf_compat_text_output_dict_image_bbox(Pointer handle, int index, PdfCompatRect out);

    /** Image dict block height in pixels. */
    int pdf_compat_text_output_dict_image_height(Pointer handle, int index, IntByReference out);

    /** Image dict block number. */
    int pdf_compat_text_output_dict_image_number(Pointer handle, int index, IntByReference out);

    /** Image dict block width in pixels. */
    int pdf_compat_text_output_dict_image_width(Pointer handle, int index, IntByReference out);

    /** Dict line box. */
    int pdf_compat_text_output_dict_line_bbox(Pointer handle, int index, int line, PdfCompatRect out);

    /** Text dict block line count. */
    int pdf_compat_text_output_dict_line_count(Pointer handle, int index, IntByReference out);

    /** Dict line direction. */
    int pdf_compat_text_output_dict_line_direction(Pointer handle, int index, int line, PdfCompatPoint out);

    /** Dict line writing mode. */
    int pdf_compat_text_output_dict_line_wmode(Pointer handle, int index, int line, IntByReference out);

    /** Dict span alpha. */
    int pdf_compat_text_output_dict_span_alpha(
            Pointer handle,
            int index,
            int line,
            int span,
            IntByReference out
    );

    /** Dict span ascender. */
    int pdf_compat_text_output_dict_span_ascender(
            Pointer handle,
            int index,
            int line,
            int span,
            DoubleByReference out
    );

    /** Dict span box. */
    int pdf_compat_text_output_dict_span_bbox(
            Pointer handle,
            int index,
            int line,
            int span,
            PdfCompatRect out
    );

    /** Dict span bidi level. */
    int pdf_compat_text_output_dict_span_bidi(
            Pointer handle,
            int index,
            int line,
            int span,
            IntByReference out
    );

    /** Dict span character count (raw dicts only). */
    int pdf_compat_text_output_dict_span_char_count(
            Pointer handle,
            int index,
            int line,
            int span,
            IntByReference out
    );

    /** Dict span character flags. */
    int pdf_compat_text_output_dict_span_char_flags(
            Pointer handle,
            int index,
            int line,
            int span,
            IntByReference out
    );

    /** Dict span colour. */
    int pdf_compat_text_output_dict_span_color(
            Pointer handle,
            int index,
            int line,
            int span,
            IntByReference out
    );

    /** Dict line span count. */
    int pdf_compat_text_output_dict_span_count(Pointer handle, int index, int line, IntByReference out);

    /** Dict span descender. */
    int pdf_compat_text_output_dict_span_descender(
            Pointer handle,
            int index,
            int line,
            int span,
            DoubleByReference out
    );

    /** Dict span flags. */
    int pdf_compat_text_output_dict_span_flags(
            Pointer handle,
            int index,
            int line,
            int span,
            IntByReference out
    );

    /** Dict span font name. */
    int pdf_compat_text_output_dict_span_font(
            Pointer handle,
            int index,
            int line,
            int span,
            PointerByReference out
    );

    /** Dict span origin. */
    int pdf_compat_text_output_dict_span_origin(
            Pointer handle,
            int index,
            int line,
            int span,
            PdfCompatPoint out
    );

    /** Dict span size in points. */
    int pdf_compat_text_output_dict_span_size(
            Pointer handle,
            int index,
            int line,
            int span,
            DoubleByReference out
    );

    /** Dict span text. */
    int pdf_compat_text_output_dict_span_text(
            Pointer handle,
            int index,
            int line,
            int span,
            PointerByReference out
    );

    /** Text dict block box. */
    int pdf_compat_text_output_dict_text_bbox(Pointer handle, int index, PdfCompatRect out);

    /** Text dict block flags. */
    int pdf_compat_text_output_dict_text_flags(Pointer handle, int index, IntByReference out);

    /** Text dict block number. */
    int pdf_compat_text_output_dict_text_number(Pointer handle, int index, IntByReference out);

    /** Page width for the dict format. */
    int pdf_compat_text_output_dict_width(Pointer handle, DoubleByReference out);

    /** Result format tag: 0 text, 1 words, 2 blocks, 3 dict, 4 json, 5 raw json, */
    int pdf_compat_text_output_format(Pointer handle, IntByReference out);

    /** Free a text result. Null is safe. */
    void pdf_compat_text_output_free(Pointer handle);

    /** Result text for the string formats (text, json, raw json, html, xhtml, xml). */
    int pdf_compat_text_output_text(Pointer handle, PointerByReference out);

    /** Word block number for the words format. */
    int pdf_compat_text_output_word_block(Pointer handle, int index, IntByReference out);

    /** Word count for the words format. */
    int pdf_compat_text_output_word_count(Pointer handle, IntByReference out);

    /** Word line number for the words format. */
    int pdf_compat_text_output_word_line(Pointer handle, int index, IntByReference out);

    /** Word number for the words format. */
    int pdf_compat_text_output_word_number(Pointer handle, int index, IntByReference out);

    /** Word box for the words format. */
    int pdf_compat_text_output_word_rect(Pointer handle, int index, PdfCompatRect out);

    /** Word text for the words format. */
    int pdf_compat_text_output_word_text(Pointer handle, int index, PointerByReference out);

    /** Ascender height. */
    int pdf_compat_text_trace_list_ascender(Pointer list, int index, DoubleByReference out);

    /** Span box in points. */
    int pdf_compat_text_trace_list_bbox(Pointer list, int index, PdfCompatRect out);

    /** Writing direction. */
    int pdf_compat_text_trace_list_bidi_dir(Pointer list, int index, IntByReference out);

    /** Bidi level. */
    int pdf_compat_text_trace_list_bidi_level(Pointer list, int index, IntByReference out);

    /** Glyph box. */
    int pdf_compat_text_trace_list_char_bbox(Pointer list, int index, int sub, PdfCompatRect out);

    /** Character codepoint. */
    int pdf_compat_text_trace_list_char_codepoint(Pointer list, int index, int sub, IntByReference out);

    /** Character count. */
    int pdf_compat_text_trace_list_char_count(Pointer list, int index, IntByReference out);

    /** Glyph id. */
    int pdf_compat_text_trace_list_char_glyph(Pointer list, int index, int sub, IntByReference out);

    /** Glyph origin. */
    int pdf_compat_text_trace_list_char_origin(Pointer list, int index, int sub, PdfCompatPoint out);

    /** Snapshot count. */
    int pdf_compat_text_trace_list_count(Pointer list, IntByReference out);

    /** Free a snapshot list. Null is safe. */
    void pdf_compat_text_trace_list_free(Pointer list);

    /** Append text; returns the used box and end point. */
    int pdf_compat_text_writer_append(
            Pointer writer,
            PdfCompatPoint pos,
            String text,
            Pointer font,
            double fontsize,
            String language,
            int right_to_left,
            int small_caps,
            PdfCompatRect out,
            PdfCompatPoint out2
    );

    /** Free a staged-text writer. Null is safe. */
    void pdf_compat_text_writer_free(Pointer handle);

    /** Stage text for a page rectangle. */
    int pdf_compat_text_writer_new(
            PdfCompatRect rect,
            double opacity,
            double[] color,
            NativeLong color_len,
            PointerByReference out
    );

    /** Flush staged text onto a page. */
    int pdf_compat_text_writer_write_text(
            Pointer writer,
            Pointer doc,
            int page,
            double[] color,
            NativeLong color_len,
            double opacity,
            int overlay,
            double[] morph,
            NativeLong morph_len,
            PdfCompatMatrix matrix,
            int render_mode,
            int oc
    );

    /** Free an outline builder. Null is safe. */
    void pdf_compat_toc_builder_free(Pointer builder);

    /** Start an empty outline builder. */
    int pdf_compat_toc_builder_new(PointerByReference out);

    /** Push one outline entry: level, title, 1-based page, and destination. */
    int pdf_compat_toc_builder_push(
            Pointer builder,
            int level,
            String title,
            int page,
            int has_dest,
            int dest_kind,
            int dest_page,
            double dest_x,
            double dest_y,
            double dest_zoom
    );

    /** Snapshot count. */
    int pdf_compat_toc_list_count(Pointer list, IntByReference out);

    /** Destination kind, or -1. */
    int pdf_compat_toc_list_dest_kind(Pointer list, int index, IntByReference out);

    /** Destination page, or -1. */
    int pdf_compat_toc_list_dest_page(Pointer list, int index, IntByReference out);

    /** Destination point, or the origin. */
    int pdf_compat_toc_list_dest_target(Pointer list, int index, PdfCompatPoint out);

    /** Destination zoom, or 0. */
    int pdf_compat_toc_list_dest_zoom(Pointer list, int index, DoubleByReference out);

    /** Free a snapshot list. Null is safe. */
    void pdf_compat_toc_list_free(Pointer list);

    /** Whether a follow-on destination exists. */
    int pdf_compat_toc_list_has_dest(Pointer list, int index, IntByReference out);

    /** Nesting level, from 1. */
    int pdf_compat_toc_list_level(Pointer list, int index, IntByReference out);

    /** 1-based destination page. */
    int pdf_compat_toc_list_page(Pointer list, int index, IntByReference out);

    /** Entry title. */
    int pdf_compat_toc_list_title(Pointer list, int index, PointerByReference out);

    /** Widget field name. */
    int pdf_compat_widget_field_name(Pointer widget, PointerByReference out);

    /** Widget field-type code. */
    int pdf_compat_widget_field_type(Pointer widget, IntByReference out);

    /** Widget field value text. */
    int pdf_compat_widget_field_value(Pointer widget, PointerByReference out);

    /** Free a widget. Null is safe. */
    void pdf_compat_widget_free(Pointer handle);

    /** Clone one entry into a fresh handle. */
    int pdf_compat_widget_list_at(Pointer list, int index, PointerByReference out);

    /** Snapshot count. */
    int pdf_compat_widget_list_count(Pointer list, IntByReference out);

    /** Free a snapshot list. Null is safe. */
    void pdf_compat_widget_list_free(Pointer list);

    /** Build a blank widget. */
    int pdf_compat_widget_new(PointerByReference out);

    /** Widget box in points. */
    int pdf_compat_widget_rect(Pointer widget, PdfCompatRect out);

    /** Set the widget field name. */
    int pdf_compat_widget_set_field_name(Pointer widget, String value);

    /** Set the widget field-type code. */
    int pdf_compat_widget_set_field_type(Pointer widget, int value);

    /** Set the widget field value text. */
    int pdf_compat_widget_set_field_value(Pointer widget, String value);

    /** Set the widget box. */
    int pdf_compat_widget_set_rect(Pointer widget, PdfCompatRect value);
}
