// Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
//
// This software is proprietary. The PDFluent application is free to use,
// including for commercial purposes. Redistribution, or extraction or reuse
// of its components (including the embedded PDF engine), requires a licence.
// See https://pdfluent.com/license for terms.

using System;
using System.IO;
using System.Text;
using Xunit;

namespace PDFluent.Tests
{
    /// <summary>
    /// Office export over the C ABI.
    ///
    /// Until 23-08-2026 no binding could convert a PDF to Word, Excel or
    /// PowerPoint while the feature page sold it. The Rust crates existed and
    /// stopped at the language boundary.
    ///
    /// The assertions look at the bytes rather than the status code: an OOXML
    /// package is a ZIP with a known entry, and "the call returned something"
    /// is not the same claim as "Word opens this".
    /// </summary>
    public class OfficeExportTests
    {
        /// <summary>
        /// The same fixture the C ABI suite converts. A synthetic in-memory PDF
        /// is not usable here: the office writers reparse the serialised bytes,
        /// and a hand-written xref table does not survive that round trip.
        /// </summary>
        private static byte[] SamplePdf()
        {
            var dir = new DirectoryInfo(AppContext.BaseDirectory);
            while (dir != null)
            {
                string candidate = Path.Combine(dir.FullName, "fixtures", "sample.pdf");
                if (File.Exists(candidate)) return File.ReadAllBytes(candidate);
                dir = dir.Parent;
            }
            throw new FileNotFoundException(
                "fixtures/sample.pdf not found above " + AppContext.BaseDirectory);
        }

        private static PdfDocument OpenSample() => PdfDocument.Open(SamplePdf());

        private static void AssertOoxml(byte[] pkg, string entry, string what)
        {
            Assert.True(pkg.Length > 0, $"{what}: empty package");
            Assert.Equal("PK", Encoding.Latin1.GetString(pkg, 0, 2));
            Assert.Contains(entry, Encoding.Latin1.GetString(pkg));
        }

        public OfficeExportTests()
        {
            // Office export is Business and up. The refusal path runs in its
            // own test class file for the reason found in the C ABI suite:
            // licence state is process-wide, so a refusal test sharing a
            // process with a licensed one proves nothing.
            Licensing.ActivateKey("tier:business");
        }

        [Fact]
        public void ToDocxReturnsAPackageWordOpens()
        {
            using var doc = OpenSample();
            AssertOoxml(doc.ToDocx(), "word/document.xml", "docx");
        }

        [Fact]
        public void ToXlsxReturnsAPackageExcelOpens()
        {
            using var doc = OpenSample();
            AssertOoxml(doc.ToXlsx(), "xl/workbook.xml", "xlsx");
        }

        [Fact]
        public void ToPptxReturnsAPackagePowerPointOpens()
        {
            using var doc = OpenSample();
            AssertOoxml(doc.ToPptx(), "ppt/presentation.xml", "pptx");
        }
    }
}
