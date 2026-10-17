package com.pdfluent;

import java.util.Arrays;
import java.util.Objects;

/**
 * One block of a document's structure: what a caller chunks, cites or embeds
 * (#573).
 *
 * <p>A paragraph, a heading, a list item, a table cell or a running head, with
 * the page and the box it was drawn in. {@link #getKind()} says which, and the
 * accessors that belong to that kind return a value while the rest return
 * {@code null} — {@link #getLevel()} on a heading and a list item,
 * {@link #getOrdinal()} on a list item, and the five cell accessors on a table
 * cell.
 *
 * <p>Coordinates are in PDF user-space points (1/72 inch) with the origin at
 * the bottom-left of the page, which is the PDF convention and the one
 * {@link TextBlock} uses.
 *
 * <p>Returned by {@link PdfluentDocument#extractBlocks()}.
 */
public final class Block {

    /** The kind of a block: what it is, not how it is written. */
    public enum Kind {
        /** Running text: one or more lines the page sets as one paragraph. */
        PARAGRAPH,
        /** A heading, at a level from 1 to 3. */
        HEADING,
        /** One item of a list. */
        LIST_ITEM,
        /** One cell of a table. */
        TABLE_CELL,
        /** A running header: the line a document repeats at the top of its pages. */
        HEADER,
        /** A running footer, including a page number. */
        FOOTER,
        /** A caption: the line that says what a picture or table beside it shows. */
        CAPTION;

        /**
         * The kind the native layer named, or {@link #PARAGRAPH} for a name
         * this version does not know.
         *
         * <p>Unknown rather than an exception on purpose: the engine's kind
         * list is open, and a document that arrives with a kind added after
         * this jar was built must still be readable.
         *
         * @param name the name the native layer wrote
         * @return the matching kind, or {@link #PARAGRAPH}
         */
        static Kind of(String name) {
            if (name == null) return PARAGRAPH;
            switch (name) {
                case "heading":   return HEADING;
                case "listItem":  return LIST_ITEM;
                case "tableCell": return TABLE_CELL;
                case "header":    return HEADER;
                case "footer":    return FOOTER;
                case "caption":   return CAPTION;
                default:          return PARAGRAPH;
            }
        }
    }

    private final Kind kind;
    private final String text;
    private final int page;
    private final double[] bbox;
    private final int index;
    private final boolean continuesPrevious;
    private final Integer level;
    private final Integer ordinal;
    private final Integer row;
    private final Integer column;
    private final Integer rowSpan;
    private final Integer columnSpan;
    private final Boolean header;
    private final Double confidence;
    private final GridDoubt grid;

    /**
     * What the heuristic table builder had to go on when it found a cell's TABLE
     * (#574): five facts rather than a score, so each can be tested and weighed on
     * its own. A property of the table, so every cell of one table carries the
     * same values.
     *
     * <p>A plain final class rather than a record: the jar targets Java 11 and
     * records need 16, so the record declaration broke the release packaging
     * with -Werror (#636). The constructor, the five accessors and the
     * equals/hashCode/toString contract are the record's, spelled out.
     */
    public static final class GridDoubt {
        private final int thinRows;
        private final double fill;
        private final Integer retriedFrom;
        private final Integer fullRows;
        private final Integer columns;

        /**
         * @param thinRows    rows admitted with too few runs of ink to be a row alone
         * @param fill        the share of the grid's cells the page drew ink in, 0..1
         * @param retriedFrom which attempt produced the grid; above 0 means rows were
         *                    trimmed because the first candidate was not a grid
         * @param fullRows    rows with ink in every column, which the bounds rest on
         * @param columns     how many columns the grid has, giving the rest their scale
         */
        public GridDoubt(int thinRows, double fill, Integer retriedFrom,
                         Integer fullRows, Integer columns) {
            this.thinRows = thinRows;
            this.fill = fill;
            this.retriedFrom = retriedFrom;
            this.fullRows = fullRows;
            this.columns = columns;
        }

