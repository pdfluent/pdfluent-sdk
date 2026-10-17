// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

import com.sun.jna.NativeLong;
import com.sun.jna.Pointer;
import com.sun.jna.ptr.DoubleByReference;
import com.sun.jna.ptr.IntByReference;
import com.sun.jna.ptr.NativeLongByReference;
import com.sun.jna.ptr.PointerByReference;

import static com.pdfluent.CompatSupport.check;
import static com.pdfluent.CompatSupport.countOf;
import static com.pdfluent.CompatSupport.flag;
import static com.pdfluent.CompatSupport.lib;
import static com.pdfluent.CompatSupport.readBytes;
import static com.pdfluent.CompatSupport.readDoubles;
import static com.pdfluent.CompatSupport.readInts;
import static com.pdfluent.CompatSupport.readString;

/**
 * One vector drawing.
 */
public final class Drawing {
    private final Rect rect;
    private final int seqno;
    private final String drawingType;
    private final double[] color;
    private final double[] fill;
    private final Double width;
    private final int[] lineCap;
    private final Integer lineJoin;
    private final String dashes;
    private final Boolean closePath;
    private final boolean evenOdd;
    private final Double fillOpacity;
    private final Double strokeOpacity;
    private final String layer;
    private final Integer level;
    private final java.util.List<DrawingItem> items;

    /** Build a Drawing. */
    public Drawing(Rect rect, int seqno, String drawingType, double[] color, double[] fill, Double width, int[] lineCap, Integer lineJoin, String dashes, Boolean closePath, boolean evenOdd, Double fillOpacity, Double strokeOpacity, String layer, Integer level, java.util.List<DrawingItem> items) {
        this.rect = rect;
        this.seqno = seqno;
        this.drawingType = drawingType;
        this.color = color;
        this.fill = fill;
        this.width = width;
        this.lineCap = lineCap;
        this.lineJoin = lineJoin;
        this.dashes = dashes;
        this.closePath = closePath;
        this.evenOdd = evenOdd;
        this.fillOpacity = fillOpacity;
        this.strokeOpacity = strokeOpacity;
        this.layer = layer;
        this.level = level;
        this.items = items;
    }

    /** Drawing box in points. */
    public Rect getRect() { return rect; }

    /** Sequence number. */
    public int getSeqno() { return seqno; }

    /** Drawing operator name. */
    public String getDrawingType() { return drawingType; }

    /** Stroke colour, or empty. */
    public double[] getColor() { return color; }

    /** Fill colour, or empty. */
    public double[] getFill() { return fill; }

    /** Stroke width, if any. */
    public Double getWidth() { return width; }

    /** Line-cap values, or empty. */
    public int[] getLineCap() { return lineCap; }

    /** Line-join value, if any. */
    public Integer getLineJoin() { return lineJoin; }

    /** Dash pattern, if any. */
    public String getDashes() { return dashes; }

    /** Close-path flag, if any. */
    public Boolean getClosePath() { return closePath; }

    /** Even-odd fill rule. */
    public boolean isEvenOdd() { return evenOdd; }

    /** Fill opacity, if any. */
    public Double getFillOpacity() { return fillOpacity; }

    /** Stroke opacity, if any. */
    public Double getStrokeOpacity() { return strokeOpacity; }

    /** Layer name. */
    public String getLayer() { return layer; }

    /** Nesting level, if any. */
    public Integer getLevel() { return level; }

    /** Path items. */
    public java.util.List<DrawingItem> getItems() { return items; }

    /** One path item: an operator and its parameters as JSON. */
    public static final class DrawingItem {
        private final String op;
        private final String params;

        /** Build a path item. */
        public DrawingItem(String op, String params) {
            this.op = op;
            this.params = params;
        }

        /** Path operator. */
        public String getOp() { return op; }

        /** Parameters as JSON. */
        public String getParams() { return params; }
    }

