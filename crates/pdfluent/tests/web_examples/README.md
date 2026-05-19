# web_examples

Bootstrap test suite that locks the public API surface against the
how-to snippets shipped at <https://pdfluent.com/how-to>.

## Purpose

Each file in this directory is a Rust code snippet copied from a how-to guide on <https://pdfluent.com/how-to>. The tests validate that the `pdfluent` public API accepts the snippet verbatim — correct imports, method names, types, and signatures — and, where Epic 2 wiring allows, actually runs the snippet against a fixture.

If an example cannot yet run because its underlying method is still `unimplemented!()` in the scaffold, the test is marked `#[ignore]` with a link to the Epic 2 sub-issue blocking it. Remove the `#[ignore]` attribute in the same PR that lands the wiring.

## Layout

```
tests/web_examples/
├── mod.rs                        # registers every example
├── README.md                     # this file
├── merge_pdfs_rust.rs            # /how-to/merge-pdfs-rust
├── fill_pdf_form_rust.rs         # /how-to/fill-pdf-form-rust
├── extract_text_pdf_rust.rs      # /how-to/extract-text-pdf-rust
├── encrypt_pdf_rust.rs           # /how-to/encrypt-pdf-rust
└── render_pdf_to_png_rust.rs     # /how-to/render-pdf-to-png-rust
```

The `tests/web_examples.rs` entry file wires these into a single integration-test binary so they share a compile pass.

## Adding a new example

1. Copy the Rust snippet verbatim from the website page.
2. Add the file under `tests/web_examples/<slug>.rs` (same slug as URL).
3. Register it in `mod.rs`.
4. Wrap in `pub fn run() -> Result<()>` + two `#[test]` functions:
   - `<slug>_compiles` (always runs, proves the API surface accepts it).
   - `<slug>_runs` (marked `#[ignore]` until wiring allows actual run).
5. Add a header comment citing the source URL and fetch date.

## CI

GitHub Actions workflow `.github/workflows/web-examples-bootstrap.yml` runs `cargo test -p pdfluent --tests web_examples` on every PR touching `crates/pdfluent/` or this directory. Target doorlooptijd: < 30 s.

## Pre-freeze validation

Per RFC 0001 and issue #1239 (API Design Freeze), at least 2 of these examples must compile before `#1239` may close. The `_compiles` tests are the binding check.
