# PDFluent for Java

[Source on GitHub](https://github.com/pdfluent/pdfluent-sdk) · [Documentation](https://pdfluent.com/docs) · [Report an issue](https://github.com/pdfluent/pdfluent-sdk/issues)

Java bindings for the PDFluent PDF engine via JNI.

## Requirements

- Java 11+
- The native library (`libpdf_java.dylib` / `libpdf_java.so` / `pdf_java.dll`)

## Building the Native Library

```bash
cargo build -p pdf-java --release
```

The native library will be at:
- macOS: `target/release/libpdf_java.dylib`
- Linux: `target/release/libpdf_java.so`
- Windows: `target/release/pdf_java.dll`

## Building the Java SDK

```bash
mvn package -f bindings/java/pom.xml
```

## Quick Start

```java
import com.pdfluent.PdfDocument;
import com.pdfluent.RenderedImage;

// Open a PDF
try (PdfDocument doc = PdfDocument.open(Path.of("input.pdf"))) {
    System.out.println("Pages: " + doc.getPageCount());

    // Get page dimensions
    double width = doc.getPageWidth(0);
    double height = doc.getPageHeight(0);

    // Extract text
    String text = doc.extractText(0);

    // Render page at 150 DPI
    RenderedImage img = doc.renderPage(0, 150.0);
    BufferedImage buffered = img.toBufferedImage();

    // Render thumbnail
    RenderedImage thumb = doc.renderThumbnail(0, 200);

    // Read metadata
    String title = doc.getMetadata("Title");
    String author = doc.getMetadata("Author");

    // Search text
    int[] pages = doc.searchText("invoice");
}
```

## Password-Protected PDFs

```java
byte[] data = Files.readAllBytes(Path.of("encrypted.pdf"));
try (PdfDocument doc = PdfDocument.openWithPassword(data, "secret")) {
    System.out.println("Pages: " + doc.getPageCount());
}
```

## Native Library Loading

The SDK loads its native libraries (`libpdfluent_java` for the JNI
`PdfluentDocument` path and `libpdf_capi` for the JNA-based
`PdfDocument` path) in this order:
1. `java.library.path` (e.g. `-Djava.library.path=/path/to/dir`)
2. `PDFLUENT_NATIVE_LIB` env var (JNI lib) or `PDFLUENT_CAPI_LIB` env var (C ABI lib)
3. Classpath extraction (bundled in JAR at `/native/<arch>/`)

A failure to locate either library throws
`com.pdfluent.PdfluentNativeLoadException` (code
`E-ENV-MISSING-DEPENDENCY`) with every search location that was tried.

## API Reference

| Method | Description |
|--------|-------------|
| `PdfDocument.open(Path)` | Open from file path |
| `PdfDocument.open(byte[])` | Open from bytes |
| `PdfDocument.openWithPassword(byte[], String)` | Open encrypted PDF |
| `getPageCount()` | Number of pages |
| `getPageWidth(int)` | Page width in points |
| `getPageHeight(int)` | Page height in points |
| `getPageRotation(int)` | Page rotation (0/90/180/270) |
| `extractText(int)` | Extract text from page |
| `renderPage(int, double)` | Render page at DPI |
| `renderThumbnail(int, int)` | Render constrained thumbnail |
| `getMetadata(String)` | Get metadata value |
| `getBookmarkCount()` | Number of bookmarks |
| `searchText(String)` | Search across all pages |
| `save(Path)` | Write the document (incl. form mutations) to a file |
| `getFormFields()` | All AcroForm fields → `List<FormField>` |
| `setFormField(String, String)` | Fill a field (text, checkbox, radio, or choice); keeps `/V`, `/AS`, `/AP` consistent |
| `setMultiSelect(String, String[])` | Select multiple options on a multi-select list box (writes `/V` array + sorted `/I`) |
| `close()` | Free native resources |

### Forms

```java
try (PdfluentDocument doc = PdfluentDocument.open(Files.readAllBytes(path))) {
    for (FormField f : doc.getFormFields()) {
        System.out.println(f.name + " [" + f.fieldType + "] = " + f.value);
    }
    doc.setFormField("Address.Street", "Damrak 1");      // text
    doc.setFormField("Agree", "Yes");                     // checkbox on-state
    doc.setMultiSelect("Languages", new String[] {"EN", "NL"}); // multi-select
    doc.save(Path.of("filled.pdf"));
}
```

`setFormField` detects the field type from `/FT` and applies the value as
text, a radio export name, a choice option, or a bool-ish checkbox state.
Hierarchical names (`"parent.child"`) are resolved through `/Kids`. Read-only
fields throw `PdfluentException`.

## Licence

AGPL-3.0 or a commercial licence, at your option. The AGPL is the default and
the complete product: there is no licence key, no activation call and no tier,
and every feature works in every build. If you cannot accept the copyleft
obligation, the commercial licence is sold yearly and self-service at
[pdfluent.com/sdk/pricing](https://pdfluent.com/sdk/pricing), in four options: Commercial (per
organisation), OEM Startup and OEM (per product), and Priority support (an
add-on). The two texts are `LICENSE` and `LICENSE-COMMERCIAL` in this package.

## Links

- **Documentation:** <https://pdfluent.com/docs>
- **Pricing:** <https://pdfluent.com/sdk/pricing>
- **PDFluent editor** (source-available): <https://github.com/pdfluent/pdfluent>. The free desktop app this SDK powers.
- Built by [Innovation Trigger BV](https://pdfluent.com)
