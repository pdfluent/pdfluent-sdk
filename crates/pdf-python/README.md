# pdfluent

**Enterprise PDF SDK for Python — built on a pure-Rust stack, zero system dependencies.**

Render pages, extract text, fill forms, annotate, redact, encrypt, merge, and validate PDF/A — all from a single `pip install`.

## Installation

```bash
pip install pdfluent

# Optional extras
pip install pdfluent[pillow]   # PIL Image support
pip install pdfluent[numpy]    # NumPy array support
```

> Requires Python ≥ 3.8. Pre-built wheels for Linux (x86_64, aarch64), macOS (x86_64, arm64), and Windows (x86_64).

## Quick Start

```python
from pdfluent import Document

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
# Multi-select list box: select several options at once
doc.set_multi_select("Languages", ["EN", "NL"])
doc.save("form_filled.pdf")

# Search-and-redact
doc = Document("contract.pdf")
report = doc.redact_text("Confidential")
print(f"Redacted {report.areas_redacted} areas on {report.pages_affected} pages")
doc.save("contract_redacted.pdf")

# PDF/A validation
from pdfluent import validate_pdfa

report = validate_pdfa("archive.pdf")
if report.is_compliant:
    print(f"✓ {report.pdfa_level} compliant")
else:
    for issue in report.issues:
        print(f"[{issue.severity}] {issue.rule}: {issue.message}")

# Merge PDFs
from pdfluent import merge_pdfs
merge_pdfs(["a.pdf", "b.pdf", "c.pdf"], "merged.pdf")

# Encrypt / decrypt
doc = Document("sensitive.pdf")
doc.encrypt("sensitive_enc.pdf", password="s3cr3t")

from pdfluent import decrypt_pdf
decrypt_pdf("sensitive_enc.pdf", "sensitive_dec.pdf", password="s3cr3t")
```

## Features

| Feature | Description |
|---|---|
| **Render** | Pages to RGBA pixels, PIL Images, or NumPy arrays at any DPI |
| **Text extraction** | Plain text or structured `TextBlock`/`TextSpan` with position |
| **Text search** | Find pages containing a query string |
| **Forms (AcroForm)** | Read and fill text, checkbox, radio, combo/list fields; **multi-select list boxes** (`set_multi_select`); hierarchical names; appearance-stream regeneration |
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

#### AcroForm fill behaviour

`set_form_field(name, value)` supports text, checkbox, radio, and choice
(combo/list) fields.  Hierarchical names (`"parent.child"`) are resolved
through `/Kids` recursion.  Every call keeps `/V`, `/AS`, and `/AP` consistent
so the fill is visible in all PDF viewers without needing `/NeedAppearances`
processing.

```python
doc.set_form_field("Name", "Jane Doe")          # text field
doc.set_form_field("Agree", "On")               # checkbox — pass on-state name
doc.set_form_field("Country", "NL")             # radio group — export value
doc.set_form_field("Category", "Urgent")        # combo / list — option value
doc.save("form_filled.pdf")
```

Read-only fields raise `PdfluentError`.

### `Page`

**Properties:** `index`, `width`, `height`, `rotation`, `geometry`  
**Methods:** `render(dpi, width, height, background)`, `thumbnail(max_dimension)`,
`extract_text()`, `extract_text_blocks()`

### `RenderedImage`

**Properties:** `width`, `height`, `pixels` (raw RGBA bytes)  
**Methods:** `to_pil()`, `to_numpy()`, `save(path)`

### `TextSpan`

Structured text with position data.

**Properties:** `text`, `x`, `y`, `font_size`  
**G1 font-metadata (Optional):** `font_name`, `is_bold`, `is_italic`, `color`

> G1 fields return `None` in the current release. They are typed as `Optional`
> so downstream code handles the `None` case correctly today and will
> automatically receive data once the G1 extraction milestone lands.

```python
for block in page.extract_text_blocks():
    for span in block.spans:
        if span.font_name is not None:
            print(f"{span.font_name} {'bold' if span.is_bold else ''}")
        print(f"  '{span.text}' @ ({span.x:.1f}, {span.y:.1f})")
```

### Module-level functions

| Function | Description |
|---|---|
| `open_pdf(path, password=None)` | Alias for `Document(path)` |
| `merge_pdfs(paths, output)` | Merge a list of PDFs |
| `validate_pdfa(path)` → `ComplianceReport` | Run PDF/A validation |
| `decrypt_pdf(input, output, password)` | Decrypt to a new file |

## Exception Hierarchy

Every pdfluent-specific error derives from `PdfluentError`, so a single
`except PdfluentError:` clause catches all library errors:

```python
from pdfluent import PdfluentError, PdfluentParseError, PdfluentEncryptedError

try:
    with Document("broken.pdf") as doc:
        doc.render_all()
except PdfluentParseError as exc:
    print(f"Not a valid PDF: {exc}")
except PdfluentEncryptedError:
    print("PDF is password-protected")
except PdfluentError as exc:
    print(f"PDF error: {exc}")
```

Full hierarchy:

```
PdfluentError                 — base; catch all pdfluent errors
├── PdfluentParseError        — corrupt / non-PDF bytes
├── PdfluentValidationError   — schema / compliance failures
├── PdfluentRenderError       — rendering and XFA flatten failures
├── PdfluentEncryptedError    — operation blocked by encryption
├── PdfluentPageRangeError    — page index out of range
├── PdfluentIoError           — file-system I/O errors
├── PdfluentLicenseError      — invalid / expired license
├── PdfluentGeometryError     — invalid page geometry
└── PdfluentLimitError        — processing-limit exceeded
```

## Typing Support

pdfluent ships with hand-written `.pyi` stub files for IDE completion and
`mypy --strict` compatibility:

- `pdfluent/__init__.pyi` — full public API stubs
- `pdfluent/_native.pyi` — native extension stubs (for mypy without a build)

### Verifying with mypy

```bash
pip install mypy
cd crates/pdf-python
mypy --strict --python-path python tests/test_pdfluent_typing.py
```

### Example with typed annotations

```python
from __future__ import annotations
from typing import Optional
from pdfluent import Document, TextSpan, PdfluentError

def get_font(span: TextSpan) -> Optional[str]:
    """Return the font name if available."""
    return span.font_name   # Optional[str] — mypy knows this may be None

def safe_open(path: str) -> Optional[Document]:
    try:
        return Document(path)
    except PdfluentError:
        return None
```

## License Activation

```python
from pdfluent import activate_license, LicenseInfo, PdfluentLicenseError

# Activate from a JSON license string or base64-encoded key
try:
    info: LicenseInfo = activate_license(open("my.license").read())
    print(f"{info.tier} license for {info.company} ({info.seats} seats)")
except PdfluentLicenseError as exc:
    print(f"License error: {exc}")

# Or set the environment variable and call with empty string:
# PDFLUENT_LICENSE_KEY="<base64-key>" python myscript.py
info = activate_license("")   # reads PDFLUENT_LICENSE_KEY from env
```

`LicenseInfo` fields: `licensee`, `company`, `tier`, `expires_at` (Unix timestamp), `seats`.

## Comparison

| | pdfluent | pypdf | pdfminer | pdfplumber | pikepdf |
|---|---|---|---|---|---|
| Rendering | ✓ | – | – | ✓ (via pdfminer) | – |
| Text extraction | ✓ | ✓ | ✓ | ✓ | – |
| Form fill | ✓ | ✓ | – | – | ✓ |
| Redaction | ✓ | – | – | – | ✓ |
| Encryption | ✓ (AES-256) | ✓ | – | – | ✓ |
| PDF/A validation | ✓ | – | – | – | – |
| Typed stubs | ✓ | partial | – | – | – |
| Native deps | **none** | none | none | none | libqpdf |
| Language | **Rust** | Python | Python | Python | C++ |

## License Activation

The SDK runs in Trial mode by default; output is marked via `/Producer`
metadata. Activate a license to unlock the paid-tier capability set.

```python
import pdfluent

# Activate from a key string
pdfluent.activate_license_key("tier:enterprise")

# Or read the key from a UTF-8 text file
pdfluent.activate_license_file("/path/to/key.lic")

# Inspect the current status (always succeeds; defaults to Trial)
status = pdfluent.license_status()
print(status.tier)              # "Enterprise"
print(status.source)            # "Explicit" | "EnvVar" | "Default"
print(status.output_is_marked)  # False
```

The `PDFLUENT_LICENSE_KEY` environment variable is honoured automatically
on process start when no explicit activation has happened.

**Behavior to be aware of:**

- The active tier is **process-global and set-once**. Re-activating with the
  same key is a no-op. Re-activating with a different tier raises
  `RuntimeError`; restart Python to switch tiers.
- Invalid keys raise `ValueError`; missing license files raise `OSError`.
- The key string is never logged or stored beyond the call to
  `activate_license_key`.

The 1.0 release accepts the simple evaluation format `tier:<name>`
(`trial`/`developer`/`team`/`business`/`enterprise`). Cryptographically
signed payloads will be accepted by the same functions in 1.1 without
breaking the API.

## Building from Source

Requires a Rust toolchain and `maturin`.

```bash
pip install maturin
cd crates/pdf-python
maturin develop --release          # install in current venv
maturin build --release            # build wheel in ./dist/
```

## License

PDFluent Commercial License. See LICENSE.
