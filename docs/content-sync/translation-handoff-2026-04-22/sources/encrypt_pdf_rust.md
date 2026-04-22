# encrypt-pdf-rust — content sync

**URL:** <https://pdfluent.com/how-to/encrypt-pdf-rust>
**Status:** WEBSITE_PROMISES_UNSUPPORTED_BEHAVIOR
**SDK pin:** master `e891ffb0d`

## Current published snippet

```rust
use pdfluent::{PdfDocument, EncryptOptions, PdfPermissions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = PdfDocument::open("report.pdf")?;

    let opts = EncryptOptions::aes256()
        .user_password("open123")
        .owner_password("owner_secret")
        .permissions(PdfPermissions::all_except_edit());

    doc.encrypt(&opts)?;
    doc.save("report_protected.pdf")?;
    Ok(())
}
```

## Problems

1. **`PdfPermissions` doesn't exist.** SDK exports `Permissions`.
2. **Builder chain is imaginary.** `EncryptOptions` doesn't have `.user_password()`, `.owner_password()`, or `.permissions()` methods with the shown shape.
3. **`doc.encrypt(&opts)` passes by reference.** SDK takes `opts: EncryptOptions` by move.
4. **`all_except_edit()` isn't a `Permissions` preset.** Available presets per RFC v1.3: `full_access()`, `print_only()`, `fill_forms_only()`, plus per-field `with_*()` builders.

## Canonical SDK-truth snippet

```rust
use pdfluent::prelude::*;

fn main() -> Result<()> {
    let mut doc = PdfDocument::open("report.pdf")?;

    // AES-256 with full-access permissions (the 1.0 default preset)
    // minus the "modify content" right:
    let permissions = Permissions::full_access()
        .with_modify_contents(false);

    doc.encrypt(
        EncryptOptions::aes256()
            .with_user_password("open123")
            .with_owner_password("owner_secret")
            .with_permissions(permissions),
    )?;

    doc.save("report_protected.pdf")?;
    Ok(())
}
```

### Notes on the canonical shape

- `use pdfluent::prelude::*;` brings `PdfDocument`, `EncryptOptions`,
  `Permissions`, and `Result` into scope in one line.
- `Permissions::full_access()` is the 1.0 default for `aes256()` /
  `aes128()`. Users opt out of individual rights via `with_*(false)`.
- `with_user_password` / `with_owner_password` are the `EncryptOptions`
  builder methods; the password arguments move into the options
  (not a reference).
- `doc.encrypt(...)` takes `EncryptOptions` by value.
- The snippet uses `pdfluent::Result` (aliased from
  `Result<T, pdfluent::Error>`) so the error path is unified.

## Related prose updates

- Remove any paragraph claiming the SDK ships an
  `all_except_edit()` preset. If the article explains preset
  categories, replace with: "Three presets ship today:
  `Permissions::full_access()`, `Permissions::print_only()`, and
  `Permissions::fill_forms_only()`. For custom combinations,
  start from `full_access()` and opt out via `with_*()` builders."
- If the article lists supported algorithms: state **AES-128 is
  accepted today but the 1.0 backend currently emits AES-256
  regardless** — registered in STABILITY.md §3.3 as a 1.1
  follow-up (#1251 Codex P2 audit item).

## What stays the same

- SEO title and meta description.
- Intro paragraphs about why to encrypt.
- Closing "see also" links.