    static java.util.List<Drawing> readList(Pointer list) {
        IntByReference count = new IntByReference();
        check(lib().pdf_compat_drawing_list_count(list, count), "read");
        java.util.List<Drawing> result = new java.util.ArrayList<>();
        for (int i = 0; i < count.getValue(); i++) {
            PdfCapiLibrary.PdfCompatRect vrect = new PdfCapiLibrary.PdfCompatRect();
            check(lib().pdf_compat_drawing_list_rect(list, i, vrect), "read");
            Rect fRect = Rect.fromNative(vrect);
            IntByReference vseqno = new IntByReference();
            check(lib().pdf_compat_drawing_list_seqno(list, i, vseqno), "read");
            int fSeqno = vseqno.getValue();
            PointerByReference vdrawingtype = new PointerByReference();
            check(lib().pdf_compat_drawing_list_drawing_type(list, i, vdrawingtype), "read");
            String fDrawingType = readString(vdrawingtype);
            IntByReference vcolorHas = new IntByReference();
            PointerByReference vcolor = new PointerByReference();
            NativeLongByReference vcolorLen = new NativeLongByReference();
            check(lib().pdf_compat_drawing_list_color(list, i, vcolorHas, vcolor, vcolorLen), "read");
            double[] vcolorAll = readDoubles(vcolor, vcolorLen);
            double[] fColor = vcolorHas.getValue() != 0 ? vcolorAll : new double[0];
            IntByReference vfillHas = new IntByReference();
            PointerByReference vfill = new PointerByReference();
            NativeLongByReference vfillLen = new NativeLongByReference();
            check(lib().pdf_compat_drawing_list_fill(list, i, vfillHas, vfill, vfillLen), "read");
            double[] vfillAll = readDoubles(vfill, vfillLen);
            double[] fFill = vfillHas.getValue() != 0 ? vfillAll : new double[0];
            IntByReference vwidthHas = new IntByReference();
            DoubleByReference vwidth = new DoubleByReference();
            check(lib().pdf_compat_drawing_list_width(list, i, vwidthHas, vwidth), "read");
            Double fWidth = vwidthHas.getValue() != 0 ? vwidth.getValue() : null;
            IntByReference vlinecapHas = new IntByReference();
            PointerByReference vlinecap = new PointerByReference();
            NativeLongByReference vlinecapLen = new NativeLongByReference();
            check(lib().pdf_compat_drawing_list_line_cap(list, i, vlinecapHas, vlinecap, vlinecapLen), "read");
            int[] vlinecapAll = readInts(vlinecap, vlinecapLen);
            int[] fLineCap = vlinecapHas.getValue() != 0 ? vlinecapAll : new int[0];
            IntByReference vlinejoinHas = new IntByReference();
            IntByReference vlinejoin = new IntByReference();
            check(lib().pdf_compat_drawing_list_line_join(list, i, vlinejoinHas, vlinejoin), "read");
            Integer fLineJoin = vlinejoinHas.getValue() != 0 ? vlinejoin.getValue() : null;
            IntByReference vdashesHas = new IntByReference();
            PointerByReference vdashes = new PointerByReference();
            check(lib().pdf_compat_drawing_list_dashes(list, i, vdashesHas, vdashes), "read");
            String vdashesAll = readString(vdashes);
            String fDashes = vdashesHas.getValue() != 0 ? vdashesAll : null;
            IntByReference vclosepathHas = new IntByReference();
            IntByReference vclosepath = new IntByReference();
            check(lib().pdf_compat_drawing_list_close_path(list, i, vclosepathHas, vclosepath), "read");
            Boolean fClosePath = vclosepathHas.getValue() != 0 ? (vclosepath.getValue() != 0) : null;
            IntByReference vevenodd = new IntByReference();
            check(lib().pdf_compat_drawing_list_even_odd(list, i, vevenodd), "read");
            boolean fEvenOdd = vevenodd.getValue() != 0;
            IntByReference vfillopacityHas = new IntByReference();
            DoubleByReference vfillopacity = new DoubleByReference();
            check(lib().pdf_compat_drawing_list_fill_opacity(list, i, vfillopacityHas, vfillopacity), "read");
            Double fFillOpacity = vfillopacityHas.getValue() != 0 ? vfillopacity.getValue() : null;
            IntByReference vstrokeopacityHas = new IntByReference();
            DoubleByReference vstrokeopacity = new DoubleByReference();
            check(lib().pdf_compat_drawing_list_stroke_opacity(list, i, vstrokeopacityHas, vstrokeopacity), "read");
            Double fStrokeOpacity = vstrokeopacityHas.getValue() != 0 ? vstrokeopacity.getValue() : null;
            PointerByReference vlayer = new PointerByReference();
            check(lib().pdf_compat_drawing_list_layer(list, i, vlayer), "read");
            String fLayer = readString(vlayer);
            IntByReference vlevelHas = new IntByReference();
            IntByReference vlevel = new IntByReference();
            check(lib().pdf_compat_drawing_list_level(list, i, vlevelHas, vlevel), "read");
            Integer fLevel = vlevelHas.getValue() != 0 ? vlevel.getValue() : null;
            IntByReference vitemcount = new IntByReference();
            check(lib().pdf_compat_drawing_list_item_count(list, i, vitemcount), "read");
            java.util.List<DrawingItem> fItems = new java.util.ArrayList<>();
            for (int j = 0; j < vitemcount.getValue(); j++) {
                PointerByReference vop = new PointerByReference();
                check(lib().pdf_compat_drawing_list_item_op(list, i, j, vop), "read");
                PointerByReference vparams = new PointerByReference();
                check(lib().pdf_compat_drawing_list_item_params(list, i, j, vparams), "read");
                fItems.add(new DrawingItem(readString(vop), readString(vparams)));
            }
            result.add(new Drawing(fRect, fSeqno, fDrawingType, fColor, fFill, fWidth, fLineCap, fLineJoin, fDashes, fClosePath, fEvenOdd, fFillOpacity, fStrokeOpacity, fLayer, fLevel, fItems));
        }
        return result;
    }
}
