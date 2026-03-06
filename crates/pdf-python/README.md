# pdfengine

High-performance PDF engine for Python — rendering, text extraction, forms, and signatures. Built on a pure-Rust PDF stack via PyO3.

## Installation

```bash
# From source (requires Rust toolchain + maturin)
cd crates/pdf-python
maturin develop --release

# With PIL/NumPy support
pip install Pillow numpy
```

## Quick Start

### Open a PDF and render pages

```python
from pdfengine import Document

with Document("invoice.pdf") as doc:
    print(f"{doc.page_count} pages")
    print(f"Title: {doc.metadata.title}")

    # Render first page to PIL Image
    img = doc[0].render(dpi=150)
    img.to_pil().show()

    # Save as PNG
    img.save("page_0.png")
```

### Extract text

```python
from pdfengine import Document

doc = Document("report.pdf")
for page in doc:
    text = page.extract_text()
    print(f"Page {page.index}: {text[:100]}...")
```

### Structured text with positions

```python
from pdfengine import Document

doc = Document("invoice.pdf")
for block in doc[0].extract_text_blocks():
    for span in block.spans:
        print(f"  [{span.x:.0f}, {span.y:.0f}] {span.text}")
```

### Thumbnails

```python
from pdfengine import Document

doc = Document("presentation.pdf")
for page in doc:
    thumb = page.thumbnail(max_dimension=128)
    thumb.save(f"thumb_{page.index}.png")
```

### Search text

```python
from pdfengine import Document

doc = Document("manual.pdf")
pages = doc.search("configuration")
print(f"Found on pages: {pages}")
```

### NumPy array output

```python
from pdfengine import Document
import numpy as np

doc = Document("chart.pdf")
arr = doc[0].render(dpi=300).to_numpy()
print(f"Shape: {arr.shape}")  # (height, width, 4)
```

### Password-protected PDFs

```python
from pdfengine import Document

doc = Document("secret.pdf", password="hunter2")
print(doc.page_count)
```

## API Reference

### `Document(source, password=None)`

Open a PDF from a file path (str) or raw bytes.

- **Properties:** `page_count`, `metadata`, `bookmarks`
- **Methods:** `render_all(dpi)`, `search(query)`
- **Protocols:** `len()`, `[]` indexing, `for` iteration, `with` context manager

### `Page`

- **Properties:** `index`, `width`, `height`, `rotation`, `geometry`
- **Methods:** `render(dpi, width, height, background)`, `thumbnail(max_dimension)`, `extract_text()`, `extract_text_blocks()`

### `RenderedImage`

- **Properties:** `width`, `height`, `pixels`
- **Methods:** `to_pil()`, `to_numpy()`, `save(path)`

### `TextBlock` / `TextSpan`

Structured text extraction with position and font size information.

### `DocumentInfo`

Document metadata: `title`, `author`, `subject`, `keywords`, `creator`, `producer`.

### `Bookmark`

Document outline: `title`, `page`, `children`.

### `PageGeometry`

Page boxes and rotation: `media_box`, `crop_box`, `rotation`, `width`, `height`, `pixel_dimensions(dpi)`.
