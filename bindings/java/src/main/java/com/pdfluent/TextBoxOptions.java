// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

import static com.pdfluent.CompatSupport.jsonDoubles;
import static com.pdfluent.CompatSupport.jsonQuote;

/**
 * Textbox layout options.
 *
 * <p>Mutable DTO with recorded defaults; serialises to the JSON the native
 * layer overlays onto its own defaults.
 */
public final class TextBoxOptions {
    /** Nested text options. */
    public InsertTextOptions text = new InsertTextOptions();

    /** Text alignment. */
    public int align = 0;

    /** Tab expansion width. */
    public int expandtabs = 1;

    /** Serialise to the native JSON overlay. */
    public String toJson() {
        StringBuilder out = new StringBuilder("{");
        boolean first = true;
        if (text != null) {
            out.append("\"text\":").append(text.toJson());
            first = false;
        }
        first = append(out, first, "align", String.valueOf(align));
        first = append(out, first, "expandtabs", String.valueOf(expandtabs));
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