        /** Rows admitted with too few runs of ink to be a row alone. */
        public int thinRows() { return thinRows; }

        /** The share of the grid's cells the page drew ink in, 0..1. */
        public double fill() { return fill; }

        /** Which attempt produced the grid; above 0 means rows were trimmed. */
        public Integer retriedFrom() { return retriedFrom; }

        /** Rows with ink in every column, which the bounds rest on. */
        public Integer fullRows() { return fullRows; }

        /** How many columns the grid has, giving the rest their scale. */
        public Integer columns() { return columns; }

        @Override
        public boolean equals(Object o) {
            if (this == o) return true;
            if (!(o instanceof GridDoubt)) return false;
            GridDoubt other = (GridDoubt) o;
            return thinRows == other.thinRows
                && Double.compare(fill, other.fill) == 0
                && Objects.equals(retriedFrom, other.retriedFrom)
                && Objects.equals(fullRows, other.fullRows)
                && Objects.equals(columns, other.columns);
        }

        @Override
        public int hashCode() {
            int result = Integer.hashCode(thinRows);
            result = 31 * result + Double.hashCode(fill);
            result = 31 * result + Objects.hashCode(retriedFrom);
            result = 31 * result + Objects.hashCode(fullRows);
            result = 31 * result + Objects.hashCode(columns);
            return result;
        }

        @Override
        public String toString() {
            return "GridDoubt[thinRows=" + thinRows + ", fill=" + fill
                + ", retriedFrom=" + retriedFrom + ", fullRows=" + fullRows
                + ", columns=" + columns + "]";
        }
    }

    /**
     * Construct a new immutable {@code Block}.
     *
     * @param kind              what this block is
     * @param text              the block's text
     * @param page              zero-based page index the block was drawn on
     * @param bbox              {@code [x0, y0, x1, y1]} in PDF user space
     * @param index             place in the document's reading order
     * @param continuesPrevious whether the block continues the one before it
     * @param level             heading level or list nesting depth, else {@code null}
     * @param ordinal           the number the document wrote on a list item, else {@code null}
     * @param row               a table cell's row, else {@code null}
     * @param column            a table cell's column, else {@code null}
     * @param rowSpan           how many rows a table cell covers, else {@code null}
     * @param columnSpan        how many columns a table cell covers, else {@code null}
     * @param header            whether a table cell is in the header row, else {@code null}
     */
    public Block(Kind kind, String text, int page, double[] bbox, int index,
                 boolean continuesPrevious, Integer level, Integer ordinal,
                 Integer row, Integer column, Integer rowSpan, Integer columnSpan,
                 Boolean header) {
        this(kind, text, page, bbox, index, continuesPrevious, level, ordinal,
             row, column, rowSpan, columnSpan, header, null);
    }

    /**
     * The same with the learned model's confidence in {@code kind} (#624), or
     * {@code null} where no model spoke about this block.
     *
     * <p>{@code null} is not low confidence. A block no model has an opinion
     * about carries the heuristics' verdict, and the heuristics produce a
     * decision rather than a probability, so {@code null} means nobody measured
     * this block. Code that routes the doubtful cases must treat it as unknown
     * and not as zero.
     *
     * <p>The thirteen-argument constructor above is kept so that callers written
     * against it keep compiling; it passes {@code null} here.
     */
    public Block(Kind kind, String text, int page, double[] bbox, int index,
                 boolean continuesPrevious, Integer level, Integer ordinal,
                 Integer row, Integer column, Integer rowSpan, Integer columnSpan,
                 Boolean header, Double confidence) {
        this(kind, text, page, bbox, index, continuesPrevious, level, ordinal,
             row, column, rowSpan, columnSpan, header, confidence, null);
    }

