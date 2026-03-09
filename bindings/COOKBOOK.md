# XFA PDF SDK — Cookbook

Common use cases across all language bindings.

## 1. Open PDF and Render Pages

### Java
```java
try (PdfDocument doc = PdfDocument.open(Path.of("input.pdf"))) {
    for (int i = 0; i < doc.getPageCount(); i++) {
        RenderedImage img = doc.renderPage(i, 150.0);
        BufferedImage bi = img.toBufferedImage();
        ImageIO.write(bi, "png", new File("page_" + i + ".png"));
    }
}
```

### C# (.NET)
```csharp
using var doc = PdfDocument.Open("input.pdf");
for (int i = 0; i < doc.PageCount; i++) {
    var img = doc.RenderPage(i, 150.0);
    // Use img.Pixels (RGBA) with System.Drawing or SkiaSharp
}
```

### Swift (iOS/macOS)
```swift
let doc = try PdfDocument(data: pdfData)
for i in 0..<doc.pageCount {
    let img = try doc.renderPage(i, dpi: 150.0)
    let uiImage = img.toUIImage()
}
```

### Kotlin (Android)
```kotlin
PdfDocument.open(pdfBytes).use { doc ->
    repeat(doc.pageCount) { i ->
        val img = doc.renderPage(i, dpi = 150.0)
        val bitmap = img.toBitmap()
    }
}
```

### WASM (JavaScript)
```js
const doc = PdfDoc.open(new Uint8Array(pdfBuffer));
const raw = doc.renderPage(0, 1.5);
const view = new DataView(raw.buffer);
const w = view.getUint32(0, true);
const h = view.getUint32(4, true);
const pixels = raw.slice(8);
const imageData = new ImageData(new Uint8ClampedArray(pixels), w, h);
ctx.putImageData(imageData, 0, 0);
```

## 2. Extract Text

### Java
```java
try (PdfDocument doc = PdfDocument.open(data)) {
    StringBuilder sb = new StringBuilder();
    for (int i = 0; i < doc.getPageCount(); i++) {
        sb.append(doc.extractText(i)).append("\n");
    }
    String fullText = sb.toString();
}
```

### C# (.NET)
```csharp
using var doc = PdfDocument.Open(data);
var texts = Enumerable.Range(0, doc.PageCount)
    .Select(i => doc.ExtractText(i));
string fullText = string.Join("\n", texts);
```

## 3. Search Text

### Java
```java
try (PdfDocument doc = PdfDocument.open(data)) {
    int[] pages = doc.searchText("invoice");
    System.out.println("Found on pages: " + Arrays.toString(pages));
}
```

### Kotlin
```kotlin
PdfDocument.open(data).use { doc ->
    val pages = doc.searchText("invoice")
    println("Found on pages: ${pages.toList()}")
}
```

## 4. Generate Thumbnails

### Java
```java
try (PdfDocument doc = PdfDocument.open(data)) {
    RenderedImage thumb = doc.renderThumbnail(0, 200);
    // thumb.getWidth() <= 200, thumb.getHeight() <= 200
}
```

### Swift
```swift
let doc = try PdfDocument(data: pdfData)
let thumb = try doc.renderThumbnail(0, maxDimension: 200)
let image = thumb.toUIImage()
```

## 5. Read Metadata

### Java
```java
try (PdfDocument doc = PdfDocument.open(data)) {
    String title = doc.getMetadata("Title");
    String author = doc.getMetadata("Author");
    String subject = doc.getMetadata("Subject");
    String creator = doc.getMetadata("Creator");
    int bookmarks = doc.getBookmarkCount();
}
```

### C#
```csharp
using var doc = PdfDocument.Open(data);
var title = doc.GetMetadata("Title");    // null if not set
var author = doc.GetMetadata("Author");
Console.WriteLine($"Title: {title ?? "(none)"}");
Console.WriteLine($"Bookmarks: {doc.BookmarkCount}");
```

## 6. Page Geometry

### C#
```csharp
using var doc = PdfDocument.Open(data);
var mediaBox = doc.GetMediaBox(0);
Console.WriteLine($"MediaBox: ({mediaBox.X0}, {mediaBox.Y0}) - ({mediaBox.X1}, {mediaBox.Y1})");
Console.WriteLine($"Size: {mediaBox.Width} x {mediaBox.Height} points");
```

### Swift
```swift
let doc = try PdfDocument(data: pdfData)
let box = try doc.mediaBox(0)
print("Size: \(box.width) x \(box.height) points")
print("At 72 DPI: \(box.width / 72) x \(box.height / 72) inches")
```

## 7. WASM Annotations

```js
// Read existing annotations
const annots = JSON.parse(doc.getAnnotations(0));
for (const a of annots) {
    console.log(`${a.subtype} at (${a.rect?.x0}, ${a.rect?.y0})`);
}

// Add a highlight
const newPdf = PdfDoc.addHighlight(pdfBytes, 0,
    100, 700, 400, 720,  // rect
    1.0, 1.0, 0.0);      // yellow RGB

// Add a sticky note
const withNote = PdfDoc.addStickyNote(pdfBytes, 0,
    50, 750, "Review this section");
```

## 8. WASM Signature Verification

```js
const doc = PdfDoc.open(signedPdfBytes);
const sigs = JSON.parse(doc.verifySignatures());
for (const sig of sigs) {
    console.log(`Signer: ${sig.signer}`);
    console.log(`Integrity: ${sig.structural_integrity ? 'OK' : 'FAIL'}`);
}
```

## 9. Password-Protected PDFs

### Java
```java
PdfDocument doc = PdfDocument.openWithPassword(data, "mypassword");
```

### C#
```csharp
var doc = PdfDocument.Open("encrypted.pdf", password: "mypassword");
```

### Swift
```swift
let doc = try PdfDocument(path: "/path/to/file.pdf", password: "mypassword")
```

### Kotlin
```kotlin
val doc = PdfDocument.openWithPassword(data, "mypassword")
```

## 10. WASM PDF/A Validation

```js
const doc = PdfDoc.open(pdfBytes);
const report = JSON.parse(doc.validatePdfA("pdfa2b"));
console.log(`Compliant: ${report.compliant}`);
console.log(`Errors: ${report.errors}, Warnings: ${report.warnings}`);
for (const issue of report.issues) {
    console.log(`  [${issue.severity}] ${issue.rule}: ${issue.message}`);
}
```
