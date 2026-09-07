# PDFluent

**A pure-Rust PDF SDK: render, extract, edit, sign, redact, and validate PDF/A — from Rust, Python, Node.js, .NET, Java, C, or the browser.**

[![crates.io](https://img.shields.io/crates/v/pdfluent?label=crates.io&color=111111)](https://crates.io/crates/pdfluent)
[![PyPI](https://img.shields.io/pypi/v/pdfluent?label=PyPI&color=111111)](https://pypi.org/project/pdfluent/)
[![npm](https://img.shields.io/npm/v/%40pdfluent%2Fnode?label=npm&color=111111)](https://www.npmjs.com/package/@pdfluent/node)
[![NuGet](https://img.shields.io/nuget/v/PDFluent?label=NuGet&color=111111)](https://www.nuget.org/packages/PDFluent)
[![Maven Central](https://img.shields.io/maven-central/v/com.pdfluent/pdfluent?label=Maven&color=111111)](https://central.sonatype.com/artifact/com.pdfluent/pdfluent)
[![Licence](https://img.shields.io/badge/licence-AGPL--3.0--only%20OR%20Commercial-111111)](LICENSE)
[![Public pull request](https://img.shields.io/github/actions/workflow/status/pdfluent/pdfluent-sdk/public-pull-request.yml?branch=main&label=checks&color=111111)](https://github.com/pdfluent/pdfluent-sdk/actions/workflows/public-pull-request.yml)

[Share on X](https://x.com/intent/tweet?text=PDFluent%20%E2%80%94%20a%20pure-Rust%20PDF%20SDK%20with%20bindings%20for%20Python%2C%20Node.js%2C%20.NET%2C%20Java%2C%20C%20and%20WebAssembly&url=https%3A%2F%2Fgithub.com%2Fpdfluent%2Fpdfluent-sdk) ·
[Share on LinkedIn](https://www.linkedin.com/sharing/share-offsite/?url=https%3A%2F%2Fgithub.com%2Fpdfluent%2Fpdfluent-sdk)

## Install

```bash
cargo add pdfluent                  # Rust
pip install pdfluent                # Python
npm install @pdfluent/node          # Node.js
npm install @pdfluent/sdk-wasm      # Browser (WebAssembly)
dotnet add package PDFluent         # .NET
```

```xml
<!-- Java (Maven) -->
<dependency>
  <groupId>com.pdfluent</groupId>
  <artifactId>pdfluent</artifactId>
</dependency>
```

C and C++ link against `pdf-capi`; [SETUP.md](SETUP.md) has the header and the
build flags.

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
a reader's first attempt (#164, #247).

## How it compares

The axes, the readers and their versions, the machine class, and what counts as
a failure are written up at
<https://pdfluent.com/benchmarks/how-we-measure>. Every figure PDFluent
publishes carries a claim ID and points back to that method. That page
deliberately carries no figure of its own, and neither does this README.

## What is in here

The engine, the language bindings and the guards that gate them are in this one
repository.

**Rendering.** A pure-Rust rasterizer (vello_cpu) to PNG, JPEG or a rasterized
PDF, with SSIM comparison against a reference renderer.

**Text and data.** Extraction with position information, find-and-replace in
content streams, OCR over scanned pages.

**Forms.** AcroForm fill, flatten and read-back across every binding. XFA is
experimental and feature-gated behind `xfa-flatten`: dynamic layout, a FormCalc
interpreter with 90+ built-ins, and SOM path resolution
(`xfa.form.subform[3].field[*]`). It is not production-supported.

**Documents.** Merge, split, insert, delete and rearrange pages; AES-256 (PDF
2.0) encryption; watermarks; redaction that removes the content rather than
covering it.

**PDF/A.** Validation and conversion for A-1, A-2 and A-3, including ZUGFeRD and
Factur-X invoices, with font embedding and subsetting, OutputIntent injection
and XMP metadata repair.

**Signatures.** PAdES signing and verification.

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

---

[![Star pdfluent/pdfluent-sdk on GitHub](https://star-prompt-worker.lnmput.workers.dev/badge?repo=pdfluent%2Fpdfluent-sdk)](https://github.com/pdfluent/pdfluent-sdk)

**External assets in this README.** The badges come from `img.shields.io` and
the star reminder above from `star-prompt-worker.lnmput.workers.dev`, the free
tier of starme.dev. Both are third-party hosts serving an image into this page;
neither is code, neither runs anything in your clone, and both can be removed
without touching the build. They are listed here so nobody has to wonder later
where an image in our README comes from.

## Licence

PDFluent is available under two licences, at your option:

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
[pdfluent.com/sdk/pricing](https://pdfluent.com/sdk/pricing) sells; enquiries go
to <sales@pdfluent.com>.

**What the build currently does, stated plainly, because the licence file
previously said otherwise.** As of 1 September 2026 the compiled SDK still
contains tier checks left over from the proprietary model: with no licence key
it runs as `Tier::Trial`, and some capabilities — rendering, redaction,
conversion — return `FeatureNotInTier`. Text edits add an "Edited with PDFluent
trial" annotation.

Those checks do not limit the rights the AGPL grants. You have the source and
the freedom to modify it, so you may remove them, and you need no permission
from us to do so. But a licence that says "no enforcement of any kind" while the
binary refuses features is a false statement, and it was one. Removal of the
enforcement is tracked as issue #199; until that lands, this paragraph is the
accurate description and the sentence it replaced was not.

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
