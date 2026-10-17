# xlsx-extract

XLSX workbook extraction onto `pdf-engine` blocks, so spreadsheets serialize through the same Markdown as PDFs.

Each sheet becomes a level-1 heading (the tab name) followed by one table run
over the used range, so `pdf_engine::to_markdown` renders the workbook as
headings plus pipe tables. Handles shared strings, numbers, booleans, ISO
dates from date-styled cells, and merged cells as spans. Formula cells show
their cached value; the formula text is exposed as metadata and never
recalculated.

- Documentation: <https://pdfluent.com/docs>
- Pricing: <https://pdfluent.com/sdk/pricing>
