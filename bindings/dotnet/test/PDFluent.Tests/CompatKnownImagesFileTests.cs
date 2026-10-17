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
using StbImageSharp;
using Xunit;

namespace PDFluent.Tests
{
    /// <summary>
    /// Compat known-images as encoded files, decoded independently (#636).
    /// <see cref="Document.ExtractImage(int)"/> over fixtures/known-images.pdf
    /// returns the image FILE (header facts plus encoded bytes), checked
    /// against fixtures/known-images.expected.json: <c>Ext</c> matches the
    /// payload's magic bytes, and the payload decoded with StbImageSharp
    /// (third party, not PDFluent) yields the expected dimensions,
    /// components, and decoded-pixel sha256. A lossy (JPEG) payload cannot
    /// match a pixel sha by construction, so it pins only magic bytes,
    /// dimensions, and components.
    ///
    /// These tests require the native library to be built and available, like
    /// <see cref="PdfDocumentTests"/>: <c>cargo build -p pdf-capi --release</c>,
    /// then <c>dotnet test bindings/dotnet/test/PDFluent.Tests/</c>.
    /// </summary>
    public class CompatKnownImagesFileTests
    {
        private static readonly byte[] PngMagic = { 0x89, (byte)'P', (byte)'N', (byte)'G', 0x0D, 0x0A, 0x1A, 0x0A };
        private static readonly byte[] JpegMagic = { 0xFF, 0xD8, 0xFF };

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

        private static bool StartsWith(byte[] data, byte[] magic)
        {
            if (data.Length < magic.Length)
            {
                return false;
            }
            for (int i = 0; i < magic.Length; i++)
            {
                if (data[i] != magic[i])
                {
                    return false;
                }
            }
            return true;
        }

        [Fact]
        public void ExtractImageReturnsTheExpectedEncodedFile()
        {
            var want = Expected();
            using var doc = new Document(File.ReadAllBytes(FindFixture("known-images.pdf")));
            List<ImageRef> refs = doc.GetPageImages(0);
            Assert.Equal(2, refs.Count);
            using var sha = SHA256.Create();
            foreach (var r in refs)
            {
                (int Width, int Height, string Space, int Comps, int Bpc, string Sha)? match = null;
                foreach (var w in want)
                {
                    if (w.Width == r.Width && w.Height == r.Height)
                    {
                        match = w;
                    }
                }
                Assert.True(match.HasValue, "unexpected size " + r.Width + "x" + r.Height);
                var wv = match!.Value;
                ImageData image = doc.ExtractImage(r.Xref);
                Assert.True(image.Ext == "png" || image.Ext == "jpeg", "ext is a file kind, got '" + image.Ext + "'");
                Assert.Equal(wv.Width, image.Width);
                Assert.Equal(wv.Height, image.Height);
                Assert.Equal(wv.Comps, image.Colorspace);
                Assert.Equal(NormSpace(wv.Space), NormSpace(image.ColorspaceName));
                Assert.Equal(wv.Bpc, image.Bits);
                if (image.Ext == "png")
                {
                    Assert.True(StartsWith(image.Data, PngMagic), "png ext matches PNG magic");
                    ColorComponents wantLayout = wv.Comps == 1 ? ColorComponents.Grey : ColorComponents.RedGreenBlue;
                    ImageResult decoded = ImageResult.FromMemory(image.Data, wantLayout);
                    Assert.Equal(wv.Width, decoded.Width);
                    Assert.Equal(wv.Height, decoded.Height);
                    Assert.Equal(wantLayout, decoded.SourceComp);
                    Assert.Equal(wv.Width * wv.Height * wv.Comps, decoded.Data.Length);
                    string digest = BitConverter.ToString(sha.ComputeHash(decoded.Data)).Replace("-", "").ToLowerInvariant();
                    Assert.Equal(wv.Sha, digest);
                }
                else
                {
                    Assert.True(StartsWith(image.Data, JpegMagic), "jpeg ext matches JPEG magic");
                    ImageResult decoded = ImageResult.FromMemory(image.Data);
                    Assert.Equal(wv.Width, decoded.Width);
                    Assert.Equal(wv.Height, decoded.Height);
                }
            }
        }
    }
}
