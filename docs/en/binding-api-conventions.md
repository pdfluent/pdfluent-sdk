# PDFluent — Binding API Conventions

How the canonical `pdfluent` facade maps across bindings, and how
intentionally-unsupported APIs are documented. Backed by
`scripts/bindings/check_binding_api_parity.py`
(`BINDING_API_PARITY_TRUE_100_PERCENT_GREEN`; 147 total / 128 supported /
19 intentionally_unsupported).

## Naming conventions per language

| Concept | Rust | C ABI | WASM/Node/TS | Python | .NET | Java |
| --- | --- | --- | --- | --- | --- | --- |
| Open | `PdfDocument::open` | `pdf_document_open*` | `PdfDocument.open` | `PdfDocument.open` | `PdfDocument.Open` | `PdfDocument.open` |
| Page count | `page_count()` | `pdf_page_count` | `pageCount` | `page_count()` | `PageCount` | `pageCount()` |
| Extract text | `extract_text()` | `pdf_document_extract_text*` | `extractText` | `extract_text()` | `ExtractText` | `extractText()` |
| Errors | `Result<_, Error>` | `PdfStatus` codes | thrown `Error`/typed | `PdfluentError` | exceptions | exceptions |

Convention: `snake_case` (Rust/Python), `camelCase` (TS/Node/WASM/Java),
`PascalCase` (.NET), `pdf_*` C functions with `PdfStatus` return codes.

## Intentionally-unsupported APIs (19)

The 19 `intentionally_unsupported` cells reported by the parity checker are
per-binding-by-design (e.g. a Rust-only builder not exposed over the C ABI).
They are tracked in the parity checker's data and are **not** gaps; each is
an explicit per-binding scope decision, not a missing feature. See the
parity checker output for the authoritative list.

## XFA

XFA dynamic-form capabilities are a **separate beta track** and are labelled
as such in the feature comparison; they are not part of the non-XFA binding
parity surface covered here.
