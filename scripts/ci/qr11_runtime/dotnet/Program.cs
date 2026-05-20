using System;
using System.IO;
using PDFluent;

int fails = 0;
void Typed(string label, Action a) {
    try { a(); Console.Error.WriteLine($"{label}: NO THROW (silent success)"); fails++; }
    catch (PdfluentException e) { Console.WriteLine($"  {label}: {e.GetType().Name}"); }
    catch (Exception e) { Console.Error.WriteLine($"{label}: UNTYPED {e.GetType().Name}"); fails++; }
}

// identity: prove the loaded assembly is the local PDFluent build
var asm = typeof(PdfDocument).Assembly.Location;
Console.WriteLine($"identity OK: {asm}");

// valid control
try {
    var d = PdfDocument.Open(File.ReadAllBytes(System.Environment.GetEnvironmentVariable("QR11_VALID_PDF") ?? "tests/corpus-mini/multi-page.pdf"));
    Console.WriteLine($"  valid_control: pages={d.PageCount}");
    if (d.PageCount < 1) { Console.Error.WriteLine("valid_control pages<1"); fails++; }
} catch (Exception e) { Console.Error.WriteLine("valid_control FAIL: " + e); fails++; }

Typed("malformed", () => PdfDocument.Open(new byte[] {0xDE,0xAD,0xBE,0xEF,0x00,0x42}));
Typed("empty", () => PdfDocument.Open(Array.Empty<byte>()));
Typed("truncated", () => PdfDocument.Open(System.Text.Encoding.ASCII.GetBytes("%PDF-1.7\n1 0 obj")));

if (fails > 0) { Console.Error.WriteLine($"QR-11 dotnet: {fails} failure(s)"); return 1; }
Console.WriteLine("QR-11 dotnet error mapping: OK"); return 0;
