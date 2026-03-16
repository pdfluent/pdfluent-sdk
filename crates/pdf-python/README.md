# xfa-pdf

**Enterprise PDF SDK for Python — built on a pure-Rust stack, zero system dependencies.**

Render pages, extract text, fill forms, annotate, redact, encrypt, merge, and validate PDF/A — all from a single `pip install`.

## Installation

```bash
pip install xfa-pdf

# Optional extras
pip install xfa-pdf[pillow]   # PIL Image support
pip install xfa-pdf[numpy]    # NumPy array support
```

> Requires Python ≥ 3.8. Pre-built wheels for Linux (x86_64, aarch64), macOS (x86_64, arm64), and Windows (x86_64).

## Quick Start

```python
from xfa_pdf import Document

# Open, inspect, render
with Document("invoice.pdf") as doc:
    print(f"{doc.page_count} pages — {doc.metadata.title}")

    img = doc[0].render(dpi=150)
    img.save("page_0.png")          # requires Pillow

# Extract text
doc = Document("report.pdf")
for page in doc:
    print(page.extract_text())

# Fill a form field and save
doc = Document("form.pdf")
doc.set_form_field("Name", "Jane Doe")
doc.save("form_filled.pdf")

# Search-and-redact
doc = Document("contract.pdf")
report = doc.redact_text("Confidential")
print(f"Redacted {report.areas_redacted} areas on {report.pages_affected} pages")
doc.save("contract_redacted.pdf")

# PDF/A validation
from xfa_pdf import validate_pdfa

report = validate_pdfa("archive.pdf")
if report.is_compliant:
    print(f"✓ {report.pdfa_level} compliant")
else:
    for issue in report.issues:
        print(f"[{issue.severity}] {issue.rule}: {issue.message}")

# Merge PDFs
from xfa_pdf import merge_pdfs
merge_pdfs(["a.pdf", "b.pdf", "c.pdf"], "merged.pdf")

# Encrypt / decrypt
doc = Document("sensitive.pdf")
doc.encrypt("sensitive_enc.pdf", password="s3cr3t")

from xfa_pdf import decrypt_pdf
decrypt_pdf("sensitive_enc.pdf", "sensitive_dec.pdf", password="s3cr3t")
```

## Features

| Feature | Description |
|---|---|
| **Render** | Pages to RGBA pixels, PIL Images, or NumPy arrays at any DPI |
| **Text extraction** | Plain text or structured `TextBlock`/`TextSpan` with position |
| **Text search** | Find pages containing a query string |
| **Forms (AcroForm)** | Read and fill text, checkbox, and dropdown fields |
| **Annotations** | Read existing annotations; add highlights and free-text notes |
| **Redaction** | Search-and-redact: black-box all occurrences of a string |
| **Encryption** | AES-256 (PDF 2.0) encrypt/decrypt with user + owner passwords |
| **Merge / split** | Merge multiple PDFs; split into individual pages (via page slicing) |
| **PDF/A validation** | Validate against PDF/A-1B, 2B, 3B with issue-level reporting |
| **Metadata** | Read title, author, subject, keywords, creator, producer |
| **Bookmarks** | Traverse the document outline tree |
| **Thumbnails** | Fast downscaled preview images |

## API Overview

### `Document(source, password=None)`

Opens a PDF from a file path (`str`) or raw bytes.

```python
doc = Document("file.pdf")             # from path
doc = Document(open("file.pdf","rb").read())  # from bytes
doc = Document("encrypted.pdf", password="pw")
```

**Properties:** `page_count`, `metadata`, `bookmarks`
**Methods:** `render_all(dpi)`, `search(query)`, `extract_text(page_num)`, `save(path)`,
`get_form_fields()`, `set_form_field(name, value)`, `get_annotations(page)`,
`add_annotation(page, type, rect, content)`, `redact_text(term, page=None)`,
`encrypt(path, password)`, `decrypt(path, password)`
**Protocols:** `len(doc)`, `doc[0]`, `for page in doc`, `with Document(...) as doc`

### `Page`

**Properties:** `index`, `width`, `height`, `rotation`, `geometry`
**Methods:** `render(dpi, width, height, background)`, `thumbnail(max_dimension)`,
`extract_text()`, `extract_text_blocks()`

### `RenderedImage`

**Properties:** `width`, `height`, `pixels` (raw RGBA bytes)
**Methods:** `to_pil()`, `to_numpy()`, `save(path)`

### Module-level functions

| Function | Description |
|---|---|
| `open_pdf(path, password=None)` | Alias for `Document(path)` |
| `merge_pdfs(paths, output)` | Merge a list of PDFs |
| `validate_pdfa(path)` → `ComplianceReport` | Run PDF/A validation |
| `decrypt_pdf(input, output, password)` | Decrypt to a new file |

## Comparison

| | xfa-pdf | pypdf | pdfminer | pdfplumber | pikepdf |
|---|---|---|---|---|---|
| Rendering | ✓ | – | – | ✓ (via pdfminer) | – |
| Text extraction | ✓ | ✓ | ✓ | ✓ | – |
| Form fill | ✓ | ✓ | – | – | ✓ |
| Redaction | ✓ | – | – | – | ✓ |
| Encryption | ✓ (AES-256) | ✓ | – | – | ✓ |
| PDF/A validation | ✓ | – | – | – | – |
| Native deps | **none** | none | none | none | libqpdf |
| Language | **Rust** | Python | Python | Python | C++ |

## Building from Source

Requires a Rust toolchain and `maturin`.

```bash
pip install maturin
git clone https://github.com/xfa-sdk/xfa-pdf
cd xfa-pdf/crates/pdf-python
maturin develop --release          # install in current venv
maturin build --release            # build wheel in ./dist/
```

## License

MIT