    /**
     * The same with the table builder's own evidence (#574), or {@code null} for a
     * block that is not a table cell. The fourteen-argument constructor above
     * passes {@code null} here, so callers written against it keep compiling.
     */
    public Block(Kind kind, String text, int page, double[] bbox, int index,
                 boolean continuesPrevious, Integer level, Integer ordinal,
                 Integer row, Integer column, Integer rowSpan, Integer columnSpan,
                 Boolean header, Double confidence, GridDoubt grid) {
        this.kind = kind == null ? Kind.PARAGRAPH : kind;
        this.text = text == null ? "" : text;
        this.page = page;
        this.bbox = bbox == null || bbox.length != 4
            ? new double[] {0, 0, 0, 0}
            : bbox.clone();
        this.index = index;
        this.continuesPrevious = continuesPrevious;
        this.level = level;
        this.ordinal = ordinal;
        this.row = row;
        this.column = column;
        this.rowSpan = rowSpan;
        this.columnSpan = columnSpan;
        this.header = header;
        this.confidence = confidence;
        this.grid = grid;
    }

    /** What this block is. */
    public Kind getKind() { return kind; }

    /** The block's text, the line breaks of the measure taken out. Never {@code null}. */
    public String getText() { return text; }

    /** Zero-based page index the block was drawn on. */
    public int getPage() { return page; }

    /** {@code [x0, y0, x1, y1]} in PDF user space, y up. A copy; never {@code null}. */
    public double[] getBbox() { return bbox.clone(); }

    /**
     * Place in the document's reading order, counted from the first block of
     * the first page — the document's numbering, not the window's.
     */
    public int getIndex() { return index; }

    /**
     * Whether the block continues the one before it, because the page broke a
     * paragraph in two.
     */
    public boolean continuesPrevious() { return continuesPrevious; }

    /**
     * A heading's level (1, 2 or 3) or a list item's nesting depth (0 for the
     * outermost); {@code null} for every other kind.
     */
    public Integer getLevel() { return level; }

    /**
     * The number the document wrote on a list item — {@code 3.} is 3,
     * {@code iv.} is 4 — or {@code null} for a bullet and every other kind.
     */
    public Integer getOrdinal() { return ordinal; }

    /** A table cell's zero-based row, else {@code null}. */
    public Integer getRow() { return row; }

    /** A table cell's zero-based column, counted across the whole table, else {@code null}. */
    public Integer getColumn() { return column; }

    /** How many rows a table cell covers, else {@code null}. */
    public Integer getRowSpan() { return rowSpan; }

    /** How many columns a table cell covers, else {@code null}. */
    public Integer getColumnSpan() { return columnSpan; }

    /** Whether a table cell is in the table's header row, else {@code null}. */
    public Boolean isHeader() { return header; }

    /**
     * How sure the learned model is of {@link #getKind()}, between 0 and 1, or
     * {@code null} where no model spoke about this block (#624).
     */
    public Double getConfidence() { return confidence; }

    /**
     * What the builder had to go on when it found this cell's table, or
     * {@code null} for a block that is not a table cell (#574).
     */
    public GridDoubt getGrid() { return grid; }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (!(o instanceof Block)) return false;
        Block other = (Block) o;
        return page == other.page
            && index == other.index
            && continuesPrevious == other.continuesPrevious
            && kind == other.kind
            && Objects.equals(text, other.text)
            && Arrays.equals(bbox, other.bbox)
            && Objects.equals(level, other.level)
            && Objects.equals(ordinal, other.ordinal)
            && Objects.equals(row, other.row)
            && Objects.equals(column, other.column)
            && Objects.equals(rowSpan, other.rowSpan)
            && Objects.equals(columnSpan, other.columnSpan)
            && Objects.equals(header, other.header)
            && Objects.equals(confidence, other.confidence)
            && Objects.equals(grid, other.grid);
    }

    @Override
    public int hashCode() {
        return 31 * Objects.hash(kind, text, page, index, continuesPrevious, level,
                                 ordinal, row, column, rowSpan, columnSpan, header,
                                 confidence, grid)
            + Arrays.hashCode(bbox);
    }

    @Override
    public String toString() {
        return "Block(" + kind + ", page=" + page + ", index=" + index
            + ", text=\"" + text + "\")";
    }
}
