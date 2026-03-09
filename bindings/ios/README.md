# XFA PDF SDK for iOS/macOS

Swift wrapper over the C API for native iOS and macOS apps.

## Requirements

- Xcode 14+, Swift 5.7+
- iOS 14+ / macOS 12+
- Rust toolchain with iOS targets

## Building the XCFramework

```bash
# Install iOS targets
rustup target add aarch64-apple-ios x86_64-apple-ios aarch64-apple-darwin x86_64-apple-darwin

# Build the XCFramework
bash bindings/ios/scripts/build-xcframework.sh
```

## Swift Package Manager

Add the package to your `Package.swift` or Xcode project:

```swift
.package(path: "path/to/bindings/ios")
```

## Quick Start

```swift
import XfaPdf

// Open from data
let data = try Data(contentsOf: url)
let doc = try PdfDocument(data: data)

print("Pages: \(doc.pageCount)")

// Page dimensions
let width = doc.pageWidth(0)
let height = doc.pageHeight(0)

// Extract text
let text = try doc.extractText(page: 0)

// Render page at 2x scale (for Retina)
let rendered = try doc.renderPage(0, dpi: 144.0)
let uiImage = rendered.toUIImage()  // iOS
let nsImage = rendered.toNSImage()  // macOS

// Render thumbnail
let thumb = try doc.renderThumbnail(0, maxDimension: 200)

// Metadata
let title = doc.metadata(forKey: "Title")
let author = doc.metadata(forKey: "Author")

// Page boxes
let mediaBox = try doc.mediaBox(0)
print("Size: \(mediaBox.width) x \(mediaBox.height)")
```

## Password-Protected PDFs

```swift
let doc = try PdfDocument(path: "/path/to/encrypted.pdf", password: "secret")
```

## Resource Management

The document is automatically closed when deallocated. You can also close explicitly:

```swift
let doc = try PdfDocument(data: pdfData)
// ... use doc ...
doc.close()
```

## Error Handling

```swift
do {
    let doc = try PdfDocument(data: invalidData)
} catch let error as PdfError {
    switch error {
    case .corruptPdf(let msg):
        print("Corrupt PDF: \(msg)")
    case .invalidPassword(let msg):
        print("Wrong password: \(msg)")
    default:
        print("Error: \(error.localizedDescription)")
    }
}
```

## API Reference

| Member | Description |
|--------|-------------|
| `PdfDocument(data:)` | Open from Data |
| `PdfDocument(url:password:)` | Open from file URL |
| `PdfDocument(path:password:)` | Open from path |
| `pageCount` | Number of pages |
| `bookmarkCount` | Number of bookmarks |
| `pageWidth(_:)` | Page width in points |
| `pageHeight(_:)` | Page height in points |
| `pageRotation(_:)` | Page rotation degrees |
| `mediaBox(_:)` | Page media box |
| `cropBox(_:)` | Page crop box |
| `extractText(page:)` | Extract text from page |
| `renderPage(_:dpi:)` | Render at DPI |
| `renderThumbnail(_:maxDimension:)` | Render thumbnail |
| `metadata(forKey:)` | Get metadata value |
| `close()` | Free native resources |
| `isOpen` | Whether document is open |
