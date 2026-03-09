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
import com.xfa.pdf.PdfDocument;
import com.xfa.pdf.RenderedImage;

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

The SDK loads the native library in this order:
1. `System.loadLibrary("pdf_java")` via `java.library.path`
2. `PDF_NATIVE_LIB` environment variable (full path)
3. Classpath extraction (bundled in JAR at `/native/<arch>/`)

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
