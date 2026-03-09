# XFA PDF SDK for Android

Kotlin wrapper via JNI for native Android apps.

## Requirements

- Android SDK (minSdk 21)
- Android NDK
- `cargo-ndk` (`cargo install cargo-ndk`)
- Rust toolchain with Android targets

## Building the Native Library

```bash
# Install Android targets
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android

# Build for all architectures
bash bindings/android/scripts/build-android.sh
```

Native `.so` files are output to `xfapdf/src/main/jniLibs/`.

## Gradle Integration

Add the module to your Android project:

```kotlin
// settings.gradle.kts
include(":xfapdf")
project(":xfapdf").projectDir = file("path/to/bindings/android/xfapdf")

// app/build.gradle.kts
dependencies {
    implementation(project(":xfapdf"))
}
```

## Quick Start

```kotlin
import com.xfa.pdf.PdfDocument

// Open from bytes
val bytes = assets.open("sample.pdf").readBytes()
PdfDocument.open(bytes).use { doc ->
    println("Pages: ${doc.pageCount}")

    // Page dimensions
    val width = doc.getPageWidth(0)
    val height = doc.getPageHeight(0)

    // Extract text
    val text = doc.extractText(0)

    // Render to Bitmap
    val image = doc.renderPage(0, dpi = 150.0)
    val bitmap = image.toBitmap()
    imageView.setImageBitmap(bitmap)

    // Thumbnail for RecyclerView
    val thumb = doc.renderThumbnail(0, maxDimension = 200)
    val thumbBitmap = thumb.toBitmap()

    // Metadata
    val title = doc.getMetadata("Title")

    // Search
    val pages = doc.searchText("invoice")
}
```

## Open from File

```kotlin
val file = File(context.filesDir, "document.pdf")
PdfDocument.open(file).use { doc ->
    // ...
}
```

## Password-Protected PDFs

```kotlin
val data = file.readBytes()
PdfDocument.openWithPassword(data, "secret").use { doc ->
    // ...
}
```

## ProGuard

ProGuard rules are included in `consumer-rules.pro` and applied automatically when using the AAR.

## API Reference

| Member | Description |
|--------|-------------|
| `PdfDocument.open(ByteArray)` | Open from bytes |
| `PdfDocument.open(File)` | Open from file |
| `PdfDocument.openWithPassword(ByteArray, String)` | Open encrypted PDF |
| `pageCount` | Number of pages |
| `bookmarkCount` | Number of bookmarks |
| `getPageWidth(Int)` | Page width in points |
| `getPageHeight(Int)` | Page height in points |
| `getPageRotation(Int)` | Page rotation degrees |
| `extractText(Int)` | Extract text from page |
| `renderPage(Int, Double)` | Render at DPI |
| `renderThumbnail(Int, Int)` | Render thumbnail |
| `getMetadata(String)` | Get metadata value |
| `searchText(String)` | Search across pages |
| `close()` | Free native resources |
| `isOpen` | Whether document is open |
