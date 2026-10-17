# llama-index-readers-pdfluent

A [LlamaIndex](https://github.com/run-llama/llama_index) reader for
[PDFluent](https://pdfluent.com). It reads a PDF with PDFluent's structured
extraction and hands LlamaIndex one `Document` per paragraph, heading, list item
or table, in reading order, with the page and box each came from in its
metadata so an answer can cite its source.

## Install

```bash
pip install llama-index-readers-pdfluent
```

It needs Python 3.9 or later and `pdfluent` 1.1.0 or later, which pip pulls in
automatically. It also depends on `llama-index-core` (MIT).

## Use

### Blocks mode

One document per block, a table as one document, in reading order:

```python
from llama_index.core import VectorStoreIndex
from llama_index.readers.pdfluent import PDFluentReader

documents = PDFluentReader().load_data("report.pdf")
index = VectorStoreIndex.from_documents(documents)

print(documents[0].text)
print(documents[0].metadata["page"])
print(documents[0].metadata["bbox"])
print(documents[0].metadata["kind"])
```

### Markdown mode

One document per page, as markdown:

```python
from llama_index.core import VectorStoreIndex
from llama_index.readers.pdfluent import PDFluentReader

documents = PDFluentReader(mode="markdown").load_data("report.pdf")
index = VectorStoreIndex.from_documents(documents)

print(documents[0].text[:200])
print(documents[0].metadata["page"])
print(documents[1].metadata["page"])
print(documents[1].text[:200])
```

### Every PDF in a directory

```python
from llama_index.core import SimpleDirectoryReader
from llama_index.readers.pdfluent import PDFluentReader

documents = SimpleDirectoryReader(
    "./reports", file_extractor={".pdf": PDFluentReader()}
).load_data()
```

Options:

| Argument | Default | What it does |
|---|---|---|
| `mode` | `"blocks"` | `"blocks"`: one document per block, a table as one document. `"markdown"`: one document per page. |
| `include_furniture` | `False` | Keep the running heads and page numbers a document repeats on every page. |
| `first_page`, `page_count` | `0`, `None` | Read a window of pages. |

## Metadata

| Key | Value |
|---|---|
| `source` | the path the PDF was read from |
| `page` | 0-based page index |
| `kind` | `paragraph`, `heading`, `list_item`, `table`, `header`, `footer`; `page` in markdown mode |
| `bbox` | `[x0, y0, x1, y1]` in points, origin at the bottom left of the page, y up (blocks mode) |
| `confidence` | the learned model's probability for `kind`, only where a model decided it |
| `section` | the last heading above the chunk |
| `level` | a heading's level, 1 to 3 |
| `continues_previous` | `True` on the second half of a paragraph a page break cut in two |
| `toc_items` | the outline entries pointing at the page, as `[level, title, page]` triples (markdown mode) |
| `tables` | one `{"bbox", "rows", "columns"}` per table on the page (markdown mode) |

`bbox`, `confidence`, `level` and `continues_previous` are excluded from the
text the embedding model and the LLM see; they are for citing and highlighting.
`extra_info` (and the file metadata `SimpleDirectoryReader` adds) is merged into
every document.

## Licence

AGPL-3.0 or a commercial licence, at your option; see `LICENSE` and
`LICENSE-COMMERCIAL`. The commercial licence is sold yearly and self-service at
<https://pdfluent.com/sdk/pricing>.
