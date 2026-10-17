// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;

namespace PDFluent
{
    /// <summary>
    /// Tables detected on one page.
    /// </summary>
    public sealed class TableFinder
    {
        /// <summary>Detected tables.</summary>
        public List<Table> Tables { get; }

        internal TableFinder(List<Table> tables)
        {
            Tables = tables;
        }

        /// <summary>One table: box, shape, and cells.</summary>
        public sealed class Table
        {
            /// <summary>Table box in points.</summary>
            public Rect Bbox { get; }

            /// <summary>Row count.</summary>
            public int RowCount { get; }

            /// <summary>Column count.</summary>
            public int ColCount { get; }

            /// <summary>Table cells.</summary>
            public List<Cell> Cells { get; }

            /// <summary>Build a table.</summary>
            public Table(Rect bbox, int rowCount, int colCount, List<Cell> cells)
            {
                Bbox = bbox;
                RowCount = rowCount;
                ColCount = colCount;
                Cells = cells;
            }
        }

        /// <summary>One table cell: box and text.</summary>
        public sealed class Cell
        {
            /// <summary>Cell box in points.</summary>
            public Rect Bbox { get; }

            /// <summary>Cell text.</summary>
            public string Text { get; }

            /// <summary>Build a cell.</summary>
            public Cell(Rect bbox, string text)
            {
                Bbox = bbox;
                Text = text;
            }
        }

        internal static unsafe TableFinder Read(IntPtr handle)
        {
            try
            {
                CompatSupport.Check(NativeMethods.pdf_compat_table_finder_table_count(
                    handle, out int n), "Read");
                var tables = new List<Table>(n);
                for (int i = 0; i < n; i++)
                {
                    Rect bbox = default;
                    CompatSupport.Check(NativeMethods.pdf_compat_table_finder_table_bbox(
                        handle, i, &bbox), "Read");
                    CompatSupport.Check(NativeMethods.pdf_compat_table_finder_table_row_count(
                        handle, i, out int rows), "Read");
                    CompatSupport.Check(NativeMethods.pdf_compat_table_finder_table_col_count(
                        handle, i, out int cols), "Read");
                    CompatSupport.Check(
                        NativeMethods.pdf_compat_table_finder_table_cell_count(handle, i,
                            out int cells), "Read");
                    var result = new List<Cell>(cells);
                    for (int c = 0; c < cells; c++)
                    {
                        Rect cellBox = default;
                        CompatSupport.Check(
                            NativeMethods.pdf_compat_table_finder_table_cell_bbox(handle, i,
                                c, &cellBox), "Read");
                        CompatSupport.Check(
                            NativeMethods.pdf_compat_table_finder_table_cell_text(handle, i,
                                c, out IntPtr cellText), "Read");
                        result.Add(new Cell(cellBox,
                            CompatSupport.ReadString(cellText)));
                    }
                    tables.Add(new Table(bbox, rows, cols, result));
                }
                return new TableFinder(tables);
            }
            finally
            {
                NativeMethods.pdf_compat_table_finder_free(handle);
            }
        }
    }
}
