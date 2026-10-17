# Compatible API — binding layers (L5)

Where the L5 lane generates the Java and .NET layers of the compatible API
(#643), and the naming rules each binding follows. L5 writes no new native
behaviour: the compatible surface is implemented once, in the `pdfluent`
facade (`crates/pdfluent/src/compat/`) and the Python/Node bindings; this lane
only projects it into Java and .NET.

## Source: `specs/api`

L5 generates from the API specs under `specs/api` (written by SPEC-role work;
not present in the tree yet — L5 starts once the specs it needs have landed).
The specs name every compatible type, method, and error mapping in our own
words; the generated layers must not introduce names the specs do not define.

`specs/api` describes the contract; the native code it calls into differs per
binding (below).

## Native footing per binding

| Binding | Calls into | How |
|---|---|---|
| Java (`bindings/java`, `com.pdfluent`) | `crates/pdf-capi` (`pdf-capi` cdylib) through JNA | `NativeLoader` loads the shared library; new C functions get JNA declarations in `PdfCapiLibrary.java` beside the existing `pdf_*` mappings |
| .NET (`bindings/dotnet`, `PDFluent`) | `crates/pdf-capi` (`pdf-capi` cdylib) | P/Invoke `DllImport`s in `NativeMethods.cs`, one per new C function |

Both bindings sit on the C ABI crate (`crates/pdf-capi`). If a compatible
operation has no C function yet, L5 adds the `pdf_*` function to `pdf-capi`
first — following `docs/c_abi_stability.md` — then binds it. The C function
names and the Java/.NET projections ship in the same L5 change so the parity
checker never sees one without the other.

## File layout

Java (`bindings/java/src/main/java/com/pdfluent/`, Maven module
`com.pdfluent:pdfluent`):

- One class per compatible type, beside the existing classes
  (`PdfDocument.java`, `Block.java`, …): e.g. the geometry value types land as
  new files in the same package, not as inner classes.
- New native declarations go in `PdfCapiLibrary.java`; error mapping extends
  the existing `PdfluentException` hierarchy in place.
- Tests beside the existing ones under `src/test/`.

.NET (`bindings/dotnet/src/PDFluent/`, P/Invoke over the same cdylib):

- One file per compatible type beside the existing files
  (`PdfDocument.cs`, `TextBlock.cs`, …), same namespace.
- New `DllImport`s go in `NativeMethods.cs`; new status codes in
  `PdfStatus.cs`; error mapping extends `PdfluentException.cs` in place.
- Tests beside the existing ones in `test/PDFluent.Tests/`.

## Naming rules

The base conventions are `docs/en/binding-api-conventions.md`; the compatible
layers follow them with these per-language shapes:

- **Node** (for reference; owned by the Node lanes, not L5): `camelCase`
  methods on the existing classes (`openPdf`, `extractText`, …), matching the
  current `@pdfluent/node` surface.
- **Java**: `camelCase` methods, `getX` for property-style getters
  (`getPageCount()`, `getText()`), matching the existing `PdfDocument.java`
  (`getFormFields`, `setFormField`).
- **.NET**: `PascalCase` throughout, `GetX` for getters (`GetPageCount()`,
  `GetText()`), matching the existing `PdfDocument.cs` surface.
- **C ABI** (underneath both): `pdf_*` free functions returning `PdfStatus`,
  opaque handles, caller-frees-own convention per `docs/c_abi_stability.md`.

New names mirror the compatible spelling only through the `specs/api`
mapping — a compatible method that reads oddly in `getX`/`GetX` shape is still
spelled that way; the spec, not taste, settles it.
