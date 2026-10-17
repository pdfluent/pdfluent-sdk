// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

using System;
using System.Collections.Generic;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using Xunit;

namespace PDFluent.Tests
{
    /// <summary>
    /// Plain known-images on the shared construction fixture (#643).
    /// <see cref="PdfDocument.PageImages(int)"/> plus
    /// <see cref="PdfDocument.DecodeImagePixels(uint)"/> over
    /// fixtures/known-images.pdf, checked against
    /// fixtures/known-images.expected.json: count, dimensions, colour
    /// space, components, bpc, and the decoded-pixel sha256.
    /// <see cref="PdfDocument.ExtractImageData(uint)"/> rides along to pin
    /// the encoded-vs-pixels split (PNG bytes, not samples).
    /// </summary>
    public class PlainKnownImagesTests
    {
        private static string FindFixture(string name)
        {
            string? at = AppContext.BaseDirectory;
            while (at != null)
            {
                string candidate = Path.Combine(at, "fixtures", name);
                if (File.Exists(candidate))
                {
                    return candidate;
                }
                at = Path.GetDirectoryName(at);
            }
            string cwd = Directory.GetCurrentDirectory();
            string? cur = cwd;
            while (cur != null)
            {
                string candidate = Path.Combine(cur, "fixtures", name);
                if (File.Exists(candidate))
                {
                    return candidate;
                }
                cur = Path.GetDirectoryName(cur);
            }
            throw new FileNotFoundException("fixtures/" + name + " not found above " + cwd);
        }

        private static string NormSpace(string name)
        {
            var kept = new StringBuilder();
            string lower = name.ToLowerInvariant().Replace("device", "").Replace("grey", "gray");
            foreach (char c in lower)
            {
                if ((c >= 'a' && c <= 'z') || (c >= '0' && c <= '9'))
                {
                    kept.Append(c);
                }
            }
            return kept.ToString();
        }

        private static List<(int Width, int Height, string Space, int Comps, int Bpc, string Sha)> Expected()
        {
            string jsonPath = FindFixture("known-images.expected.json");
            var want = new List<(int Width, int Height, string Space, int Comps, int Bpc, string Sha)>();
            using (var jsonDoc = JsonDocument.Parse(File.ReadAllText(jsonPath)))
            {
                foreach (var el in jsonDoc.RootElement.GetProperty("images").EnumerateArray())
                {
                    want.Add((
                        el.GetProperty("width").GetInt32(),
                        el.GetProperty("height").GetInt32(),
                        el.GetProperty("colorspace").GetString()!,
                        el.GetProperty("components").GetInt32(),
                        el.GetProperty("bpc").GetInt32(),
                        el.GetProperty("sha256").GetString()!
                    ));
                }
            }
            Assert.Equal(2, want.Count);
            want.Sort((a, b) => a.Width != b.Width ? a.Width.CompareTo(b.Width) : a.Height.CompareTo(b.Height));
            return want;
        }

        private static List<(uint Id, int Width, int Height, int Comps, string Space, int Bpc)> Listing(PdfDocument doc)
        {
            using JsonDocument parsed = JsonDocument.Parse(doc.PageImages(0));
            var outList = new List<(uint Id, int Width, int Height, int Comps, string Space, int Bpc)>();
            foreach (var el in parsed.RootElement.EnumerateArray())
            {
                outList.Add((
                    el.GetProperty("id").GetUInt32(),
                    el.GetProperty("width").GetInt32(),
                    el.GetProperty("height").GetInt32(),
                    el.GetProperty("components").GetInt32(),
                    el.GetProperty("color_space").GetString()!,
                    el.GetProperty("bits_per_component").GetInt32()
                ));
            }
            outList.Sort((a, b) => a.Width != b.Width ? a.Width.CompareTo(b.Width) : a.Height.CompareTo(b.Height));
            return outList;
        }

        [Fact]
        public void PageImagesListsTheSharedFixture()
        {
            var want = Expected();
            using var doc = PdfDocument.Open(File.ReadAllBytes(FindFixture("known-images.pdf")));
            var got = Listing(doc);
            Assert.Equal(want.Count, got.Count);
            Assert.Equal(2, got.Count);
            for (int i = 0; i < want.Count; i++)
            {
                Assert.Equal(want[i].Width, got[i].Width);
                Assert.Equal(want[i].Height, got[i].Height);
                Assert.Equal(NormSpace(want[i].Space), NormSpace(got[i].Space));
                Assert.Equal(want[i].Comps, got[i].Comps);
                Assert.Equal(want[i].Bpc, got[i].Bpc);
            }
        }

        [Fact]
        public void DecodeImagePixelsMatchesTheExpectedSha()
        {
            var want = Expected();
            using var doc = PdfDocument.Open(File.ReadAllBytes(FindFixture("known-images.pdf")));
            var got = Listing(doc);
            Assert.Equal(2, got.Count);
            using var sha = SHA256.Create();
            foreach (var entry in got)
            {
                (int Width, int Height, string Space, int Comps, int Bpc, string Sha)? match = null;
                foreach (var w in want)
                {
                    if (w.Width == entry.Width && w.Height == entry.Height)
                    {
                        match = w;
                    }
                }
                Assert.True(match.HasValue, "unexpected size " + entry.Width + "x" + entry.Height);
                var wv = match!.Value;
                DecodedImage decoded = doc.DecodeImagePixels(entry.Id);
                Assert.Equal(wv.Width, decoded.Width);
                Assert.Equal(wv.Height, decoded.Height);
                Assert.Equal(wv.Comps, decoded.Components);
                Assert.Equal(NormSpace(wv.Space), NormSpace(decoded.ColorSpace));
                Assert.Equal(wv.Width * wv.Height * wv.Comps, decoded.Pixels.Length);
                string digest = BitConverter.ToString(sha.ComputeHash(decoded.Pixels)).Replace("-", "").ToLowerInvariant();
                Assert.Equal(wv.Sha, digest);
            }
        }

        [Fact]
        public void ExtractImageDataIsPngNotPixels()
        {
            using var doc = PdfDocument.Open(File.ReadAllBytes(FindFixture("known-images.pdf")));
            var got = Listing(doc);
            Assert.Equal(2, got.Count);
            foreach (var entry in got)
            {
                ExtractedImage extracted = doc.ExtractImageData(entry.Id);
                Assert.Equal("png", extracted.Encoding);
                Assert.True(extracted.Data.Length > 4);
                Assert.Equal(0x89, extracted.Data[0]);
                Assert.Equal((byte)'P', extracted.Data[1]);
                Assert.Equal((byte)'N', extracted.Data[2]);
                Assert.Equal((byte)'G', extracted.Data[3]);
                DecodedImage decoded = doc.DecodeImagePixels(entry.Id);
                Assert.NotEqual(extracted.Data.Length, decoded.Pixels.Length);
            }
        }
    }
}
