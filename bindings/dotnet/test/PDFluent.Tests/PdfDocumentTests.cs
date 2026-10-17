// Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
//
// This software is proprietary. The PDFluent application is free to use,
// including for commercial purposes. Redistribution, or extraction or reuse
// of its components (including the embedded PDF engine), requires a licence.
// See https://pdfluent.com/license for terms.

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
    /// xUnit tests for PdfDocument P/Invoke bindings.
    ///
    /// These tests require the native library to be built and available.
    /// Build the native library first:
    ///   cargo build -p pdf-capi --release
    ///
    /// Then run:
    ///   dotnet test bindings/dotnet/test/PDFluent.Tests/
    ///
    /// On macOS, set DYLD_LIBRARY_PATH to the Rust target directory:
    ///   export DYLD_LIBRARY_PATH=target/release
    /// On Linux:
    ///   export LD_LIBRARY_PATH=target/release
    /// On Windows, copy pdf_capi.dll to the test output directory.
    /// </summary>
    public class PdfDocumentTests : IDisposable
    {
        private static byte[] CreateTestPdf()
        {
            string pdf =
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
            return Encoding.ASCII.GetBytes(pdf);
        }

        /// <summary>
        /// A file from the repository's <c>fixtures</c> directory, found by
        /// walking up from the test assembly (same pattern as the office and
        /// structured suites, generalised to any fixture name).
        /// </summary>
        private static byte[] LoadFixture(string name)
        {
            var dir = new DirectoryInfo(AppContext.BaseDirectory);
            while (dir != null)
            {
                string candidate = Path.Combine(dir.FullName, "fixtures", name);
                if (File.Exists(candidate)) return File.ReadAllBytes(candidate);
                dir = dir.Parent;
            }
            throw new FileNotFoundException(
                "fixtures/" + name + " not found above " + AppContext.BaseDirectory);
        }

        // Embedded synthetic AcroForm fixtures, byte-identical to the Java
        // suite's (from pdfluent-forms gen_acroform_corpus), so the form tests
        // need no external files and assert the same field names.
        private const string MultiselectPdfB64 =
            "JVBERi0xLjcKJbutwN4KMSAwIG9iago8PC9UeXBlL1BhZ2VzL0tpZHNbMyAwIFJdL0NvdW50IDE+PgplbmRvYmoKMiAwIG9iago8PC9MZW5ndGggMD4+c3RyZWFtCgplbmRzdHJlYW0gCmVuZG9iagozIDAgb2JqCjw8L1R5cGUvUGFnZS9QYXJlbnQgMSAwIFIvTWVkaWFCb3hbMCAwIDYxMiA3OTJdL0NvbnRlbnRzIDIgMCBSL1Jlc291cmNlczw8Pj4vQW5ub3RzWzQgMCBSXT4+CmVuZG9iago0IDAgb2JqCjw8L1R5cGUvQW5ub3QvU3VidHlwZS9XaWRnZXQvRlQvQ2gvVChsYW5ndWFnZXMpL0ZmIDIwOTcxNTIvUmVjdFsxMDAgNDAwIDMyMCA1MjBdL09wdFsoRU4pKE5MKShERSkoRlIpXT4+CmVuZG9iago1IDAgb2JqCjw8L0ZpZWxkc1s0IDAgUl0vREEoL0hlbHYgMCBUZiAwIGcpPj4KZW5kb2JqCjYgMCBvYmoKPDwvVHlwZS9DYXRhbG9nL1BhZ2VzIDEgMCBSL0Fjcm9Gb3JtIDUgMCBSPj4KZW5kb2JqCjcgMCBvYmoKPDwvUm9vdCA2IDAgUi9UeXBlL1hSZWYvU2l6ZSA4L1dbMSA0IDJdL0luZGV4WzEgN10vTGVuZ3RoIDQ5Pj5zdHJlYW0KAQAAAA8AAAEAAABCAAABAAAAcQAAAQAAAN0AAAEAAAFVAAABAAABigAAAQAAAcYAAAplbmRzdHJlYW0gCmVuZG9iagoKc3RhcnR4cmVmCjQ1NAolJUVPRg==";
        private const string PureTextPdfB64 =
            "JVBERi0xLjcKJbutwN4KMSAwIG9iago8PC9UeXBlL1BhZ2VzL0tpZHNbMyAwIFJdL0NvdW50IDE+PgplbmRvYmoKMiAwIG9iago8PC9MZW5ndGggMD4+c3RyZWFtCgplbmRzdHJlYW0gCmVuZG9iagozIDAgb2JqCjw8L1R5cGUvUGFnZS9QYXJlbnQgMSAwIFIvTWVkaWFCb3hbMCAwIDYxMiA3OTJdL0NvbnRlbnRzIDIgMCBSL1Jlc291cmNlczw8Pj4vQW5ub3RzWzQgMCBSXT4+CmVuZG9iago0IDAgb2JqCjw8L1R5cGUvQW5ub3QvU3VidHlwZS9XaWRnZXQvRlQvVHgvVChmdWxsX25hbWUpL1JlY3RbMTAwIDcwMCAzMjAgNzIwXT4+CmVuZG9iago1IDAgb2JqCjw8L0ZpZWxkc1s0IDAgUl0vREEoL0hlbHYgMCBUZiAwIGcpPj4KZW5kb2JqCjYgMCBvYmoKPDwvVHlwZS9DYXRhbG9nL1BhZ2VzIDEgMCBSL0Fjcm9Gb3JtIDUgMCBSPj4KZW5kb2JqCjcgMCBvYmoKPDwvUm9vdCA2IDAgUi9UeXBlL1hSZWYvU2l6ZSA4L1dbMSA0IDJdL0luZGV4WzEgN10vTGVuZ3RoIDQ5Pj5zdHJlYW0KAQAAAA8AAAEAAABCAAABAAAAcQAAAQAAAN0AAAEAAAE0AAABAAABaQAAAQAAAaUAAAplbmRzdHJlYW0gCmVuZG9iagoKc3RhcnR4cmVmCjQyMQolJUVPRg==";

        private static byte[] MultiselectPdf() => Convert.FromBase64String(MultiselectPdfB64);

        private static byte[] PureTextPdf() => Convert.FromBase64String(PureTextPdfB64);

        public void Dispose() { }

        [Fact]
        public void OpenAndDispose()
        {
            var doc = PdfDocument.Open(CreateTestPdf());
            Assert.False(doc.IsDisposed);
            doc.Dispose();
            Assert.True(doc.IsDisposed);
        }

        [Fact]
        public void UsingPattern()
        {
            using var doc = PdfDocument.Open(CreateTestPdf());
            Assert.False(doc.IsDisposed);
            Assert.True(doc.PageCount > 0);
        }

        [Fact]
        public void PageCount()
        {
            using var doc = PdfDocument.Open(CreateTestPdf());
            Assert.Equal(1, doc.PageCount);
        }

        [Fact]
        public void PageDimensions()
        {
            using var doc = PdfDocument.Open(CreateTestPdf());
            double width = doc.GetPageWidth(0);
            double height = doc.GetPageHeight(0);
            Assert.Equal(612.0, width, 1.0);
            Assert.Equal(792.0, height, 1.0);
        }

        [Fact]
        public void PageRotation()
        {
            using var doc = PdfDocument.Open(CreateTestPdf());
            Assert.Equal(0, doc.GetPageRotation(0));
        }

        [Fact]
        public void MediaBox()
        {
            using var doc = PdfDocument.Open(CreateTestPdf());
            PageBox box = doc.GetMediaBox(0);
            Assert.Equal(0.0, box.X0, 1.0);
            Assert.Equal(0.0, box.Y0, 1.0);
            Assert.Equal(612.0, box.X1, 1.0);
            Assert.Equal(792.0, box.Y1, 1.0);
            Assert.Equal(612.0, box.Width, 1.0);
            Assert.Equal(792.0, box.Height, 1.0);
        }

        [Fact]
        public void ExtractTextFromEmptyPage()
        {
            using var doc = PdfDocument.Open(CreateTestPdf());
            string text = doc.ExtractText(0);
            Assert.NotNull(text);
        }

        [Fact]
        public void ExtractTextBlocksFromEmptyPage()
        {
            // The 1-page in-memory fixture has no content stream, so we
            // expect zero blocks but never a null list.
            using var doc = PdfDocument.Open(CreateTestPdf());
            var blocks = doc.ExtractTextBlocks(0);
            Assert.NotNull(blocks);
            // Shape check on every (potential) entry.
            foreach (var b in blocks)
            {
                Assert.True(b.Width >= 0.0, $"width {b.Width} must be >= 0");
                Assert.True(b.Height >= 0.0, $"height {b.Height} must be >= 0");
                Assert.NotNull(b.Text);
            }
        }

        [Fact]
        public void ExtractTextBlocksOutOfRangeThrows()
        {
            using var doc = PdfDocument.Open(CreateTestPdf());
            Assert.Throws<PdfluentPageRangeException>(() => doc.ExtractTextBlocks(9999));
        }

        [Fact]
        public void ExtractTextBlocksNegativePageThrows()
        {
            using var doc = PdfDocument.Open(CreateTestPdf());
            // Negative page maps to ErrorInvalidArgument → PdfluentValidationException.
            Assert.Throws<PdfluentValidationException>(() => doc.ExtractTextBlocks(-1));
        }

        [Fact]
        public void InvalidPdfThrows()
        {
            // Any PdfluentException subtype is acceptable for a corrupt input.
            Assert.Throws<PdfluentParseException>(() => PdfDocument.Open(new byte[] { 1, 2, 3 }));
        }

        [Fact]
        public void NullDataThrows()
        {
            Assert.Throws<ArgumentNullException>(() => PdfDocument.Open((byte[])null!));
        }

        [Fact]
        public void DisposedDocumentThrows()
        {
            var doc = PdfDocument.Open(CreateTestPdf());
            doc.Dispose();
            Assert.Throws<ObjectDisposedException>(() => _ = doc.PageCount);
        }

        [Fact]
        public void DoubleDisposeIsSafe()
        {
            var doc = PdfDocument.Open(CreateTestPdf());
            doc.Dispose();
            doc.Dispose(); // must not throw
        }

        [Fact]
        public void MetadataReturnsNullForMissing()
        {
            using var doc = PdfDocument.Open(CreateTestPdf());
            Assert.Null(doc.GetMetadata("Title"));
        }

        [Fact]
        public void BookmarkCount()
        {
            using var doc = PdfDocument.Open(CreateTestPdf());
            Assert.Equal(0, doc.BookmarkCount);
        }

        [Fact]
        public void RenderPage()
        {
            using var doc = PdfDocument.Open(CreateTestPdf());
            RenderedImage img = doc.RenderPage(0, 72.0);
            Assert.NotNull(img);
            Assert.True(img.Width > 0);
            Assert.True(img.Height > 0);
            Assert.Equal(img.Width * img.Height * 4, img.Pixels.Length);
        }

        [Fact]
        public void RenderThumbnail()
        {
            using var doc = PdfDocument.Open(CreateTestPdf());
            RenderedImage img = doc.RenderThumbnail(0, 100);
            Assert.NotNull(img);
            Assert.True(img.Width <= 100);
            Assert.True(img.Height <= 100);
        }

        [Fact]
        public void PdfluentExceptionHasNativeStatus()
        {
            try
            {
                PdfDocument.Open(new byte[] { 1, 2, 3 });
                Assert.Fail("Should have thrown");
            }
            catch (PdfluentParseException ex)
            {
                Assert.Equal(PdfStatus.ErrorCorruptPdf, ex.NativeStatus);
            }
        }

        [Fact]
        public void PdfluentExceptionIsBaseForParseException()
        {
            // Verify the hierarchy: PdfluentParseException is-a PdfluentException.
            var ex = Assert.Throws<PdfluentParseException>(
                () => PdfDocument.Open(new byte[] { 1, 2, 3 }));
            Assert.IsAssignableFrom<PdfluentException>(ex);
        }

        // ---------- Scenario 5: Read AcroForm fields ----------

        // mirrors bindings/java/src/test/java/com/pdfluent/PdfluentDocumentTest.java::readFormFields
        // NOTE: GetFormFields + FormField (Name/FieldType) do not exist on
        // PdfDocument yet; the names follow the Java test.
        [Fact]
        public void ReadFormFields()
        {
            using var doc = PdfDocument.Open(MultiselectPdf());
            var fields = doc.GetFormFields();
            Assert.NotEmpty(fields);
            FormField? languages = null;
            foreach (var f in fields)
            {
                if (f.Name == "languages")
                {
                    languages = f;
                    break;
                }
            }
            Assert.True(languages != null, "languages field missing");
            Assert.Equal("choice", languages!.FieldType);
        }

        // ---------- Scenario 6: Fill text field, save ----------

        // mirrors bindings/java/src/test/java/com/pdfluent/PdfluentDocumentTest.java::writeTextFormFieldRoundTrips
        // NOTE: SetFormField + Save(path) do not exist on PdfDocument yet;
        // the names follow the Java test.
        [Fact]
        public void WriteFormField()
        {
            string outPath = Path.GetTempFileName();
            try
            {
                using (var doc = PdfDocument.Open(PureTextPdf()))
                {
                    Assert.True(doc.SetFormField("full_name", "Jane Doe"));
                    doc.Save(outPath);
                }
                using var doc2 = PdfDocument.Open(File.ReadAllBytes(outPath));
                string value = "";
                foreach (var f in doc2.GetFormFields())
                {
                    if (f.Name == "full_name")
                    {
                        value = f.Value ?? "";
                        break;
                    }
                }
                Assert.Equal("Jane Doe", value);
            }
            finally
            {
                File.Delete(outPath);
            }
        }

        // ---------- Scenario 7: Read annotations ----------

        // mirrors crates/pdf-python/tests/test_smoke.py::test_annotations_read
        // NOTE: GetAnnotations + Annotation (AnnotType/Rect) do not exist on
        // PdfDocument yet; the names follow the Python test. (The Page-level
        // AnnotEntry calls the type TypeName; the implementer reconciles.)
        [Fact]
        public void ReadAnnotations()
        {
            using var doc = PdfDocument.Open(LoadFixture("sample.pdf"));
            var annots = doc.GetAnnotations(0);
            Assert.NotNull(annots);
            foreach (var a in annots)
            {
                Assert.IsType<string>(a.AnnotType);
                Assert.IsType<Rect>(a.Rect);
                // A rect is four edges: reading all four is the shape check
                // the Python test writes as isinstance(tuple) + len == 4.
                double[] edges = { a.Rect.X0, a.Rect.Y0, a.Rect.X1, a.Rect.Y1 };
                Assert.Equal(4, edges.Length);
            }
        }

        // ---------- Scenario 8: Add highlight, save ----------

        // mirrors crates/pdf-python/tests/test_smoke.py::test_annotation_highlight
        // NOTE: AddAnnotation + Save(path) do not exist on PdfDocument yet;
        // the names follow the Python test. (Page.AddHighlightAnnot exists
        // with a quads shape; the mirrored call keeps Python's page/type/
        // rect/text form.)
        [Fact]
        public void AddHighlightAnnotation()
        {
            using var doc = PdfDocument.Open(LoadFixture("sample.pdf"));
            doc.AddAnnotation(0, "highlight", new Rect(72.0, 700.0, 300.0, 720.0), "highlighted text");
            string outPath = Path.GetTempFileName();
            try
            {
                doc.Save(outPath);
                using var reloaded = PdfDocument.Open(File.ReadAllBytes(outPath));
                Assert.Equal(doc.PageCount, reloaded.PageCount);
                Assert.NotEmpty(reloaded.GetAnnotations(0));
            }
            finally
            {
                File.Delete(outPath);
            }
        }

        // ---------- Scenario 9: Validate PDF/A ----------

        // mirrors crates/pdf-python/tests/test_smoke.py::test_pdfa_validation
        // NOTE: the static ValidatePdfA + its report (IsCompliant/ErrorCount/
        // WarningCount/Issues) do not exist on PdfDocument yet; the names
        // follow the Python test. (A Java JNI-level test also exists but
        // asserts on the native flat string array, not the public report.)
        [Fact]
        public void ValidatePdfA()
        {
            var report = PdfDocument.ValidatePdfA(LoadFixture("sample.pdf"));
            Assert.NotNull(report);
            Assert.IsType<bool>(report.IsCompliant);
            Assert.IsType<int>(report.ErrorCount);
            Assert.IsType<int>(report.WarningCount);
            Assert.NotNull(report.Issues);
            Assert.IsAssignableFrom<System.Collections.IEnumerable>(report.Issues);
        }

        // ---------- Scenario 10: Merge 2 PDFs ----------

        // mirrors crates/pdf-python/tests/test_smoke.py::test_merge_pdfs
        // NOTE: the static Merge (Python: merge_pdfs) does not exist on
        // PdfDocument yet; it returns the merged bytes per the stub shape.
        [Fact]
        public void MergePdfs()
        {
            byte[] merged = PdfDocument.Merge(LoadFixture("sample.pdf"), LoadFixture("multi-page.pdf"));
            using var doc = PdfDocument.Open(merged);
            using var docA = PdfDocument.Open(LoadFixture("sample.pdf"));
            using var docB = PdfDocument.Open(LoadFixture("multi-page.pdf"));
            Assert.Equal(docA.PageCount + docB.PageCount, doc.PageCount);
        }

        // ---------- Scenario 11: Verify signature ----------

        [Fact]
        public void VerifySignature()
        {
            // The in-memory fixture is unsigned, so the signature surface must
            // report zero signatures across every accessor — and the calls must
            // actually traverse the P/Invoke boundary into the native engine.
            using var doc = PdfDocument.Open(CreateTestPdf());

            // Count accessor.
            Assert.Equal(0, doc.SignatureCount());

            // Structured verification: empty (never null) for an unsigned doc.
            var sigs = doc.VerifySignatures();
            Assert.NotNull(sigs);
            Assert.Empty(sigs);

            // AllValid() over an empty set is false (no signature proves trust).
            Assert.False(sigs.AllValid());

            // Out-of-range index is reported as not-valid, never throws.
            Assert.False(doc.IsSignatureValid(0));
            Assert.False(doc.IsSignatureValid(9999));
        }

        // ---------- Scenario 12: Extract images ----------

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
                // Test output nests several levels below the repo root; also
                // try walking straight up for a fixtures sibling.
                string sibling = Path.Combine(at, name);
                if (File.Exists(sibling) && Path.GetFileName(at) == "fixtures")
                {
                    return sibling;
                }
                at = Path.GetDirectoryName(at);
            }
            // Fall back to the working directory walk (dotnet test runs with
            // the project directory as cwd).
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

        [Fact]
        public void ExtractImages()
        {
            // Known images through the plain image API (#643). Reads
            // fixtures/known-images.expected.json; no fixture literals
            // duplicated. Plain route (PdfDocument.PageImages plus
            // PdfDocument.DecodeImagePixels): the compat ExtractImage
            // returns the encoded file, not samples.
            string pdfPath = FindFixture("known-images.pdf");
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
            want.Sort((a, b) => a.Width != b.Width ? a.Width.CompareTo(b.Width) : a.Height.CompareTo(b.Height));
            using var doc = PdfDocument.Open(File.ReadAllBytes(pdfPath));
            using JsonDocument parsed = JsonDocument.Parse(doc.PageImages(0));
            var refs = new List<(uint Id, int Width, int Height, int Comps, string Space, int Bpc)>();
            foreach (var el in parsed.RootElement.EnumerateArray())
            {
                refs.Add((
                    el.GetProperty("id").GetUInt32(),
                    el.GetProperty("width").GetInt32(),
                    el.GetProperty("height").GetInt32(),
                    el.GetProperty("components").GetInt32(),
                    el.GetProperty("color_space").GetString()!,
                    el.GetProperty("bits_per_component").GetInt32()
                ));
            }
            Assert.Equal(want.Count, refs.Count);
            Assert.Equal(2, refs.Count);
            refs.Sort((a, b) => a.Width != b.Width ? a.Width.CompareTo(b.Width) : a.Height.CompareTo(b.Height));
            using var sha = SHA256.Create();
            for (int i = 0; i < want.Count; i++)
            {
                Assert.Equal(want[i].Width, refs[i].Width);
                Assert.Equal(want[i].Height, refs[i].Height);
                Assert.Equal(NormSpace(want[i].Space), NormSpace(refs[i].Space));
                Assert.Equal(want[i].Bpc, refs[i].Bpc);
                DecodedImage decoded = doc.DecodeImagePixels(refs[i].Id);
                Assert.Equal(want[i].Width, decoded.Width);
                Assert.Equal(want[i].Height, decoded.Height);
                Assert.Equal(want[i].Comps, decoded.Components);
                Assert.Equal(want[i].Width * want[i].Height * want[i].Comps, decoded.Pixels.Length);
                string digest = BitConverter.ToString(sha.ComputeHash(decoded.Pixels)).Replace("-", "").ToLowerInvariant();
                Assert.Equal(want[i].Sha, digest);
            }
        }
    }
}
