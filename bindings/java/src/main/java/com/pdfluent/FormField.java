package com.pdfluent;

/**
 * An interactive AcroForm field, as returned by
 * {@link PdfluentDocument#getFormFields()}.
 *
 * <p>Instances are immutable value objects. The {@link #fieldType} string is
 * one of {@code "text"}, {@code "button"}, {@code "choice"},
 * {@code "signature"}, or {@code "unknown"}.
 */
public final class FormField {

    /** Fully-qualified field name (e.g. {@code "Address.Street"}). */
    public final String name;

    /** Field type: {@code text}, {@code button}, {@code choice}, {@code signature}. */
    public final String fieldType;

    /** Current value, or the empty string when the field has none. */
    public final String value;

    /** 0-based page index of the field's first widget, or -1 when unknown. */
    public final int page;

    /**
     * Construct a form field descriptor.
     *
     * @param name      fully-qualified field name
     * @param fieldType field type string
     * @param value     current value (never {@code null}; empty when absent)
     * @param page      0-based page index, or -1 when unknown
     */
    public FormField(String name, String fieldType, String value, int page) {
        this.name = name;
        this.fieldType = fieldType;
        this.value = value;
        this.page = page;
    }

    @Override
    public String toString() {
        return "FormField{name='" + name + "', fieldType='" + fieldType
            + "', value='" + value + "', page=" + page + '}';
    }
}
