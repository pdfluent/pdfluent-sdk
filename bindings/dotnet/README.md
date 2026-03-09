# XFA PDF SDK for .NET

C# bindings for the XFA PDF engine via P/Invoke over the C API.

## Requirements

- .NET 6.0+ (or .NET Framework 4.7+ for netstandard2.0)
- The native library (`libpdf_capi.dylib` / `libpdf_capi.so` / `pdf_capi.dll`)

## Building the Native Library

```bash
cargo build -p pdf-capi --release
```

## Quick Start

```csharp
using XfaPdf;

// Open a PDF
using var doc = PdfDocument.Open("input.pdf");
Console.WriteLine($"Pages: {doc.PageCount}");

// Get page dimensions
double width = doc.GetPageWidth(0);
double height = doc.GetPageHeight(0);

// Extract text
string text = doc.ExtractText(0);

// Render page at 150 DPI
RenderedImage img = doc.RenderPage(0, 150.0);
Console.WriteLine($"Image: {img.Width}x{img.Height}");

// Render thumbnail
RenderedImage thumb = doc.RenderThumbnail(0, 200);

// Read metadata
string? title = doc.GetMetadata("Title");

// Get page boxes
PageBox mediaBox = doc.GetMediaBox(0);
Console.WriteLine($"MediaBox: {mediaBox.Width}x{mediaBox.Height}");
```

## Async Support

```csharp
// Open asynchronously
using var doc = await PdfDocument.OpenAsync("large.pdf");

// Extract text asynchronously
string text = await doc.ExtractTextAsync(0);

// Render asynchronously
RenderedImage img = await doc.RenderPageAsync(0, 150.0);
```

## Password-Protected PDFs

```csharp
using var doc = PdfDocument.Open("encrypted.pdf", password: "secret");
```

## Open from Bytes

```csharp
byte[] data = File.ReadAllBytes("input.pdf");
using var doc = PdfDocument.Open(data);
```

## Error Handling

```csharp
try
{
    using var doc = PdfDocument.Open(invalidBytes);
}
catch (PdfException ex)
{
    Console.WriteLine($"Status: {ex.Status}");
    Console.WriteLine($"Message: {ex.Message}");
}
```

## NuGet Package

The project generates a NuGet package with native library bundling per platform:
- `runtimes/win-x64/native/pdf_capi.dll`
- `runtimes/linux-x64/native/libpdf_capi.so`
- `runtimes/osx-arm64/native/libpdf_capi.dylib`

## API Reference

| Member | Description |
|--------|-------------|
| `PdfDocument.Open(string, string?)` | Open from file path |
| `PdfDocument.Open(byte[])` | Open from bytes |
| `PdfDocument.OpenAsync(string, string?)` | Open asynchronously |
| `PageCount` | Number of pages |
| `BookmarkCount` | Number of bookmarks |
| `GetPageWidth(int)` | Page width in points |
| `GetPageHeight(int)` | Page height in points |
| `GetPageRotation(int)` | Page rotation degrees |
| `GetMediaBox(int)` | Page media box |
| `GetCropBox(int)` | Page crop box |
| `ExtractText(int)` | Extract text from page |
| `ExtractTextAsync(int)` | Extract text (async) |
| `RenderPage(int, double)` | Render at DPI |
| `RenderPageAsync(int, double)` | Render (async) |
| `RenderThumbnail(int, int)` | Render thumbnail |
| `GetMetadata(string)` | Get metadata value |
| `Dispose()` | Free native resources |
