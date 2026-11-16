# markitdown-pdfluent

A [MarkItDown](https://github.com/microsoft/markitdown) plugin that converts PDFs with the
[PDFluent](https://pdfluent.com) engine. It runs on your machine and needs no network.

## Install

```bash
pip install markitdown-pdfluent
```

## Use

Plugins are off by default in MarkItDown. Turn them on:

```python
from markitdown import MarkItDown

md = MarkItDown(enable_plugins=True)
print(md.convert("report.pdf").markdown)
```

From the command line:

```bash
markitdown --use-plugins report.pdf
```

The plugin registers ahead of MarkItDown's built-in PDF converter, so PDFs go through PDFluent and
every other format keeps using MarkItDown's own converters.

## Licence

This plugin is MIT licensed; see `LICENSE`. The PDFluent engine it calls (`pdfluent` on PyPI) is
licensed separately, under AGPL-3.0 or a commercial licence.
