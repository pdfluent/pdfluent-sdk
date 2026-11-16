# pdfluent-haystack

A [Haystack](https://haystack.deepset.ai) converter for [PDFluent](https://pdfluent.com). It reads a
PDF into structure-aware `Document` chunks in reading order, each with the page, box and kind it came
from in its `meta`, so an answer can cite its source.

## Install

```bash
pip install pdfluent-haystack            # hosted API mode
pip install "pdfluent-haystack[local]"   # also the local engine
```

## Two modes

Local mode works today. API mode needs the hosted PDFluent API, which is not generally available yet.

```python
from pdfluent_haystack import PDFluentConverter

# Local: the engine runs on this machine and the PDF does not leave it.
docs = PDFluentConverter().run(sources=["report.pdf"])["documents"]

# API: with a key (or PDFLUENT_API_KEY) the PDF is sent to the hosted PDFluent API for that call.
# The service keeps no document. Chunking options are API-only: by_title, basic, by_page.
docs = PDFluentConverter(api_key="...", chunking_strategy="by_title").run(sources=["report.pdf"])["documents"]
```

`sources` takes paths or `ByteStream` objects. `meta` is merged into every document of a source.
Pages count from 0, as in the other PDFluent loaders. The key is read from the argument or the
environment, sent in the `x-api-key` header, and never appears in a repr or an error.

## Licence

This integration is MIT licensed; see `LICENSE`. The PDFluent SDK it calls in local mode (`pdfluent` on PyPI) is licensed separately, under AGPL-3.0 or a commercial licence.
