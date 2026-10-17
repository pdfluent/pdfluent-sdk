// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

import static com.pdfluent.CompatSupport.jsonDoubles;
import static com.pdfluent.CompatSupport.jsonQuote;

/**
 * Vector drawing options.
 *
 * <p>Mutable DTO with recorded defaults; serialises to the JSON the native
 * layer overlays onto its own defaults.
 */
public final class DrawOptions {
    /** Stroke colour. */
    public double[] color = new double[] {0.0};

    /** Fill colour, if any. */
    public double[] fill = null;

    /** Dash pattern, if any. */
    public String dashes = null;

    /** Stroke width in points. */
    public double width = 1.0;

    /** Line-cap style. */
    public int lineCap = 0;

    /** Line-join style. */
    public int lineJoin = 0;

    /** Morph matrix, if any. */
    public double[] morph = null;

    /** Close the path. */
    public boolean closePath = false;

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
        first = append(out, first, "color", color == null ? null : jsonDoubles(color));
        first = append(out, first, "fill", fill == null ? null : jsonDoubles(fill));
        first = append(out, first, "dashes", dashes == null ? null : jsonQuote(dashes));
        first = append(out, first, "width", String.valueOf(width));
        first = append(out, first, "line_cap", String.valueOf(lineCap));
        first = append(out, first, "line_join", String.valueOf(lineJoin));
        first = append(out, first, "morph", morph == null ? null : jsonDoubles(morph));
        first = append(out, first, "close_path", String.valueOf(closePath));
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
