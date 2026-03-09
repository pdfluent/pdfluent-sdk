# pdf-capi — C API for XFA PDF Engine

C-compatible API layer providing a stable ABI for the PDF engine. This is the foundation for the .NET (P/Invoke), iOS (Swift/C interop), and other FFI bindings.

## Building

```bash
# Shared library (.dylib / .so / .dll)
cargo build -p pdf-capi --release

# Static library (.a / .lib)
cargo build -p pdf-capi --release
```

Output: `target/release/libpdf_capi.{dylib,so}` or `pdf_capi.dll`

## C Header

See [`pdf_capi.h`](pdf_capi.h) for the complete API declaration.

## Example

See [`examples/basic.c`](examples/basic.c) for a minimal working example:

```bash
cargo build -p pdf-capi --release
cc -o basic examples/basic.c -L../../target/release -lpdf_capi -I.
./basic input.pdf
```

## API Overview

| Function | Description |
|----------|-------------|
| `pdf_init()` / `pdf_destroy()` | Initialize/teardown the library |
| `pdf_document_open(path, pw, &doc)` | Open PDF from file path |
| `pdf_document_open_from_bytes(data, len, pw, &doc)` | Open PDF from memory |
| `pdf_document_free(doc)` | Free a document handle |
| `pdf_document_page_count(doc)` | Number of pages |
| `pdf_page_width/height/rotation(doc, i)` | Page geometry |
| `pdf_page_render(doc, i, dpi, &w, &h, &pixels)` | Render page to RGBA |
| `pdf_page_render_thumbnail(doc, i, max, &w, &h, &px)` | Render thumbnail |
| `pdf_page_extract_text(doc, i)` | Extract text (caller frees) |
| `pdf_document_get_meta(doc, key)` | Read metadata (caller frees) |
| `pdf_page_media_box/crop_box(doc, i, &box)` | Page boxes |
| `pdf_bookmark_count(doc)` | Number of bookmarks |
| `pdf_get_last_error()` | Last error message |
| `pdf_version()` | Library version string |

## Thread Safety

Each `PdfDocument*` handle must be used from a single thread. The error state is thread-local.
