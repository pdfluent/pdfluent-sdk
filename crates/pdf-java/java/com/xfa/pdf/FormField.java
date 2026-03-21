package com.xfa.pdf;

/**
 * Represents an interactive form field (AcroForm widget).
 */
public class FormField {

    /** Fully-qualified field name, e.g. {@code "Address.Street"}. */
    public final String name;

    /**
     * Field type: {@code "text"}, {@code "button"}, {@code "choice"},
     * {@code "signature"}, or {@code "unknown"}.
     */
    public final String fieldType;

    /** Current text value, or {@code null} if the field has no value. */
    public final String value;

    /**
     * 0-based page index on which the widget appears, or {@code -1} if unknown.
     */
    public final int page;

    /**
     * @param name      fully-qualified field name
     * @param fieldType field type string
     * @param value     raw value string (empty string is normalised to {@code null})
     * @param page      0-based page index, or {@code -1} if unknown
     */
    public FormField(String name, String fieldType, String value, int page) {
        this.name      = name;
        this.fieldType = fieldType;
        this.value     = value.isEmpty() ? null : value;
        this.page      = page;
    }

    @Override
    public String toString() {
        return "FormField{name='" + name + "', type='" + fieldType
                + "', value='" + value + "', page=" + page + "}";
    }
}
