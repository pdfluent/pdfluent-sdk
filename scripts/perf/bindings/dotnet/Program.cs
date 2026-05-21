using System;
using System.IO;
using System.Diagnostics;
using PDFluent;

int iters = int.Parse(Environment.GetEnvironmentVariable("JD_ITERS") ?? "400");
string valid = Environment.GetEnvironmentVariable("JD_VALID") ?? "tests/corpus-mini/multi-page.pdf";
string outp = Environment.GetEnvironmentVariable("JD_OUT") ?? "/tmp/jd_dotnet_perf.json";
byte[] b = File.ReadAllBytes(valid);
byte[] bad = { 0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x42 };
double nsPerTick = 1e9 / Stopwatch.Frequency;

for (int i = 0; i < 30; i++) { try { using var d = PdfDocument.Open(b); _ = d.PageCount; } catch { } }

long[] Loop(byte[] buf, out int oks, out int errs, out int pages)
{
    var t = new long[iters]; oks = 0; errs = 0; pages = -1;
    var sw = new Stopwatch();
    for (int i = 0; i < iters; i++)
    {
        sw.Restart();
        try { using var d = PdfDocument.Open(buf); pages = d.PageCount; oks++; }
        catch { errs++; }
        t[i] = (long)(sw.ElapsedTicks * nsPerTick);
    }
    Array.Sort(t);
    return t;
}
long Pct(long[] a, double q) => a[Math.Min(a.Length - 1, (int)(q * (a.Length - 1)))];

var vt = Loop(b, out int voks, out _, out int pages);
var mt = Loop(bad, out _, out int merrs, out _);
string json = $"{{\"binding\":\"dotnet\",\"valid\":{{\"samples\":{iters},\"oks\":{voks},\"pages\":{pages},\"min_ns\":{vt[0]},\"p50_ns\":{Pct(vt,.5)},\"p95_ns\":{Pct(vt,.95)},\"p99_ns\":{Pct(vt,.99)},\"max_ns\":{vt[iters-1]}}},\"malformed\":{{\"samples\":{iters},\"errs\":{merrs},\"p50_ns\":{Pct(mt,.5)},\"p95_ns\":{Pct(mt,.95)}}},\"status\":\"green_measured\"}}";
File.WriteAllText(outp, json);
Console.WriteLine("JD_DOTNET_PERF " + json);
