# PDFluent SDK

Pure Rust PDF/A SDK with WASM bindings and experimental, feature-gated XFA support.

![Crates.io](https://img.shields.io/crates/v/pdfluent)
![License](https://img.shields.io/badge/license-AGPL--3.0--only%20OR%20Commercial-blue)

The engine, the language bindings and the guards that gate them are in this one
repository. [SETUP.md](SETUP.md) is the contributor onboarding,
[CHANGELOG.md](CHANGELOG.md) is what changed and when, and
<https://pdfluent.com> is the product around it.

## Cookbook

Ten recipes with the explanation around them: <https://pdfluent.com/cookbook/>.

Every block on that page is one of the `// site:<name>` markers in
[`crates/pdfluent/examples/site_snippets.rs`](crates/pdfluent/examples/site_snippets.rs),
which CI compiles, and each recipe links back to it. An example that stops
building breaks the build rather than a reader's first attempt (#164, #247).

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let doc = PdfDocument::open("input.pdf")?;
    for page in doc.pages() {
        println!("{}", page.text()?);
    }
    let report = doc.validate_pdfa(PdfAProfile::A2b)?;
    if report.is_compliant() {
        println!("PDF/A-2B ✓");
    }
    Ok(())
}
```

---

## Features

### XFA Forms (experimental, feature-gated — not production-supported)
- **XFA 3.3 feature set** — dynamic forms with scriptable calculations (experimental, behind the `xfa` feature gate)
- **Font embedding pipeline** — automatic resolution of embedded, system, and fallback fonts
- **Image embedding** — JPEG/PNG XObjects with alpha transparency via SMask
- **Layout engine** — positioned, flowed (tb/lr-tb), and table layouts with pagination
- **FormCalc interpreter** — 90+ built-in functions for form calculations
- **SOM path resolution** — `xfa.form.subform[3].field[*]` expressions

### PDF Rendering
- **Pure Rust rasterizer** — vello_cpu for memory-safe rendering
- **Multi-format output** — PNG, JPEG, PDF (rasterized)
- **SSIM quality metrics** — visual comparison against Adobe Reader ground truth

### PDF/A Compliance
- **PDF/A-1a, A-2a, A-3a** — ZUGFeRD/Factur-X invoice support
- **Font embedding** — automatic embedding with subsetting
- **Color space normalization** — OutputIntent injection
- **Metadata repair** — XMP writer integration

### Document Manipulation
- **Page operations** — merge, split, insert, delete, rearrange
- **Encryption** — AES-256 PDF 2.0 encryption/decryption
- **Content editing** — find-and-replace text in content streams
- **Watermarks** — text and image overlay

### WASM & Bindings
- **WebAssembly** — runs in browser via the `@pdfluent/sdk-wasm` npm package
- **Node.js** — napi-rs bindings
- **Python** — PyO3 bindings (separate crate)
- **C FFI** — pdf-capi for C/C++ integration

## Build and test

Rust 1.94.0, pinned in [`rust-toolchain.toml`](rust-toolchain.toml) so `rustup`
installs it on the first `cargo` call. The default feature set needs no system
library beyond a C toolchain.

```
cargo build -p pdfluent            # the SDK crate
cargo test  -p pdfluent
cargo fmt --all --check
cargo clippy --workspace -- -D warnings
```

`cargo build --workspace` builds the bindings and the tools around the SDK as
well, which is what CI does and what takes the time.

Default features: `signing`, `pdfa`, `redaction`, `font-subset`. XFA sits behind
the `xfa-flatten` feature, is experimental and is not production-supported.

## Contributing

Sign off every commit — `git commit -s` — and read
[CONTRIBUTING.md](CONTRIBUTING.md) before the first one: it says what the
sign-off certifies, what it does not, and which contributions need a CLA that
does not exist yet. A pull request here runs two checks and both must pass: no
commit publishes a personal address, and every commit written since the DCO
landed carries a matching `Signed-off-by`.

Security reports do not go in a public issue. [SECURITY.md](SECURITY.md) says
where they go.

## How the measurements are made

Every figure PDFluent publishes carries a claim ID, and the method behind those
figures — the axes, the readers and their versions, the machine class, and what
counts as a failure — is written up at
<https://pdfluent.com/benchmarks/how-we-measure>. It deliberately carries no
figure of its own. This README carries none either.

## License

PDFluent is available under two licences, at your option: the **GNU AGPLv3**, or
the **PDFluent Commercial Licence**. See [LICENSE](LICENSE) — the AGPL is the
default and needs no key, no permission and nothing from us. The commercial
licence exists for buyers who cannot publish their own source; it is what
[pdfluent.com/sdk/pricing](https://pdfluent.com/sdk/pricing) sells.

Some dependencies are separately available under MIT or Apache-2.0, including
the forked crates this repository carries; [NOTICE](NOTICE) says which is which,
per crate.

**Is the SDK covered by the free PDFluent editor license?** No. The PDFluent
desktop editor is free to use, including at work, but that license covers the
application itself. Embedding, linking, or calling this SDK (or any of its
crates or language bindings) from your own software is covered by the two
licences above.
