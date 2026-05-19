package com.pdfluent;

import java.util.Objects;

/**
 * A single structured text block extracted from a PDF page.
 *
 * <p>Coordinates are in PDF user-space points (1/72 inch). The origin is
 * the bottom-left of the page (PDF convention). {@link #getWidth()} and
 * {@link #getHeight()} are always non-negative; empty blocks have all
 * four geometric fields set to zero.
 *
 * <p>Returned by {@link PdfluentDocument#extractTextBlocks(int)}.
 */
public final class TextBlock {
    private final double x;
    private final double y;
    private final double width;
    private final double height;
    private final String text;

    /** Construct a new immutable {@code TextBlock}. */
    public TextBlock(double x, double y, double width, double height, String text) {
        this.x = x;
        this.y = y;
        this.width = width;
        this.height = height;
        this.text = text == null ? "" : text;
    }

    /** PDF user-space X of the block's bottom-left corner. */
    public double getX() { return x; }

    /** PDF user-space Y of the block's bottom-left corner. */
    public double getY() { return y; }

    /** Block width in PDF points. Always &gt;= 0. */
    public double getWidth() { return width; }

    /** Block height in PDF points. Always &gt;= 0. */
    public double getHeight() { return height; }

    /**
     * Concatenated UTF-8 text of all spans in this block, joined in
     * reading order. Never {@code null}; may be empty.
     */
    public String getText() { return text; }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (!(o instanceof TextBlock)) return false;
        TextBlock other = (TextBlock) o;
        return Double.compare(x, other.x) == 0
            && Double.compare(y, other.y) == 0
            && Double.compare(width, other.width) == 0
            && Double.compare(height, other.height) == 0
            && Objects.equals(text, other.text);
    }

    @Override
    public int hashCode() {
        return Objects.hash(x, y, width, height, text);
    }

    @Override
    public String toString() {
        return "TextBlock(x=" + x + ", y=" + y
            + ", w=" + width + ", h=" + height
            + ", text=" + (text == null ? "null" : "\"" + text + "\"") + ")";
    }
}
