package com.xfa.pdf;

/**
 * Represents a PDF annotation on a page.
 */
public class Annotation {

    /**
     * Annotation type as reported by the engine, e.g.
     * {@code "Text"}, {@code "Highlight"}, {@code "FreeText"}, {@code "Link"}.
     */
    public final String annotationType;

    /** 0-based page index on which this annotation resides. */
    public final int page;

    /** Bounding rectangle in PDF user-space points: x0, y0, x1, y1. */
    public final double x0, y0, x1, y1;

    /** Text content / subject of the annotation, or {@code null}. */
    public final String contents;

    /** Author of the annotation, or {@code null}. */
    public final String author;

    /**
     * @param annotationType annotation type string (e.g. {@code "Highlight"})
     * @param page           0-based page index
     * @param x0             left edge of bounding rectangle in PDF user-space points
     * @param y0             bottom edge of bounding rectangle
     * @param x1             right edge of bounding rectangle
     * @param y1             top edge of bounding rectangle
     * @param contents       text content (empty string is normalised to {@code null})
     * @param author         author string (empty string is normalised to {@code null})
     */
    public Annotation(String annotationType, int page,
                      double x0, double y0, double x1, double y1,
                      String contents, String author) {
        this.annotationType = annotationType;
        this.page     = page;
        this.x0       = x0;
        this.y0       = y0;
        this.x1       = x1;
        this.y1       = y1;
        this.contents = contents.isEmpty() ? null : contents;
        this.author   = author.isEmpty()   ? null : author;
    }

    @Override
    public String toString() {
        return "Annotation{type='" + annotationType + "', page=" + page
                + ", rect=[" + x0 + "," + y0 + "," + x1 + "," + y1 + "]"
                + ", contents='" + contents + "'}";
    }
}
