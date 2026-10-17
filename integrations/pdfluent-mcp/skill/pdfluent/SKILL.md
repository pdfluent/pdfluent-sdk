---
name: pdfluent
description: Read local PDF files as markdown or as structured blocks (headings, list items, tables with page and box), and find the pages that need a vision model. Use when a task needs the contents of a PDF, citations to a page, or table data from a PDF.
---

# Reading PDFs with PDFluent

The `pdfluent` MCP server (`pdfluent-mcp`) reads PDFs on this machine. Pass it
the file's path; it never uploads anything.

## When to use which tool

- **To read, summarise or quote a document**, call `extract_markdown`. Headings,
  lists and tables keep their shape. For a long document, read it in windows
  with `first_page` and `page_count` (0-based).
- **To cite where something is, or to work with one kind of block**, call
  `extract_blocks`. Every block has `kind`, `text`, `page` (0-based) and
  `bbox`, in PDF points with the origin at the bottom left. Table cells carry
  `row`, `column` and `header`. Give page numbers to the user 1-based.
- **Before you send a page to a vision model or ask for a screenshot**, call
  `vision_advice`, or read `pages_needing_vision` from `extract_blocks`. A
  page listed there has no text layer; it is usually a scan. Text extraction
  returns nothing for such a page, so do not report it as empty. Say it needs
  OCR or a vision model.

## Reading `confidence`

A block's `confidence` is the probability that the learned model gave for the
block's kind. `null` means no model decided it, so treat it as unknown, not as
low. When a decision depends on a block's kind and its confidence is low, check
the block's text before relying on the kind.

## Running heads

`extract_markdown` leaves out running heads and page numbers, and
`extract_blocks` does too unless you pass `include_furniture=true`. Ask for
them only when the task is about the headers or footers themselves.
