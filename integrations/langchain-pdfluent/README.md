# langchain-pdfluent

A [LangChain](https://github.com/langchain-ai/langchain) document loader for
[PDFluent](https://pdfluent.com). It reads a PDF with PDFluent's structured
extraction and hands LangChain one `Document` per paragraph, heading, list item
or table, in reading order, with the page and box each came from in its
metadata so an answer can cite its source.

## Install

```bash
pip install langchain-pdfluent
```

It needs Python 3.9 or later and `pdfluent` 1.1.0 or later, which pip pulls in
automatically. It also depends on `langchain-core` (MIT).

## Use

### Blocks mode

One document per block, a table as one document, in reading order:

```python
from langchain_pdfluent import PDFluentLoader

loader = PDFluentLoader("report.pdf")
docs = loader.load()

print(len(docs), "chunks")
print(docs[0].page_content)
print(docs[0].metadata["page"])
print(docs[0].metadata["bbox"])
print(docs[0].metadata["kind"])
```

### Markdown mode

One document per page, as markdown:

```python
from langchain_pdfluent import PDFluentLoader

loader = PDFluentLoader("report.pdf", mode="markdown")
pages = loader.load()

print(len(pages), "pages")
print(pages[0].page_content[:200])
print(pages[0].metadata["page"])
print(pages[1].metadata["page"])
print(pages[0].metadata["kind"])
```

Options:

| Argument | Default | What it does |
|---|---|---|
| `mode` | `"blocks"` | `"blocks"`: one document per block, a table as one document. `"markdown"`: one document per page. |
| `include_furniture` | `False` | Keep the running heads and page numbers a document repeats on every page. |
| `first_page`, `page_count` | `0`, `None` | Load a window of pages. |

## Metadata

| Key | Value |
|---|---|
| `source` | the path the PDF was loaded from |
| `page` | 0-based page index |
| `kind` | `paragraph`, `heading`, `list_item`, `table`, `header`, `footer`; `page` in markdown mode |
| `bbox` | `[x0, y0, x1, y1]` in points, origin at the bottom left of the page, y up (blocks mode) |
| `confidence` | the learned model's probability for `kind`, only where a model decided it |
| `section` | the last heading above the chunk |
| `level` | a heading's level, 1 to 3 |
| `continues_previous` | `True` on the second half of a paragraph a page break cut in two |
| `toc_items` | the outline entries pointing at the page, as `[level, title, page]` triples (markdown mode) |
| `tables` | one `{"bbox", "rows", "columns"}` per table on the page (markdown mode) |

`bbox`, `toc_items` and `tables` are lists. A vector store that only accepts
scalar metadata, such as Chroma, needs `filter_complex_metadata` from
`langchain_community` over the documents first.

## Licence

AGPL-3.0 or a commercial licence, at your option; see `LICENSE` and
`LICENSE-COMMERCIAL`. The commercial licence is sold yearly and self-service at
<https://pdfluent.com/sdk/pricing>.
