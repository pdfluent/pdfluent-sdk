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
 * A compatible font.
 */
public final class Font implements AutoCloseable {
    private Pointer handle;

    /**
     * Build a font with default options.
     */
    public Font() {
        this(new FontOptions());
    }

    /**
     * Build a font from a name, file, or buffer.
     *
     * @param options font options
     */
    public Font(FontOptions options) {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_font_new(options.fontname, options.fontfile, options.fontbuffer,
            countOf(options.fontbuffer), options.script, options.language, options.ordering,
            flag(options.isBold), flag(options.isItalic), flag(options.isSerif),
            flag(options.embed), out), "Font");
        this.handle = out.getValue();
    }

    Font(Pointer handle) {
        this.handle = handle;
    }


    /**
     * Typeset width of a string.
     *
     * @param text    string to measure
     * @param options measurement options
     * @return the width in points
     */
    public double textLength(String text, FontMeasureOptions options) {
        DoubleByReference out = new DoubleByReference();
        check(lib().pdf_compat_font_text_length(requireOpen(), text, options.fontsize,
            options.language, options.script, options.wmode, flag(options.smallCaps),
            options.fallback, out), "textLength");
        return out.getValue();
    }


    /** Convenience overload with defaults. */
    public double textLength(String text) {
        return textLength(text, new FontMeasureOptions());
    }

    /**
     * Close the handle. Idempotent.
     */
    @Override
    public void close() {
        if (handle != null) {
            lib().pdf_compat_font_free(handle);
            handle = null;
        }
    }

    /**
     * The native handle, failing when closed.
     *
     * @return the live handle
     */
    public Pointer requireOpen() {
        if (handle == null) {
            throw new PdfluentException("font is closed");
        }
        return handle;
    }
}
