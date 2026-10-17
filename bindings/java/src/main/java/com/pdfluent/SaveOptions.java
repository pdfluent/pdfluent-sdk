// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

import static com.pdfluent.CompatSupport.jsonDoubles;
import static com.pdfluent.CompatSupport.jsonQuote;

/**
 * Save and serialise options.
 *
 * <p>Mutable DTO with recorded defaults; serialises to the JSON the native
 * layer overlays onto its own defaults.
 */
public final class SaveOptions {
    /** Garbage-collection level. */
    public int garbage = 0;

    /** Clean content streams. */
    public boolean clean = false;

    /** Deflate streams. */
    public boolean deflate = false;

    /** Deflate images. */
    public boolean deflateImages = false;

    /** Deflate fonts. */
    public boolean deflateFonts = false;

    /** Append an incremental update. */
    public boolean incremental = false;

    /** Use ASCII filters. */
    public boolean ascii = false;

    /** Expand compressed objects. */
    public boolean expand = false;

    /** Linearize the output. */
    public boolean linear = false;

    /** Keep the file identifier. */
    public boolean noNewId = false;

    /** Regenerate appearances. */
    public boolean appearance = false;

    /** Pretty-print objects. */
    public boolean pretty = false;

    /** Encryption selector. */
    public int encryption = 1;

    /** Permission bits. */
    public int permissions = 4095;

    /** Owner password, if any. */
    public String ownerPassword = null;

    /** User password, if any. */
    public String userPassword = null;

    /** Keep XMP metadata. */
    public boolean preserveMetadata = true;

    /** Use object streams. */
    public boolean useObjstms = false;

    /** Compression effort. */
    public int compressionEffort = 0;

    /** Fail on repair. */
    public boolean raiseOnRepair = false;

    /** Byte-stable output. */
    public boolean reproducible = false;

    /** Serialise to the native JSON overlay. */
    public String toJson() {
        StringBuilder out = new StringBuilder("{");
        boolean first = true;
        first = append(out, first, "garbage", String.valueOf(garbage));
        first = append(out, first, "clean", String.valueOf(clean));
        first = append(out, first, "deflate", String.valueOf(deflate));
        first = append(out, first, "deflate_images", String.valueOf(deflateImages));
        first = append(out, first, "deflate_fonts", String.valueOf(deflateFonts));
        first = append(out, first, "incremental", String.valueOf(incremental));
        first = append(out, first, "ascii", String.valueOf(ascii));
        first = append(out, first, "expand", String.valueOf(expand));
        first = append(out, first, "linear", String.valueOf(linear));
        first = append(out, first, "no_new_id", String.valueOf(noNewId));
        first = append(out, first, "appearance", String.valueOf(appearance));
        first = append(out, first, "pretty", String.valueOf(pretty));
        first = append(out, first, "encryption", String.valueOf(encryption));
        first = append(out, first, "permissions", String.valueOf(permissions));
        first = append(out, first, "owner_password", ownerPassword == null ? null : jsonQuote(ownerPassword));
        first = append(out, first, "user_password", userPassword == null ? null : jsonQuote(userPassword));
        first = append(out, first, "preserve_metadata", String.valueOf(preserveMetadata));
        first = append(out, first, "use_objstms", String.valueOf(useObjstms));
        first = append(out, first, "compression_effort", String.valueOf(compressionEffort));
        first = append(out, first, "raise_on_repair", String.valueOf(raiseOnRepair));
        first = append(out, first, "reproducible", String.valueOf(reproducible));
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
