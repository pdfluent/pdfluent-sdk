# PDFluent

**A pure-Rust PDF SDK: render, extract, edit, sign, redact, and validate PDF/A — from Rust, Python, Node.js, .NET, Java, C, or the browser.**

[![crates.io](https://img.shields.io/crates/v/pdfluent?label=crates.io&color=111111)](https://crates.io/crates/pdfluent)
[![PyPI](https://img.shields.io/pypi/v/pdfluent?label=PyPI&color=111111)](https://pypi.org/project/pdfluent/)
[![npm](https://img.shields.io/npm/v/%40pdfluent%2Fnode?label=npm&color=111111)](https://www.npmjs.com/package/@pdfluent/node)
[![NuGet](https://img.shields.io/nuget/v/PDFluent?label=NuGet&color=111111)](https://www.nuget.org/packages/PDFluent)
[![Maven Central](https://img.shields.io/maven-central/v/com.pdfluent/pdfluent?label=Maven&color=111111)](https://central.sonatype.com/artifact/com.pdfluent/pdfluent)
[![Licence](https://img.shields.io/badge/licence-AGPL--3.0--only%20OR%20Commercial-111111)](LICENSE)
[![Discussions](https://img.shields.io/badge/discussions-ask%20a%20question-111111?logo=github)](https://github.com/pdfluent/pdfluent-sdk/discussions)
[![Public pull request](https://img.shields.io/github/actions/workflow/status/pdfluent/pdfluent-sdk/public-pull-request.yml?branch=main&label=checks&color=111111)](https://github.com/pdfluent/pdfluent-sdk/actions/workflows/public-pull-request.yml)

[Share on X](https://x.com/intent/tweet?text=PDFluent%20%E2%80%94%20a%20pure-Rust%20PDF%20SDK%20with%20bindings%20for%20Python%2C%20Node.js%2C%20.NET%2C%20Java%2C%20C%20and%20WebAssembly&url=https%3A%2F%2Fgithub.com%2Fpdfluent%2Fpdfluent-sdk) ·
[Share on LinkedIn](https://www.linkedin.com/sharing/share-offsite/?url=https%3A%2F%2Fgithub.com%2Fpdfluent%2Fpdfluent-sdk)

![A statement printed to PDF, and the same statement after PDFluent redacted the
bank line, watermarked it and converted it to PDF/A-1B](docs/readme-hero.png)

<sup>Not a mock-up: `cargo run --example readme_hero` rebuilds this image from
[`docs/hero/`](docs/hero/), and both panels — and the picture itself — come out
of PDFluent's own rasterizer. The generator refuses to write it if the redacted
text is still extractable or `validate_pdfa()` disagrees.</sup>

## Install

```bash
cargo add pdfluent                  # Rust
pip install pdfluent                # Python
npm install @pdfluent/node          # Node.js
npm install @pdfluent/sdk-wasm      # Browser (WebAssembly)
dotnet add package PDFluent         # .NET
```

Java 1.1.0 is served from the PDFluent Maven repository at
<https://maven.pdfluent.com>, so add it next to the dependency. Maven Central
carries 1.0.0; 1.1.x follows there later.

```xml
<!-- Java (Maven) -->
<repositories>
  <repository>
    <id>pdfluent</id>
    <url>https://maven.pdfluent.com</url>
  </repository>
</repositories>

<dependencies>
  <dependency>
    <groupId>com.pdfluent</groupId>
    <artifactId>pdfluent</artifactId>
    <version>1.1.0</version>
  </dependency>
</dependencies>
```

```kotlin
// Java (Gradle, Kotlin DSL)
repositories {
    mavenCentral()
    maven { url = uri("https://maven.pdfluent.com") }
}

dependencies {
    implementation("com.pdfluent:pdfluent:1.1.0")
}
```

C and C++ link against `pdf-capi`; [SETUP.md](SETUP.md) has the header and the
build flags.

### Command-line tool

The `pdfluent` CLI ships as archives on the [v1.1.0 release
page](https://github.com/pdfluent/pdfluent-sdk/releases/tag/v1.1.0), built with
the model features. The `pdfluent` command comes from those archives. `pip
install pdfluent` and `npm install @pdfluent/node` do not install it, and no
Python or Node function installs models. Install the models once:

```sh
pdfluent models install layout tables
```

`pdfluent-cli` is an alias for the same binary.

## In thirty seconds

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

[QUICKSTART.md](QUICKSTART.md) is the same five minutes in each language, and
<https://pdfluent.com/cookbook/> has ten recipes with the explanation around
them. Every block on that page is a `// site:<name>` marker in
[`crates/pdfluent/examples/site_snippets.rs`](crates/pdfluent/examples/site_snippets.rs),
which CI compiles — an example that stops building breaks the build rather than
a reader's first attempt.

## How it compares

Every figure in this README carries a claim ID. The figures, their method and
their limits are in the [1.1.0 release
notes](https://github.com/pdfluent/pdfluent-sdk/releases/tag/v1.1.0) and the
evidence bundle (<EVIDENCE_BUNDLE_URL>).

## What is in here

The engine, the language bindings and the guards that gate them are in this one
repository.

**Structured extraction (Preview).** Every binding returns Markdown, blocks
and Docling JSON. Rust, Python, Node and the CLI also return words with boxes,
and Python, Node and the CLI return chunks for retrieval pipelines [B51].
Blocks are paragraphs,
headings with level, list items, table cells with grid position, and running
heads, each with page and box. Output quality is still improving and the API
may change in a minor release. The `fast` tier runs everywhere. The optional
`balanced` tier runs the layout and table-structure models locally.

**Rendering.** A pure-Rust rasterizer (vello_cpu) to PNG, JPEG or a rasterized
PDF, with SSIM comparison against a reference renderer.

**Text and data.** Extraction with position information, find-and-replace in
content streams, OCR over scanned pages.

**Forms.** AcroForm fill, flatten and read-back across every binding. XFA is
experimental and feature-gated behind `xfa-flatten`: dynamic layout, a FormCalc
interpreter with the standard built-in functions, and SOM path resolution
(`xfa.form.subform[3].field[*]`). It is not production-supported.

**Documents.** Merge, split, insert, delete and rearrange pages; AES-256 (PDF
2.0) encryption; watermarks; redaction that removes the content rather than
covering it.

**PDF/A.** Validation and conversion for A-1, A-2 and A-3, including ZUGFeRD and
Factur-X invoices, with font embedding and subsetting, OutputIntent injection
and XMP metadata repair.

**Signatures.** PAdES signing and verification.

## Quality tiers

Extraction runs in one of two tiers:

- `fast` (default): the engine alone — pure Rust, no models, everywhere
  including WASM.
- `balanced` (preview): the layout model regions each page — headings,
  lists, tables, page furniture — and the table-structure model
  structures tables, all run locally through ONNX Runtime 1.23.2.
  Needs the model files (`pdfluent models install layout tables`) and
  does more work per page than `fast`. Native only, never WASM.
  Balanced/tables: glibc Linux, macOS, Windows; not on musl Linux
  (loading returns a typed ModelError).

`balanced` scored higher than Docling on FinTabNet and PubTables-1M, level with
it on ParseBench, and ahead of pymupdf4llm on FinTabNet and ParseBench
[A38, A39, A40, 2026-10-14]. On PubTables-1M against pymupdf4llm our score is
higher, but not above our pre-set threshold of 0.05. The margin is +0.081
[+0.046; +0.127] on 120 pages from 55 documents, from the development (tuning)
split [A39, 2026-10-14]. Docling had more fully perfect ParseBench tables in an
earlier round [A37, 2026-10-08]. All three come from the development (tuning)
split, which we used while building the table work. Our side is an upper bound,
not a held-out result. The table-structure model we ship is Microsoft's Table
Transformer v1.1-all, trained on PubTables-1M and FinTabNet.c. We have not
checked whether our sample pages fall inside that training data, so the
FinTabNet and PubTables-1M results may favour us for that reason too. Table
structure only. Numbers, sample sizes and intervals: [1.1.0 release
notes](https://github.com/pdfluent/pdfluent-sdk/releases/tag/v1.1.0),
<EVIDENCE_BUNDLE_URL>.

The Python wheel, the Node addon and the CLI archives ship with the model
features (`ort-layout`, `ort-tables`) already built in. The `pdfluent` command
comes from the CLI archives on the release page, not from pip or npm.

| Tier | Needs | Runs | Status |
|---|---|---|---|
| `fast` | Nothing | Everywhere, including WASM | Shipping |
| `balanced` | The `ort-layout` build (`ort-tables` for table structure) plus installed models | Native only | Preview |

```sh
pdfluent models install layout tables   # explicit, once; SHA-256 verified, never automatic
pdfluent models status
```

[Quality tiers](docs/STRUCTURED_EXTRACTION.md#quality-tiers) has the
whole story: enabling, per-language options, errors and licences.

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

Default features: `signing`, `pdfa`, `redaction`, `font-subset`.

## Community

Ask questions in [GitHub Discussions](https://github.com/pdfluent/pdfluent-sdk/discussions/categories/q-a), in the Q&A category. Report bugs in [this repository's issue tracker](https://github.com/pdfluent/pdfluent-sdk/issues/new/choose). Report security problems by email to [security@pdfluent.com](mailto:security@pdfluent.com), never in a public issue.

## Contributing

Issues labelled [`good first issue`](https://github.com/pdfluent/pdfluent-sdk/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22)
and [`help wanted`](https://github.com/pdfluent/pdfluent-sdk/issues?q=is%3Aissue+is%3Aopen+label%3A%22help+wanted%22)
are the ones we would most like a hand with.

Sign off every commit — `git commit -s` — and read
[CONTRIBUTING.md](CONTRIBUTING.md) before the first one: it says what the
sign-off certifies, what it does not, and which contributions need a CLA that
does not exist yet. A pull request here runs two checks and both must pass: no
commit publishes a personal address, and every commit written since the DCO
landed carries a matching `Signed-off-by`.

Security reports do not go in a public issue. [SECURITY.md](SECURITY.md) says
where they go.

[SETUP.md](SETUP.md) is the contributor onboarding, [CHANGELOG.md](CHANGELOG.md)
is what changed and when, and <https://pdfluent.com> is the product around it.
Runnable examples in Rust and a browser/WASM demo live in
[`pdfluent/examples`](https://github.com/pdfluent/examples). This engine also
ships inside [`pdfluent/pdfluent`](https://github.com/pdfluent/pdfluent), the
free desktop editor.

---

**External assets in this README.** The badges come from `img.shields.io`, a
third-party host serving an image into this page. It is not code, it runs
nothing in your clone, and it can be removed without touching the build. It is
listed here so nobody has to wonder later where an image in our README comes
from.

The two images that are not external — `docs/readme-hero.png` and
`docs/social-preview.png` — are ours outright. The hero is generated by
`cargo run --example readme_hero` from the HTML in `docs/hero/`; the document on
it is invented, and no customer document or test corpus appears in either.

## Licence

**AGPL-3.0 or a commercial licence**, at your option:

| file | what it is |
|---|---|
| [LICENSE](LICENSE) | the GNU Affero General Public License, version 3 — the FSF's text, verbatim |
| [LICENSE-COMMERCIAL](LICENSE-COMMERCIAL) | the PDFluent Commercial Licence |

`SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-PDFluent-Commercial`

Choose either. You do not need permission to choose the first.

`LICENSE` holds nothing but the AGPL itself, and that is deliberate: a licence
file with our explanation wrapped around it is not the AGPL, and neither a
reader nor a licence scanner can tell how much of what they are reading is the
FSF's. The explanation is here instead, where prose belongs, and
`scripts/ci/license_boundary.py` fails the build if `LICENSE` moves by one byte
from the text pinned at
[`docs/licensing/agpl-text-provenance.md`](docs/licensing/agpl-text-provenance.md).

### Which one you want

The AGPL is the default. No key is required to receive it, nothing expires, and
the rights it grants are not conditional on anything we do.

It asks something in return: if you convey the software, or let users interact
with a modified version over a network, you must offer those users the
corresponding source under the same licence.

If that is impossible for you — a closed surrounding product, a customer
contract, a legal department that will not accept copyleft in a shipped
binary — the commercial licence is the alternative. Under it you are not buying
features. You are buying the right not to publish your own source. It is what
[pdfluent.com/sdk/pricing](https://pdfluent.com/sdk/pricing) sells, yearly and
self-service, in four options: Commercial (per organisation), OEM Startup and
OEM (per product), and Priority support (an add-on). Enquiries go to
<sales@pdfluent.com>.

**There is nothing to activate.** Since 6 September 2026 the compiled SDK
carries no tier check and reads no key: every capability is present in every
build, and nothing in the output marks whether a licence was bought. Earlier
builds still contained checks left over from the proprietary model while the
licence file of that time said there were none; this paragraph replaced the
sentence that was wrong, and it will be updated again the day it stops being
true.

### Not everything here is ours to license

Several crates in this repository are forks of third-party open-source projects
and remain under their own permissive licences. They are not PDFluent's to
relicense and need no licence from PDFluent.

| file | what it holds |
|---|---|
| [NOTICE](NOTICE) | which crates, and under what |
| [`docs/licensing/boundary.toml`](docs/licensing/boundary.toml) | the same thing, machine-readable |
| [THIRD_PARTY_LICENSES.txt](THIRD_PARTY_LICENSES.txt) | attribution for everything we ship |

`scripts/ci/license_boundary.py` fails the build if a crate stops agreeing with
the side of that boundary it is recorded on.

The rendering path is built on [`hayro`](https://github.com/pdfluent/hayro),
Laurenz Stampfl's pure-Rust PDF interpreter — mirrored under the PDFluent
org, unmodified license terms as recorded in NOTICE above.

**Is the SDK covered by the free PDFluent editor licence?** No. The PDFluent
desktop editor is free to use, including at work, but that licence covers the
application itself. Embedding, linking, or calling this SDK (or any of its
crates or language bindings) from your own software is covered by the two
licences above.

### History

Until 1 September 2026 `LICENSE` held the PDFluent Commercial License v1.0 of
2 May 2026, and that was the only licence offered. It described licence keys,
tiers and an expiring evaluation, none of which exist. It is kept, unaltered, at
[`docs/licensing/archive/LICENSE-COMMERCIAL-v1.0-2026-05-02.txt`](docs/licensing/archive/LICENSE-COMMERCIAL-v1.0-2026-05-02.txt),
because versions published under it are still out there and a licensee is
entitled to read the terms they agreed to.

From 1 September 2026 `LICENSE` carried the two-licence explanation above and
the AGPL text sat beside it in `LICENSE-AGPL`. Since 7 September 2026 there is
one AGPL text, in `LICENSE`, and this section is the explanation.
