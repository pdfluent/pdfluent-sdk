# pdfluent-extract

PDF and Office files to Markdown, JSON and RAG chunks, in three calls. It is
pure Python: the engine underneath is
[`pdfluent`](https://pypi.org/project/pdfluent/), which pip pulls in
automatically. There is nothing to compile and no system dependency to
install — on Linux arm64 and Alpine/musl as much as anywhere else, because
`pdfluent` 1.1.0 ships a wheel for every platform in the matrix below.

## Install

```bash
pip install pdfluent-extract
```

It needs Python 3.8 or later and `pdfluent` 1.1.0 or later.

## Use

```python
from pdfluent_extract import to_markdown

print(to_markdown("report.pdf"))
```

`to_markdown` takes a path, `bytes` or a file-like object, and routes PDF and
Office (`.docx`/`.pptx`/`.xlsx`) by magic bytes, not by the extension. `pages`
lists 0-based PDF page indices (`None` for the whole file).

```python
from pdfluent_extract import to_json

doc = to_json("report.pdf")
print(doc["name"], len(doc["texts"]))
```

`to_json` returns the file as a Docling `DoclingDocument`, as a dict — the
same JSON `DoclingDocument.model_validate_json` reads.

## Command line

```console
$ pdfluent-extract report.pdf -o report.md
$ pdfluent-extract report.pdf --format json --pages 1-3 > doc.json
$ cat report.pdf | pdfluent-extract - --format text
```

`--format` is one of `md`, `json`, `chunks` or `text`; `--pages` counts
from 1 (`1-3,5`). Without `-o` (or with `-o -`) the output goes to stdout.

## Chunks for RAG

```python
from pdfluent_extract import to_chunks

for chunk in to_chunks("report.pdf", max_tokens=512):
    print(chunk.metadata["page"], chunk.metadata["section_path"], chunk.text[:80])
```

One chunk per paragraph, heading and list item, and one per table. Every chunk
carries `page`, `section_path` (the enclosing headings) and `bbox` in its
metadata, plus `source` for path inputs. Tables longer than the budget split
by rows with the header repeated. PDF only in v1.

## With LangChain and LlamaIndex

```bash
pip install "pdfluent-extract[langchain]"
```

```python
from langchain_pdfluent import PDFluentLoader

docs = PDFluentLoader("report.pdf").load()
print(docs[0].page_content, docs[0].metadata["bbox"])
```

```bash
pip install "pdfluent-extract[llamaindex]"
```

```python
from llama_index.readers.pdfluent import PDFluentReader

docs = PDFluentReader().load_data("report.pdf")
print(docs[0].text[:200])
```

## Supported platforms

Every platform `pdfluent` 1.1.0 ships a wheel for: macOS (universal2),
Windows x64, Linux x86_64 and arm64 on glibc (manylinux2014) and musl
(musllinux_1_2) — Python 3.8 through 3.13 from one abi3 wheel per platform.

## Quality

`quality="fast"` is the only level in v1; anything else is refused by name.
Further levels (vision/OCR) are designed later.

## Licence

AGPL-3.0 or a commercial licence, at your option; see `LICENSE` and
`LICENSE-COMMERCIAL`. The commercial licence is sold yearly and self-service
at <https://pdfluent.com/sdk/pricing/>.
