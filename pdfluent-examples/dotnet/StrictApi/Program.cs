// PDFluent .NET — StrictApi example
// Demonstrates the four G-track operations: Extract · Render · Validate · Merge.
// Text-editing operations (Replace, Format, Style) are gated on
// #if PDFLUENT_TEXT_EDITING until those APIs reach GA.

using System;
using System.IO;
using XfaPdf;

// ─── helpers ─────────────────────────────────────────────────────────────────

static byte[] MinimalPdf()
{
    // A minimal, structurally valid single-page PDF for offline demo purposes.
    const string src =
        "%PDF-1.4\n" +
        "1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n" +
        "2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n" +
        "3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]>>endobj\n" +
        "xref\n0 4\n" +
        "0000000000 65535 f \n" +
        "0000000009 00000 n \n" +
        "0000000058 00000 n \n" +
        "0000000115 00000 n \n" +
        "trailer<</Size 4/Root 1 0 R>>\n" +
        "startxref\n191\n%%EOF";
    return System.Text.Encoding.ASCII.GetBytes(src);
}

// ─── main ────────────────────────────────────────────────────────────────────

string inputPath = args.Length > 0 ? args[0] : string.Empty;
bool useFile = inputPath.Length > 0 && File.Exists(inputPath);

Console.WriteLine("PDFluent .NET SDK — StrictApi demo");
Console.WriteLine(new string('─', 50));

try
{
    // ── G1: Open ─────────────────────────────────────────────────────────────
    using PdfDocument doc = useFile
        ? PdfDocument.Open(inputPath)
        : PdfDocument.Open(MinimalPdf());

    string source = useFile ? $"file '{inputPath}'" : "in-memory minimal PDF";
    Console.WriteLine($"Opened {source}");
    Console.WriteLine($"  Pages   : {doc.PageCount}");
    Console.WriteLine($"  Disposed: {doc.IsDisposed}");

    // ── G2: Extract text ──────────────────────────────────────────────────────
    string text = doc.ExtractText(0);
    Console.WriteLine($"\nExtract (page 0): {text.Length} chars");
    if (text.Length > 0)
        Console.WriteLine($"  Preview: {text[..Math.Min(80, text.Length)]}…");

    // ── G3: Render page ───────────────────────────────────────────────────────
    RenderedImage img = doc.RenderPage(0, dpi: 72.0);
    Console.WriteLine($"\nRender (72 dpi): {img.Width}×{img.Height} px, " +
                      $"{img.Pixels.Length} bytes RGBA");

    // ── G4: Page geometry ─────────────────────────────────────────────────────
    PageBox mediaBox = doc.GetMediaBox(0);
    Console.WriteLine($"\nMediaBox: [{mediaBox.X0:F1}, {mediaBox.Y0:F1}, " +
                      $"{mediaBox.X1:F1}, {mediaBox.Y1:F1}]  " +
                      $"({mediaBox.Width:F1} × {mediaBox.Height:F1} pt)");

    // ── G5: Metadata ──────────────────────────────────────────────────────────
    string? title    = doc.GetMetadata("Title");
    string? author   = doc.GetMetadata("Author");
    string? producer = doc.GetMetadata("Producer");
    Console.WriteLine($"\nMetadata:");
    Console.WriteLine($"  Title   : {title    ?? "(none)"}");
    Console.WriteLine($"  Author  : {author   ?? "(none)"}");
    Console.WriteLine($"  Producer: {producer ?? "(none)"}");

#if PDFLUENT_TEXT_EDITING
    // ── G-track: Text-editing operations (not yet available in beta) ──────────
    // Replace:   doc.ReplaceText(pageIndex: 0, search: "old", replacement: "new");
    // Format:    doc.SetTextStyle(pageIndex: 0, range: (0, 10), bold: true);
    // Style:     doc.SetPageBackground(pageIndex: 0, color: "#FFFFFF");
    Console.WriteLine("\n[PDFLUENT_TEXT_EDITING] Text-editing operations available.");
#else
    Console.WriteLine("\n[PDFLUENT_TEXT_EDITING] Text-editing APIs not yet GA. " +
                      "Define PDFLUENT_TEXT_EDITING when available.");
#endif

    Console.WriteLine("\nAll operations completed successfully.");
}
catch (PdfluentIoException ex)
{
    Console.Error.WriteLine($"IO error [{ex.NativeStatus}]: {ex.Message}");
    Environment.Exit(2);
}
catch (PdfluentParseException ex)
{
    Console.Error.WriteLine($"Parse error [{ex.NativeStatus}]: {ex.Message}");
    Environment.Exit(3);
}
catch (PdfluentPermissionException ex)
{
    Console.Error.WriteLine($"Permission error [{ex.NativeStatus}]: {ex.Message}");
    Console.Error.WriteLine("Tip: supply a password via PdfDocument.Open(path, password).");
    Environment.Exit(4);
}
catch (PdfluentPageRangeException ex)
{
    Console.Error.WriteLine($"Page range error [{ex.NativeStatus}]: {ex.Message}");
    Environment.Exit(5);
}
catch (PdfluentRenderException ex)
{
    Console.Error.WriteLine($"Render error [{ex.NativeStatus}]: {ex.Message}");
    Environment.Exit(6);
}
catch (PdfluentException ex)
{
    // Catch-all for any other PDFluent error.
    Console.Error.WriteLine($"PDFluent error [{ex.NativeStatus}]: {ex.Message}");
    Environment.Exit(1);
}
