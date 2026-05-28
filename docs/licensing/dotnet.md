# .NET License Activation

This document describes the license activation surface exposed by the PDFluent
.NET binding (`bindings/dotnet/src/PDFluent`). All public types are in the
`PDFluent` namespace.

The .NET binding is a thin P/Invoke wrapper over the C ABI documented in
[`cabi.md`](./cabi.md). The mapping is 1:1; this document describes only the
managed surface.

## Types

### `LicenseTier`

```csharp
public enum LicenseTier
{
    Trial      = 0,
    Developer  = 1,
    Team       = 2,
    Business   = 3,
    Enterprise = 4,
}
```

### `LicenseSource`

```csharp
public enum LicenseSource
{
    Default  = 0, // No key provided; running in Trial.
    EnvVar   = 1, // Resolved from PDFLUENT_LICENSE_KEY.
    Explicit = 2, // Set via Licensing.ActivateKey / ActivateFile.
}
```

### `LicenseStatus`

```csharp
public readonly struct LicenseStatus
{
    public LicenseTier   Tier           { get; }
    public LicenseSource Source         { get; }
    public bool          OutputIsMarked { get; }
    public bool          Active         { get; } // True iff Tier > Trial
}
```

Mirrors the C ABI `PdfluentLicenseStatus` struct. `Active` is a convenience
flag that matches the Python `LicenseStatus.active` attribute.

## API

### `Licensing.ActivateKey(string key)`

Activate the process-global license from a key string.

| Failure | Exception |
|---------|-----------|
| `key is null` | `ArgumentNullException` |
| Malformed / unknown tier | `PdfluentLicenseException` (`Code = "E-LICENSE-INVALID"`, `NativeStatus = ErrorInvalidLicense`) |
| Different tier already active | `PdfluentLicenseException` (`Code = "E-LICENSE-INVALID"`, `NativeStatus = ErrorLicenseAlreadySet`) |
| Bad UTF-8 / null arg from C ABI | `PdfluentValidationException` |

### `Licensing.ActivateFile(string path)`

Read a UTF-8 key file and activate.

| Failure | Exception |
|---------|-----------|
| `path is null` | `ArgumentNullException` |
| File missing / unreadable | `PdfluentIoException` (`NativeStatus = ErrorLicenseFile`) |
| Key invalid | `PdfluentLicenseException` (`Code = "E-LICENSE-INVALID"`) |

### `Licensing.GetStatus()` / `Licensing.Status`

Always succeeds. Returns the current `LicenseStatus`. Before any activation
the snapshot has `Tier = Trial`, `Source = Default`, `OutputIsMarked = true`.

### `Licensing.EffectiveTier`

Returns the active tier as an `int` (0–4). Mirrors the C ABI
`pdfluent_license_effective_tier()` function directly.

`Licensing.EffectiveTierEnum` returns the same value as `LicenseTier`.

## Canonical error code mapping

The C8 error catalogue (`docs/error_catalogue.md`) defines stable string
codes that are surfaced on every language binding. The .NET binding exposes
them via `PdfluentException.Code`:

| Rust `Error` variant       | C ABI `PdfStatus`           | .NET exception                   | C8 code                              |
|----------------------------|-----------------------------|----------------------------------|--------------------------------------|
| `InvalidLicense`           | `ErrorInvalidLicense` (16)  | `PdfluentLicenseException`       | `E-LICENSE-INVALID`                  |
| `InvalidLicense` ("already set") | `ErrorLicenseAlreadySet` (17) | `PdfluentLicenseException` | `E-LICENSE-INVALID`                  |
| (license file I/O)         | `ErrorLicenseFile` (18)     | `PdfluentIoException`            | (none — IO category)                 |
| `FeatureNotInTier`         | (currently surfaced as last-error message on the call that gates) | (raised by the gated call, not by activation) | `E-LICENSE-FEATURE-NOT-IN-TIER`      |
| `CapabilityNotCompiled`    | (build-feature gate, never returned by activation) | (build-time)                | `E-LICENSE-CAPABILITY-NOT-COMPILED`  |

> The C ABI surfaces `FeatureNotInTier` as the failure of the *gated* call
> (e.g. `pdf_document_sign`) rather than as an activation error, so the
> licensing API itself only emits the `E-LICENSE-INVALID` code today.

## Example

```csharp
using PDFluent;

try
{
    Licensing.ActivateKey(Environment.GetEnvironmentVariable("PDFLUENT_LICENSE_KEY")
                          ?? "tier:developer");
}
catch (PdfluentLicenseException ex) when (ex.NativeStatus == PdfStatus.ErrorLicenseAlreadySet)
{
    // Another component already activated — fine.
}

LicenseStatus status = Licensing.GetStatus();
Console.WriteLine($"tier={status.Tier} active={status.Active}");
```

See `pdfluent-examples/dotnet/StrictApi/Program.cs` for a runnable example.
