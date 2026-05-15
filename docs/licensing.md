# License Activation

PDFluent runs in **Trial** mode by default. Trial unlocks the minimum surface
needed to evaluate the SDK and marks output via the `/Producer` metadata
field. To remove the watermark and unlock the full capability set, activate
a license key.

Each binding exposes the same three operations:

1. **Activate from a key string** — primary path
2. **Activate from a file** — convenience: read a UTF-8 text file and call activate
3. **Read the current license status** — tier, source, and whether output is marked

## Key format (1.0)

The 1.0 release accepts a **simple evaluation format**:

```
tier:trial
tier:developer
tier:team
tier:business
tier:enterprise
```

Cryptographically-signed payloads (Ed25519) ship in 1.1 and will be accepted
by the same `activate_*` functions without breaking the existing API.

## Environment variable

All bindings honour the `PDFLUENT_LICENSE_KEY` environment variable. If set
to a parseable key when the SDK initialises, the resolved tier becomes the
default.

```bash
export PDFLUENT_LICENSE_KEY=tier:developer
```

In a browser (WASM in a real browser tab) the env-var path is not available;
use `activateLicenseKey` instead.

## Process-global, set-once

The Rust core uses a process-global write-once mechanism. Re-activating with
the same tier is idempotent. Re-activating with a different tier returns an
"already set" error — **restart the process to switch tiers**.

## Per-language usage

### Rust

```rust
use pdfluent::{set_license_key, license_info};

set_license_key("tier:enterprise")?;
let info = license_info();
println!("{:?}", info.tier);  // Enterprise
```

### C ABI

```c
#include "pdf_capi.h"

PdfStatus s = pdfluent_license_activate_key("tier:enterprise");
if (s != PDF_STATUS_OK) {
    fprintf(stderr, "%s\n", pdf_get_last_error());
}

PdfluentLicenseStatus status;
pdfluent_license_status(&status);
printf("tier=%d source=%d marked=%d\n",
       status.tier, status.source, status.output_is_marked);
```

### Python

```python
import pdfluent

pdfluent.activate_license_key("tier:enterprise")
status = pdfluent.license_status()
print(status.tier)    # "Enterprise"
print(status.source)  # "Explicit"
```

### WASM (JavaScript / TypeScript)

```javascript
import init, { activateLicenseKey, licenseStatus } from '@pdfluent/xfa-wasm';

await init();
activateLicenseKey('tier:enterprise');
const s = licenseStatus();
console.log(s.tier);            // "Enterprise"
console.log(s.outputIsMarked);  // false
```

### .NET

```csharp
using XfaPdf;

Licensing.ActivateKey("tier:enterprise");
LicenseStatus s = Licensing.Status;
Console.WriteLine(s.Tier);  // Enterprise
```

### Java

```java
import com.xfa.pdf.PdfluentLicensing;

PdfluentLicensing.activateKey("tier:enterprise");
PdfluentLicensing.LicenseStatus s = PdfluentLicensing.status();
System.out.println(s.tier);  // ENTERPRISE
```

## Status object shape

All bindings expose the same three fields:

| Field | Meaning |
|-------|---------|
| `tier` | `Trial` / `Developer` / `Team` / `Business` / `Enterprise` |
| `source` | `Default` (no key) / `EnvVar` / `Explicit` |
| `output_is_marked` | `true` only in Trial — output carries the trial `/Producer` mark |

## Error model

| Failure | Rust | C ABI | Python | WASM | .NET | Java |
|---------|------|-------|--------|------|------|------|
| Invalid key | `Error::InvalidLicense` | `ErrorInvalidLicense=16` | `ValueError` | `Error` | `PdfException` | `PdfException` |
| Already set | `Error::InvalidLicense` | `ErrorLicenseAlreadySet=17` | `RuntimeError` | `Error` | `InvalidOperationException` | `IllegalStateException` |
| File read failed | — (manual) | `ErrorLicenseFile=18` | `OSError` | not exposed | `FileNotFoundException` | `IOException` |

The C ABI thread-local last-error string (via `pdf_get_last_error()`)
carries a human-readable message. Bindings translate this into idiomatic
exceptions.

## Security notes

- The SDK does not log license keys.
- Error messages report parse failure modes, never the raw key.
- Tests use only fake-format keys (`tier:developer`, etc.). No real signed
  key should be committed to source control.
