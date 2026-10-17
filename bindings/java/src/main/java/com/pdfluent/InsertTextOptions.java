// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

import static com.pdfluent.CompatSupport.jsonDoubles;
import static com.pdfluent.CompatSupport.jsonQuote;

/**
 * Inserted-text layout options.
 *
 * <p>Mutable DTO with recorded defaults; serialises to the JSON the native
 * layer overlays onto its own defaults.
 */
public final class InsertTextOptions {
    /** Font size in points. */
    public double fontsize = 11.0;

    /** Line height, if any. */
    public Double lineheight = null;

    /** Base font name. */
    public String fontname = "helv";

    /** Font file, if any. */
    public String fontfile = null;

    /** Use simple positioning. */
    public boolean setSimple = false;

    /** Text encoding. */
    public int encoding = 0;

    /** Text colour, if any. */
    public double[] color = null;

    /** Fill colour, if any. */
    public double[] fill = null;

    /** Border width. */
    public double borderWidth = 0.05;

    /** Miter limit. */
    public double miterLimit = 1.0;

    /** Text rendering mode. */
    public int renderMode = 0;

    /** Rotation in degrees. */
    public int rotate = 0;

    /** Morph matrix, if any. */
    public double[] morph = null;

    /** Draw over content. */
    public boolean overlay = true;

    /** Stroke opacity. */
    public double strokeOpacity = 1.0;

    /** Fill opacity. */
    public double fillOpacity = 1.0;

    /** Optional-content id. */
    public int oc = 0;

    /** Serialise to the native JSON overlay. */
    public String toJson() {
        StringBuilder out = new StringBuilder("{");
        boolean first = true;
        first = append(out, first, "fontsize", String.valueOf(fontsize));
        first = append(out, first, "lineheight", lineheight == null ? null : lineheight.toString());
        first = append(out, first, "fontname", fontname == null ? null : jsonQuote(fontname));
        first = append(out, first, "fontfile", fontfile == null ? null : jsonQuote(fontfile));
        first = append(out, first, "set_simple", String.valueOf(setSimple));
        first = append(out, first, "encoding", String.valueOf(encoding));
        first = append(out, first, "color", color == null ? null : jsonDoubles(color));
        first = append(out, first, "fill", fill == null ? null : jsonDoubles(fill));
        first = append(out, first, "border_width", String.valueOf(borderWidth));
        first = append(out, first, "miter_limit", String.valueOf(miterLimit));
        first = append(out, first, "render_mode", String.valueOf(renderMode));
        first = append(out, first, "rotate", String.valueOf(rotate));
        first = append(out, first, "morph", morph == null ? null : jsonDoubles(morph));
        first = append(out, first, "overlay", String.valueOf(overlay));
        first = append(out, first, "stroke_opacity", String.valueOf(strokeOpacity));
        first = append(out, first, "fill_opacity", String.valueOf(fillOpacity));
        first = append(out, first, "oc", String.valueOf(oc));
        return out.append("}").toString();
    }

    private static boolean append(StringBuilder out, boolean first, String key, String value) {
        if (value == null) {
            return first;
        }
        if (!first) {
            out.append(",");
        }
        out.append(jsonQuote(key)).append(":").append(value);
        return false;
    }
}
