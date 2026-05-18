# XFA PDF SDK for Java

Java bindings for the XFA PDF engine via JNI.

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
`PdfDocument` + `PdfluentLicensing` path) in this order:
1. `java.library.path` (e.g. `-Djava.library.path=/path/to/dir`)
2. `PDFLUENT_NATIVE_LIB` env var (JNI lib) or `PDFLUENT_CAPI_LIB` env var (C ABI lib)
3. Classpath extraction (bundled in JAR at `/native/<arch>/`)

A failure to locate either library throws
`com.pdfluent.PdfluentNativeLoadException` (code
`E-ENV-MISSING-DEPENDENCY`) with every search location that was tried.

## License Activation

The SDK runs in Trial mode by default; output is marked via `/Producer`
metadata. Activate a license to unlock the paid-tier capability set.

```java
import com.pdfluent.PdfluentLicensing;

// Activate from a key string
PdfluentLicensing.activateKey("tier:enterprise");

// Or read the key from a UTF-8 text file (may throw IOException)
PdfluentLicensing.activateFile("/path/to/key.lic");

// Inspect the current status (always succeeds; defaults to Trial)
PdfluentLicensing.LicenseStatus s = PdfluentLicensing.status();
System.out.println(s.tier);            // Tier.ENTERPRISE
System.out.println(s.source);          // Source.EXPLICIT / ENV_VAR / DEFAULT
System.out.println(s.outputIsMarked);  // false

PdfluentLicensing.Tier t = PdfluentLicensing.effectiveTier();  // shortcut
```

The `PDFLUENT_LICENSE_KEY` environment variable is honoured automatically.

**Behavior to be aware of:**

- The active tier is **process-global and set-once**. Re-activating with
  the same key is a no-op. Re-activating with a different tier throws
  `IllegalStateException`; restart the JVM to switch tiers.
- Invalid keys throw `com.pdfluent.PdfluentLicenseException`. The exception
  carries the canonical C8 error code via `getCode()` — one of
  `E-LICENSE-INVALID`, `E-LICENSE-FEATURE-NOT-IN-TIER`, or
  `E-LICENSE-CAPABILITY-NOT-COMPILED`. Branch on the code instead of parsing
  the message string:

  ```java
  try {
      PdfluentLicensing.activateKey(badKey);
  } catch (PdfluentLicenseException e) {
      if ("E-LICENSE-INVALID".equals(e.getCode())) {
          // tell the user the key is malformed
      }
  }
  ```

- Missing license files throw `java.io.IOException`.
- The key string is never logged or stored beyond the call.

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
| `close()` | Free native resources |
