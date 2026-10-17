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
    /// A compatible page: text, images, and drawing. A lightweight address of a document page; all calls delegate through the owning document.
    /// </summary>
    public sealed class Page
    {
        private readonly Document _doc;
        private readonly int _page;

        internal Document Doc => _doc;

        internal int Number => _page;

        /// <summary>
        /// Address a page of a document.
        /// </summary>
        /// <param name="document">Owning document.</param>
        /// <param name="page">0-based page number.</param>
        public Page(Document document, int page)
        {
            _doc = document;
            _page = page;
        }


        /// <summary>Highlight text; returns the annotation xref.</summary>
        public int AddHighlightAnnot(Quad[]? quads, Point? start, Point? stop, Rect? clip)
        {
            unsafe
            {
                fixed (Quad* quadsPtr = quads)
                {
                Point startTmp = start.GetValueOrDefault();
                Point* startPtr = start.HasValue ? &startTmp : null;
                Point stopTmp = stop.GetValueOrDefault();
                Point* stopPtr = stop.HasValue ? &stopTmp : null;
                Rect clipTmp = clip.GetValueOrDefault();
                Rect* clipPtr = clip.HasValue ? &clipTmp : null;
                int outValue;
                CompatSupport.Check(NativeMethods.pdf_compat_page_add_highlight_annot(_doc.RequireOpen(), _page, quadsPtr, CompatSupport.Count(quads), startPtr, stopPtr, clipPtr, out outValue), "AddHighlightAnnot");
                return outValue;
                }
            }
        }

        /// <summary>Convenience overload with defaults.</summary>
        public int AddHighlightAnnot()
        {
            return AddHighlightAnnot(null, null, null, null);
        }

        /// <summary>Add a rectangle annotation; returns its xref.</summary>
        public unsafe int AddRectAnnot(Rect rect)
        {
            Rect rectTmp = rect;
            int outValue;
            CompatSupport.Check(NativeMethods.pdf_compat_page_add_rect_annot(_doc.RequireOpen(), _page, &rectTmp, out outValue), "AddRectAnnot");
            return outValue;
        }

        /// <summary>Convenience overload with defaults.</summary>
        public unsafe int AddRedactAnnot(Quad quad)
        {
            return AddRedactAnnot(quad, new RedactAnnotOptions());
        }

        /// <summary>Add a text annotation; returns its xref.</summary>
        public unsafe int AddTextAnnot(Point at, string? text, string? icon)
        {
            Point atTmp = at;
            int outValue;
            CompatSupport.Check(NativeMethods.pdf_compat_page_add_text_annot(_doc.RequireOpen(), _page, &atTmp, text, icon, out outValue), "AddTextAnnot");
            return outValue;
        }

        /// <summary>Convenience overload with defaults.</summary>
        public unsafe int AddTextAnnot(Point at, string? text)
        {
            return AddTextAnnot(at, text, "Note");
        }

        /// <summary>Add a form widget; returns its xref.</summary>
        public int AddWidget(Widget widget)
        {
            int outValue;
            CompatSupport.Check(NativeMethods.pdf_compat_page_add_widget(_doc.RequireOpen(), _page, widget.RequireOpen(), out outValue), "AddWidget");
            return outValue;
        }

        /// <summary>Annotations on the page.</summary>
        public List<AnnotEntry> Annots(int[]? types)
        {
            unsafe
            {
                fixed (int* typesPtr = types)
                {
                IntPtr listPtr;
                CompatSupport.Check(NativeMethods.pdf_compat_page_annots(_doc.RequireOpen(), _page, typesPtr, CompatSupport.Count(types), out listPtr), "Annots");
                try
                {
                    return AnnotEntry.ReadList(listPtr);
                }
                finally
                {
                    NativeMethods.pdf_compat_annot_entry_list_free(listPtr);
                }
                }
            }
        }

        /// <summary>Convenience overload with defaults.</summary>
        public List<AnnotEntry> Annots()
        {
            return Annots(null);
        }

        /// <summary>Burn pending redactions in.</summary>
        public bool ApplyRedactions(int images, int graphics, int text)
        {
            int outValue;
            CompatSupport.Check(NativeMethods.pdf_compat_page_apply_redactions(_doc.RequireOpen(), _page, images, graphics, text, out outValue), "ApplyRedactions");
            return outValue != 0;
        }

        /// <summary>Convenience overload with defaults.</summary>
        public bool ApplyRedactions()
        {
            return ApplyRedactions(2, 1, 0);
        }

        /// <summary>Tidy the content stream.</summary>
        public void CleanContents(bool sanitize)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_page_clean_contents(_doc.RequireOpen(), _page, CompatSupport.Flag(sanitize)), "CleanContents");
        }

        /// <summary>Convenience overload with defaults.</summary>
        public void CleanContents()
        {
            CleanContents(true);
        }

        /// <summary>Delete an annotation.</summary>
        public unsafe void DeleteAnnot(int xref, int type_code, string? type_name, Rect rect)
        {
            Rect rectTmp = rect;
            CompatSupport.Check(NativeMethods.pdf_compat_page_delete_annot(_doc.RequireOpen(), _page, xref, type_code, type_name, &rectTmp), "DeleteAnnot");
        }

        /// <summary>Convenience overload with defaults.</summary>
        public unsafe void DeleteAnnot(AnnotEntry entry)
        {
            DeleteAnnot(entry.Xref, entry.TypeCode, entry.TypeName, entry.Rect);
        }

        /// <summary>Stroke a Bezier curve.</summary>
        public unsafe Point DrawBezier(Point p1, Point p2, Point p3, Point p4, DrawOptions? options)
        {
            Point p1Tmp = p1;
            Point p2Tmp = p2;
            Point p3Tmp = p3;
            Point p4Tmp = p4;
            Point outValue = default;
            CompatSupport.Check(NativeMethods.pdf_compat_page_draw_bezier(_doc.RequireOpen(), _page, &p1Tmp, &p2Tmp, &p3Tmp, &p4Tmp, options == null ? null : options.ToJson(), &outValue), "DrawBezier");
            return outValue;
        }

        /// <summary>Convenience overload with defaults.</summary>
        public unsafe Point DrawBezier(Point p1, Point p2, Point p3, Point p4)
        {
            return DrawBezier(p1, p2, p3, p4, null);
        }

        /// <summary>Stroke a circle.</summary>
        public unsafe Point DrawCircle(Point center, double radius, DrawOptions? options)
        {
            Point centerTmp = center;
            Point outValue = default;
            CompatSupport.Check(NativeMethods.pdf_compat_page_draw_circle(_doc.RequireOpen(), _page, &centerTmp, radius, options == null ? null : options.ToJson(), &outValue), "DrawCircle");
            return outValue;
        }

        /// <summary>Convenience overload with defaults.</summary>
        public unsafe Point DrawCircle(Point center, double radius)
        {
            return DrawCircle(center, radius, null);
        }

        /// <summary>Stroke a line.</summary>
        public unsafe Point DrawLine(Point p1, Point p2, DrawOptions? options)
        {
            Point p1Tmp = p1;
            Point p2Tmp = p2;
            Point outValue = default;
            CompatSupport.Check(NativeMethods.pdf_compat_page_draw_line(_doc.RequireOpen(), _page, &p1Tmp, &p2Tmp, options == null ? null : options.ToJson(), &outValue), "DrawLine");
            return outValue;
        }

        /// <summary>Convenience overload with defaults.</summary>
        public unsafe Point DrawLine(Point p1, Point p2)
        {
            return DrawLine(p1, p2, null);
        }

        /// <summary>Stroke a rectangle.</summary>
        public unsafe Point DrawRect(Rect rect, DrawOptions? options, double? radius)
        {
            Rect rectTmp = rect;
            Point outValue = default;
            CompatSupport.Check(NativeMethods.pdf_compat_page_draw_rect(_doc.RequireOpen(), _page, &rectTmp, options == null ? null : options.ToJson(), radius ?? double.NaN, &outValue), "DrawRect");
            return outValue;
        }

        /// <summary>Convenience overload with defaults.</summary>
        public unsafe Point DrawRect(Rect rect)
        {
            return DrawRect(rect, null, null);
        }

        /// <summary>Detect tables on the page.</summary>
        public TableFinder FindTables()
        {
            IntPtr hPtr;
            CompatSupport.Check(NativeMethods.pdf_compat_page_find_tables(_doc.RequireOpen(), _page, out hPtr), "FindTables");
            return TableFinder.Read(hPtr);
        }

        /// <summary>First annotation, if any.</summary>
        public List<AnnotEntry> FirstAnnot()
        {
            IntPtr listPtr;
            CompatSupport.Check(NativeMethods.pdf_compat_page_first_annot(_doc.RequireOpen(), _page, out listPtr), "FirstAnnot");
            try
            {
                return AnnotEntry.ReadList(listPtr);
            }
            finally
            {
                NativeMethods.pdf_compat_annot_entry_list_free(listPtr);
            }
        }

        /// <summary>Content-stream object numbers.</summary>
        public int[] GetContents()
        {
            IntPtr dataPtr;
            UIntPtr dataLen;
            CompatSupport.Check(NativeMethods.pdf_compat_page_get_contents(_doc.RequireOpen(), _page, out dataPtr, out dataLen), "GetContents");
            return CompatSupport.ReadInts(dataPtr, dataLen);
        }

        /// <summary>Vector drawings on the page.</summary>
        public List<Drawing> GetDrawings(bool extended)
        {
            IntPtr listPtr;
            CompatSupport.Check(NativeMethods.pdf_compat_page_get_drawings(_doc.RequireOpen(), _page, CompatSupport.Flag(extended), out listPtr), "GetDrawings");
            try
            {
                return Drawing.ReadList(listPtr);
            }
            finally
            {
                NativeMethods.pdf_compat_drawing_list_free(listPtr);
            }
        }

        /// <summary>Convenience overload with defaults.</summary>
        public List<Drawing> GetDrawings()
        {
            return GetDrawings(false);
        }

        /// <summary>Fonts used on the page.</summary>
        public List<FontInfo> GetFonts(bool full)
        {
            IntPtr listPtr;
            CompatSupport.Check(NativeMethods.pdf_compat_page_get_fonts(_doc.RequireOpen(), _page, CompatSupport.Flag(full), out listPtr), "GetFonts");
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
        public List<FontInfo> GetFonts()
        {
            return GetFonts(false);
        }

        /// <summary>Placed images with geometry.</summary>
        public List<PlacedImage> GetImageInfo(bool hashes, bool xrefs)
        {
            IntPtr listPtr;
            CompatSupport.Check(NativeMethods.pdf_compat_page_get_image_info(_doc.RequireOpen(), _page, CompatSupport.Flag(hashes), CompatSupport.Flag(xrefs), out listPtr), "GetImageInfo");
            try
            {
                return PlacedImage.ReadList(listPtr);
            }
            finally
            {
                NativeMethods.pdf_compat_placed_image_list_free(listPtr);
            }
        }

        /// <summary>Convenience overload with defaults.</summary>
        public List<PlacedImage> GetImageInfo()
        {
            return GetImageInfo(false, false);
        }

        /// <summary>Boxes where a named image appears.</summary>
        public List<Rect> GetImageRects(string? name, bool transform)
        {
            IntPtr listPtr;
            CompatSupport.Check(NativeMethods.pdf_compat_page_get_image_rects(_doc.RequireOpen(), _page, name, CompatSupport.Flag(transform), out listPtr), "GetImageRects");
            {
                CompatSupport.Check(
                NativeMethods.pdf_compat_rect_list_count(listPtr, out int n), "GetImageRects");
                var result = new List<Rect>(n);
                for (int i = 0; i < n; i++)
                {
                    Rect r = default;
                    unsafe
                    {
                        CompatSupport.Check(NativeMethods.pdf_compat_rect_list_at(listPtr, i, &r), "GetImageRects");
                    }
                    result.Add(r);
                }
                NativeMethods.pdf_compat_rect_list_free(listPtr);
                return result;
            }
        }

        /// <summary>Convenience overload with defaults.</summary>
        public List<Rect> GetImageRects(string? name)
        {
            return GetImageRects(name, false);
        }

        /// <summary>Images on the page.</summary>
        public List<ImageRef> GetImages(bool full)
        {
            IntPtr listPtr;
            CompatSupport.Check(NativeMethods.pdf_compat_page_get_images(_doc.RequireOpen(), _page, CompatSupport.Flag(full), out listPtr), "GetImages");
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
        public List<ImageRef> GetImages()
        {
            return GetImages(false);
        }

        /// <summary>Links on the page.</summary>
        public List<Link> GetLinks()
        {
            IntPtr listPtr;
            CompatSupport.Check(NativeMethods.pdf_compat_page_get_links(_doc.RequireOpen(), _page, out listPtr), "GetLinks");
            try
            {
                return Link.ReadList(listPtr);
            }
            finally
            {
                NativeMethods.pdf_compat_link_list_free(listPtr);
            }
        }

        /// <summary>Convenience overload with defaults.</summary>
        public unsafe Pixmap GetPixmap()
        {
            return GetPixmap(null);
        }

        /// <summary>Render the page to SVG text.</summary>
        public unsafe string GetSvgImage(Matrix? matrix, bool text_as_path)
        {
            Matrix matrixTmp = matrix.GetValueOrDefault();
            Matrix* matrixPtr = matrix.HasValue ? &matrixTmp : null;
            IntPtr sPtr;
            CompatSupport.Check(NativeMethods.pdf_compat_page_get_svg_image(_doc.RequireOpen(), _page, matrixPtr, CompatSupport.Flag(text_as_path), out sPtr), "GetSvgImage");
            return CompatSupport.ReadString(sPtr);
        }

        /// <summary>Convenience overload with defaults.</summary>
        public unsafe string GetSvgImage()
        {
            return GetSvgImage(null, true);
        }

        /// <summary>Extract text in the named format.</summary>
        public unsafe TextOutput GetText(string? format, Rect? clip, int? flags, bool sort, bool use_textpage)
        {
            Rect clipTmp = clip.GetValueOrDefault();
            Rect* clipPtr = clip.HasValue ? &clipTmp : null;
            IntPtr hPtr;
            CompatSupport.Check(NativeMethods.pdf_compat_page_get_text(_doc.RequireOpen(), _page, format, clipPtr, flags ?? -1, CompatSupport.Flag(sort), CompatSupport.Flag(use_textpage), out hPtr), "GetText");
            return TextOutput.Read(hPtr);
        }

        /// <summary>Convenience overload with defaults.</summary>
        public unsafe TextOutput GetText()
        {
            return GetText("text", null, null, false, false);
        }

        /// <summary>Convenience overload with defaults.</summary>
        public unsafe TextOutput GetText(string? format)
        {
            return GetText(format, null, null, false, false);
        }

        /// <summary>Text inside a rectangle.</summary>
        public unsafe string GetTextbox(Rect rect, bool use_textpage)
        {
            Rect rectTmp = rect;
            IntPtr sPtr;
            CompatSupport.Check(NativeMethods.pdf_compat_page_get_textbox(_doc.RequireOpen(), _page, &rectTmp, CompatSupport.Flag(use_textpage), out sPtr), "GetTextbox");
            return CompatSupport.ReadString(sPtr);
        }

        /// <summary>Convenience overload with defaults.</summary>
        public unsafe string GetTextbox(Rect rect)
        {
            return GetTextbox(rect, false);
        }

        /// <summary>Typeset spans with glyph origins.</summary>
        public List<TextTrace> GetTexttrace()
        {
            IntPtr listPtr;
            CompatSupport.Check(NativeMethods.pdf_compat_page_get_texttrace(_doc.RequireOpen(), _page, out listPtr), "GetTexttrace");
            try
            {
                return TextTrace.ReadList(listPtr);
            }
            finally
            {
                NativeMethods.pdf_compat_text_trace_list_free(listPtr);
            }
        }

        /// <summary>Convenience overload with defaults.</summary>
        public unsafe int InsertFont()
        {
            return InsertFont(new InsertFontOptions());
        }

        /// <summary>Convenience overload with defaults.</summary>
        public unsafe int InsertImage(Rect rect)
        {
            return InsertImage(rect, new ImageInsertOptions());
        }

        /// <summary>Add a link to the page.</summary>
        public void InsertLink(Link link, bool mark)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_page_insert_link(_doc.RequireOpen(), _page, link.RequireOpen(), CompatSupport.Flag(mark)), "InsertLink");
        }

        /// <summary>Convenience overload with defaults.</summary>
        public void InsertLink(Link link)
        {
            InsertLink(link, true);
        }

        /// <summary>Media box in points.</summary>
        public unsafe Rect Mediabox()
        {
            Rect outValue = default;
            CompatSupport.Check(NativeMethods.pdf_compat_page_mediabox(_doc.RequireOpen(), _page, &outValue), "Mediabox");
            return outValue;
        }

        /// <summary>Open a drawing canvas on the page.</summary>
        public void NewShape()
        {
            CompatSupport.Check(NativeMethods.pdf_compat_page_new_shape(_doc.RequireOpen(), _page), "NewShape");
        }

        /// <summary>Page box in points.</summary>
        public unsafe Rect Rect()
        {
            Rect outValue = default;
            CompatSupport.Check(NativeMethods.pdf_compat_page_rect(_doc.RequireOpen(), _page, &outValue), "Rect");
            return outValue;
        }

        /// <summary>Page rotation in degrees.</summary>
        public int Rotation()
        {
            int outValue;
            CompatSupport.Check(NativeMethods.pdf_compat_page_rotation(_doc.RequireOpen(), _page, out outValue), "Rotation");
            return outValue;
        }

        /// <summary>Search for text; rectangles or quadrilaterals.</summary>
        public unsafe SearchHits SearchFor(string? needle, Rect? clip, bool quads, int? flags, int limit, bool use_textpage)
        {
            Rect clipTmp = clip.GetValueOrDefault();
            Rect* clipPtr = clip.HasValue ? &clipTmp : null;
            IntPtr hPtr;
            CompatSupport.Check(NativeMethods.pdf_compat_page_search_for(_doc.RequireOpen(), _page, needle, clipPtr, CompatSupport.Flag(quads), flags ?? -1, limit, CompatSupport.Flag(use_textpage), out hPtr), "SearchFor");
            return SearchHits.Read(hPtr);
        }

        /// <summary>Convenience overload with defaults.</summary>
        public unsafe SearchHits SearchFor(string? needle)
        {
            return SearchFor(needle, null, false, null, 0, false);
        }

        /// <summary>Set the crop box.</summary>
        public unsafe void SetCropbox(Rect rect)
        {
            Rect rectTmp = rect;
            CompatSupport.Check(NativeMethods.pdf_compat_page_set_cropbox(_doc.RequireOpen(), _page, &rectTmp), "SetCropbox");
        }

        /// <summary>Set the rotation.</summary>
        public void SetRotation(int rotation)
        {
            CompatSupport.Check(NativeMethods.pdf_compat_page_set_rotation(_doc.RequireOpen(), _page, rotation), "SetRotation");
        }

        /// <summary>Convenience overload with defaults.</summary>
        public unsafe int ShowPdfPage(Rect rect, Document source)
        {
            return ShowPdfPage(rect, source, 0, new ShowPageOptions());
        }

        /// <summary>Form widgets on the page.</summary>
        public List<Widget> Widgets(int[]? types)
        {
            unsafe
            {
                fixed (int* typesPtr = types)
                {
                IntPtr listPtr;
                CompatSupport.Check(NativeMethods.pdf_compat_page_widgets(_doc.RequireOpen(), _page, typesPtr, CompatSupport.Count(types), out listPtr), "Widgets");
                try
                {
                    return Widget.ReadList(listPtr);
                }
                finally
                {
                    NativeMethods.pdf_compat_widget_list_free(listPtr);
                }
                }
            }
        }

        /// <summary>Convenience overload with defaults.</summary>
        public List<Widget> Widgets()
        {
            return Widgets(null);
        }

        /// <summary>Page object number.</summary>
        public int Xref()
        {
            int outValue;
            CompatSupport.Check(NativeMethods.pdf_compat_page_xref(_doc.RequireOpen(), _page, out outValue), "Xref");
            return outValue;
        }

        /// <summary>
        /// Render the page to a pixel buffer.
        /// </summary>
        /// <param name="options">Render options.</param>
        /// <returns>The rendered pixmap.</returns>
        public unsafe Pixmap GetPixmap(RenderOptions? options)
        {
            options = options ?? new RenderOptions();
            Matrix mTmp = options.Matrix ?? new Matrix(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
            Rect clipTmp = options.Clip.GetValueOrDefault();
            Rect* clipPtr = options.Clip.HasValue ? &clipTmp : null;
            CompatSupport.Check(NativeMethods.pdf_compat_page_get_pixmap(_doc.RequireOpen(),
                _page, &mTmp, options.Dpi ?? double.NaN,
                options.Colorspace.HasValue ? (int)options.Colorspace.Value : -1,
                clipPtr, CompatSupport.Flag(options.Alpha),
                CompatSupport.Flag(options.Annots), out IntPtr hPtr), "GetPixmap");
            return new Pixmap(hPtr);
        }


        /// <summary>
        /// Insert text at a point.
        /// </summary>
        /// <param name="at">Insertion point.</param>
        /// <param name="text">Text to insert.</param>
        /// <param name="options">Layout options, or null for defaults.</param>
        /// <returns>The insertion result code.</returns>
        public unsafe int InsertText(Point at, string? text, InsertTextOptions? options)
        {
            Point atTmp = at;
            CompatSupport.Check(NativeMethods.pdf_compat_page_insert_text(_doc.RequireOpen(),
                _page, &atTmp, text, IntPtr.Zero, UIntPtr.Zero,
                options == null ? null : options.ToJson(), out int v), "InsertText");
            return v;
        }

        /// <summary>
        /// Insert text lines at a point.
        /// </summary>
        /// <param name="at">Insertion point.</param>
        /// <param name="lines">Text lines to insert.</param>
        /// <param name="options">Layout options, or null for defaults.</param>
        /// <returns>The insertion result code.</returns>
        public unsafe int InsertText(Point at, List<string>? lines, InsertTextOptions? options)
        {
            Point atTmp = at;
            string[]? array = lines == null ? null : lines.ToArray();
            IntPtr linesPtr = CompatSupport.AllocStringArray(array, out UIntPtr linesLen);
            try
            {
                CompatSupport.Check(NativeMethods.pdf_compat_page_insert_text(
                    _doc.RequireOpen(), _page, &atTmp, null, linesPtr, linesLen,
                    options == null ? null : options.ToJson(), out int v), "InsertText");
                return v;
            }
            finally
            {
                CompatSupport.FreeStringArray(linesPtr, linesLen);
            }
        }

        /// <summary>
        /// Insert text at a point with default options.
        /// </summary>
        /// <param name="at">Insertion point.</param>
        /// <param name="text">Text to insert.</param>
        /// <returns>The insertion result code.</returns>
        public int InsertText(Point at, string? text)
        {
            return InsertText(at, text, null);
        }

        /// <summary>
        /// Insert text lines at a point with default options.
        /// </summary>
        /// <param name="at">Insertion point.</param>
        /// <param name="lines">Text lines to insert.</param>
        /// <returns>The insertion result code.</returns>
        public int InsertText(Point at, List<string>? lines)
        {
            return InsertText(at, lines, null);
        }


        /// <summary>
        /// Flow text into a rectangle.
        /// </summary>
        /// <param name="rect">Box to fill.</param>
        /// <param name="text">Text to flow.</param>
        /// <param name="options">Layout options, or null for defaults.</param>
        /// <returns>The used height.</returns>
        public unsafe double InsertTextbox(Rect rect, string? text, TextBoxOptions? options)
        {
            Rect rectTmp = rect;
            CompatSupport.Check(NativeMethods.pdf_compat_page_insert_textbox(_doc.RequireOpen(),
                _page, &rectTmp, text, IntPtr.Zero, UIntPtr.Zero,
                options == null ? null : options.ToJson(), out double v), "InsertTextbox");
            return v;
        }

        /// <summary>
        /// Flow text lines into a rectangle.
        /// </summary>
        /// <param name="rect">Box to fill.</param>
        /// <param name="lines">Text lines to flow.</param>
        /// <param name="options">Layout options, or null for defaults.</param>
        /// <returns>The used height.</returns>
        public unsafe double InsertTextbox(Rect rect, List<string>? lines, TextBoxOptions? options)
        {
            Rect rectTmp = rect;
            string[]? array = lines == null ? null : lines.ToArray();
            IntPtr linesPtr = CompatSupport.AllocStringArray(array, out UIntPtr linesLen);
            try
            {
                CompatSupport.Check(NativeMethods.pdf_compat_page_insert_textbox(
                    _doc.RequireOpen(), _page, &rectTmp, null, linesPtr, linesLen,
                    options == null ? null : options.ToJson(), out double v), "InsertTextbox");
                return v;
            }
            finally
            {
                CompatSupport.FreeStringArray(linesPtr, linesLen);
            }
        }

        /// <summary>
        /// Flow text into a rectangle with default options.
        /// </summary>
        /// <param name="rect">Box to fill.</param>
        /// <param name="text">Text to flow.</param>
        /// <returns>The used height.</returns>
        public double InsertTextbox(Rect rect, string? text)
        {
            return InsertTextbox(rect, text, null);
        }

        /// <summary>
        /// Flow text lines into a rectangle with default options.
        /// </summary>
        /// <param name="rect">Box to fill.</param>
        /// <param name="lines">Text lines to flow.</param>
        /// <returns>The used height.</returns>
        public double InsertTextbox(Rect rect, List<string>? lines)
        {
            return InsertTextbox(rect, lines, null);
        }


        /// <summary>
        /// Place an image on the page.
        /// </summary>
        /// <param name="rect">Box to fill.</param>
        /// <param name="options">Placement options.</param>
        /// <returns>The image object number.</returns>
        public unsafe int InsertImage(Rect rect, ImageInsertOptions options)
        {
            Rect rectTmp = rect;
            fixed (byte* streamPtr = options.Stream)
            {
                CompatSupport.Check(NativeMethods.pdf_compat_page_insert_image(
                    _doc.RequireOpen(), _page, &rectTmp, options.Alpha, options.Filename,
                    options.Height, CompatSupport.Flag(options.KeepProportion),
                    options.MaskJson, options.Oc, CompatSupport.Flag(options.Overlay),
                    options.Pixmap == null ? IntPtr.Zero : options.Pixmap.RequireOpen(),
                    options.Rotate, streamPtr, CompatSupport.Count(options.Stream),
                    options.Width, options.Xref, out int v), "InsertImage");
                return v;
            }
        }


        /// <summary>
        /// Register a font for the page.
        /// </summary>
        /// <param name="options">Font options.</param>
        /// <returns>The font object number.</returns>
        public unsafe int InsertFont(InsertFontOptions options)
        {
            fixed (byte* bufPtr = options.Fontbuffer)
            {
                CompatSupport.Check(NativeMethods.pdf_compat_page_insert_font(
                    _doc.RequireOpen(), _page, options.Fontname, options.Fontfile,
                    bufPtr, CompatSupport.Count(options.Fontbuffer),
                    CompatSupport.Flag(options.SetSimple), options.Wmode, options.Encoding,
                    out int v), "InsertFont");
                return v;
            }
        }


        /// <summary>
        /// Stamp a source page onto this one.
        /// </summary>
        /// <param name="rect">Box to fill.</param>
        /// <param name="source">Source document.</param>
        /// <param name="pageNo">Source page number.</param>
        /// <param name="options">Placement options.</param>
        /// <returns>The stamp result code.</returns>
        public unsafe int ShowPdfPage(Rect rect, Document source, int pageNo,
            ShowPageOptions options)
        {
            Rect rectTmp = rect;
            Rect clipTmp = options.Clip.GetValueOrDefault();
            Rect* clipPtr = options.Clip.HasValue ? &clipTmp : null;
            CompatSupport.Check(NativeMethods.pdf_compat_page_show_pdf_page(_doc.RequireOpen(),
                _page, &rectTmp, source.RequireOpen(), pageNo,
                CompatSupport.Flag(options.KeepProportion),
                CompatSupport.Flag(options.Overlay), options.Oc, options.Rotate,
                clipPtr, out int v), "ShowPdfPage");
            return v;
        }


        /// <summary>
        /// Mark a quadrilateral for redaction; returns its xref.
        /// </summary>
        /// <param name="quad">Quadrilateral to cover.</param>
        /// <param name="options">Redaction options.</param>
        /// <returns>The annotation xref.</returns>
        public unsafe int AddRedactAnnot(Quad quad, RedactAnnotOptions options)
        {
            Quad quadTmp = quad;
            fixed (double* fillPtr = options.Fill)
            fixed (double* colorPtr = options.TextColor)
            {
                CompatSupport.Check(NativeMethods.pdf_compat_page_add_redact_annot(
                    _doc.RequireOpen(), _page, &quadTmp, options.Text, options.Fontname,
                    options.Fontsize, options.Align, fillPtr,
                    CompatSupport.Count(options.Fill), colorPtr,
                    CompatSupport.Count(options.TextColor),
                    CompatSupport.Flag(options.CrossOut), out int v), "AddRedactAnnot");
                return v;
            }
        }

    }
}
