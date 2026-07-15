# PDFluent SDK examples

Minimal, runnable examples for the [PDFluent SDK](https://pdfluent.com/docs) —
a pure-Rust PDF engine with bindings for Rust, Python, Node.js, .NET, Java, C,
and WebAssembly.

Each example follows the same golden path: open a PDF, read its metadata,
extract text, and handle errors with the binding's typed exception hierarchy.
Every example works out of the box against an in-repo test fixture, and also
accepts a path to your own PDF.

## Examples by language

| Language | Path | Install |
|---|---|---|
| Rust | [`rust/`](rust/) | `cargo add pdfluent` |
| Python | [`python/`](python/) | `pip install pdfluent` |
| Node.js | [`node/strict-ts/`](node/strict-ts/) | `npm install @pdfluent/node` |
| .NET | [`dotnet/StrictApi/`](dotnet/StrictApi/) | `dotnet add package PDFluent` |
| Java | [`java/StrictApi/`](java/StrictApi/) | `com.pdfluent:pdfluent` (Maven Central) |
| C | [`c/strict-api/`](c/strict-api/) | see [C ABI docs](https://pdfluent.com/docs) |
| WebAssembly | [`wasm/strict-ts-edit/`](wasm/strict-ts-edit/) | `npm install @pdfluent/sdk-wasm` |

## Status

These examples currently cover **open + read metadata + extract text**. Render,
fill-and-save a form, and save-to-file examples per language are in progress —
track progress in the SDK repository. Version pins in each example predate the
current SDK release; treat them as illustrative until the pins are refreshed.

## Documentation

- SDK docs: <https://pdfluent.com/docs>
- Capability matrix and pricing: <https://pdfluent.com/pricing>
- Report an issue with an example: <https://pdfluent.com/support>
