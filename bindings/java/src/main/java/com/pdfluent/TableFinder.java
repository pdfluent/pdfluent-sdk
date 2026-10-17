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
 * Tables detected on one page.
 */
public final class TableFinder {
    private final java.util.List<Table> tables;

    TableFinder(java.util.List<Table> tables) {
        this.tables = tables;
    }

    /** Detected tables. */
    public java.util.List<Table> getTables() { return tables; }

    /** One table: box, shape, and cells. */
    public static final class Table {
        private final Rect bbox;
        private final int rowCount;
        private final int colCount;
        private final java.util.List<Cell> cells;

        /** Build a table. */
        public Table(Rect bbox, int rowCount, int colCount, java.util.List<Cell> cells) {
            this.bbox = bbox;
            this.rowCount = rowCount;
            this.colCount = colCount;
            this.cells = cells;
        }

        /** Table box in points. */
        public Rect getBbox() { return bbox; }

        /** Row count. */
        public int getRowCount() { return rowCount; }

        /** Column count. */
        public int getColCount() { return colCount; }

        /** Table cells. */
        public java.util.List<Cell> getCells() { return cells; }
    }

    /** One table cell: box and text. */
    public static final class Cell {
        private final Rect bbox;
        private final String text;

        /** Build a cell. */
        public Cell(Rect bbox, String text) {
            this.bbox = bbox;
            this.text = text;
        }

        /** Cell box in points. */
        public Rect getBbox() { return bbox; }

        /** Cell text. */
        public String getText() { return text; }
    }

    static TableFinder read(Pointer handle) {
        try {
            IntByReference count = new IntByReference();
            check(lib().pdf_compat_table_finder_table_count(handle, count), "read");
            java.util.List<Table> tables = new java.util.ArrayList<>();
            for (int i = 0; i < count.getValue(); i++) {
                PdfCapiLibrary.PdfCompatRect bbox = new PdfCapiLibrary.PdfCompatRect();
                check(lib().pdf_compat_table_finder_table_bbox(handle, i, bbox), "read");
                IntByReference rows = new IntByReference();
                check(lib().pdf_compat_table_finder_table_row_count(handle, i, rows), "read");
                IntByReference cols = new IntByReference();
                check(lib().pdf_compat_table_finder_table_col_count(handle, i, cols), "read");
                IntByReference cells = new IntByReference();
                check(lib().pdf_compat_table_finder_table_cell_count(handle, i, cells), "read");
                java.util.List<Cell> result = new java.util.ArrayList<>();
                for (int c = 0; c < cells.getValue(); c++) {
                    PdfCapiLibrary.PdfCompatRect cellBox = new PdfCapiLibrary.PdfCompatRect();
                    check(lib().pdf_compat_table_finder_table_cell_bbox(handle, i, c, cellBox), "read");
                    PointerByReference cellText = new PointerByReference();
                    check(lib().pdf_compat_table_finder_table_cell_text(handle, i, c, cellText), "read");
                    result.add(new Cell(Rect.fromNative(cellBox), readString(cellText)));
                }
                tables.add(new Table(Rect.fromNative(bbox), rows.getValue(), cols.getValue(), result));
            }
            return new TableFinder(tables);
        } finally {
            lib().pdf_compat_table_finder_free(handle);
        }
    }
}
