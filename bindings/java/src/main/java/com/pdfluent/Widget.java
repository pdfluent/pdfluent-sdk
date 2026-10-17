// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

import com.sun.jna.Pointer;
import com.sun.jna.ptr.IntByReference;
import com.sun.jna.ptr.PointerByReference;

import static com.pdfluent.CompatSupport.check;
import static com.pdfluent.CompatSupport.lib;
import static com.pdfluent.CompatSupport.readString;

/**
 * A compatible form widget: owned field data built blank, then added to a page.
 */
public final class Widget implements AutoCloseable {
    private Pointer handle;

    /**
     * Build a blank widget.
     */
    public Widget() {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_widget_new(out), "Widget");
        this.handle = out.getValue();
    }

    Widget(Pointer handle) {
        this.handle = handle;
    }

    /**
     * Close the handle. Idempotent.
     */
    @Override
    public void close() {
        if (handle != null) {
            lib().pdf_compat_widget_free(handle);
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
            throw new PdfluentException("widget is closed");
        }
        return handle;
    }

    /** Widget field name. */
    public String getFieldName() {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_widget_field_name(requireOpen(), out), "getFieldName");
        return readString(out);
    }

    /** Set the widget field name. */
    public void setFieldName(String value) {
        check(lib().pdf_compat_widget_set_field_name(requireOpen(), value), "setFieldName");
    }

    /** Widget field-type code. */
    public int getFieldType() {
        IntByReference out = new IntByReference();
        check(lib().pdf_compat_widget_field_type(requireOpen(), out), "getFieldType");
        return out.getValue();
    }

    /** Set the widget field-type code. */
    public void setFieldType(int value) {
        check(lib().pdf_compat_widget_set_field_type(requireOpen(), value), "setFieldType");
    }

    /** Widget field value text. */
    public String getFieldValue() {
        PointerByReference out = new PointerByReference();
        check(lib().pdf_compat_widget_field_value(requireOpen(), out), "getFieldValue");
        return readString(out);
    }

    /** Set the widget field value text. */
    public void setFieldValue(String value) {
        check(lib().pdf_compat_widget_set_field_value(requireOpen(), value), "setFieldValue");
    }

    /** Widget box in points. */
    public Rect getRect() {
        PdfCapiLibrary.PdfCompatRect out = new PdfCapiLibrary.PdfCompatRect();
        check(lib().pdf_compat_widget_rect(requireOpen(), out), "getRect");
        return Rect.fromNative(out);
    }

    /** Set the widget box. */
    public void setRect(Rect value) {
        check(lib().pdf_compat_widget_set_rect(requireOpen(), value.toNative()), "setRect");
    }

    static java.util.List<Widget> readList(Pointer list) {
        IntByReference count = new IntByReference();
        check(lib().pdf_compat_widget_list_count(list, count), "read");
        java.util.List<Widget> result = new java.util.ArrayList<>();
        for (int i = 0; i < count.getValue(); i++) {
            PointerByReference out = new PointerByReference();
            check(lib().pdf_compat_widget_list_at(list, i, out), "read");
            result.add(new Widget(out.getValue()));
        }
        return result;
    }
}
