// Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
//
// This software is proprietary. The PDFluent application is free to use,
// including for commercial purposes. Redistribution, or extraction or reuse
// of its components (including the embedded PDF engine), requires a licence.
// See https://pdfluent.com/license for terms.

using System;
using System.IO;
using Xunit;

namespace PDFluent.Tests.Unlicensed
{
    /// <summary>
    /// Office export must refuse without a Business licence.
    ///
    /// This lives in its own assembly, and therefore its own process, on
    /// purpose. The licence is process-wide state: a refusal test that shares
    /// a process with a test calling ActivateKey proves nothing about the
    /// refusal, because whichever ran first decides the answer for both. The
    /// first version of the C ABI suite made exactly that mistake — deleting
    /// the capability check left all five tests green.
    /// </summary>
    public class OfficeExportUnlicensedTests
    {
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

        private static void AssertRefused(Func<byte[]> call, string what)
        {
            var ex = Assert.ThrowsAny<PdfluentException>(() => call());
            Assert.Contains("pdfluent.com", ex.Message);
        }

        [Fact]
        public void ToDocxRefusesWithoutALicence()
        {
            using var doc = PdfDocument.Open(SamplePdf());
            AssertRefused(doc.ToDocx, "docx");
        }

        [Fact]
        public void ToXlsxRefusesWithoutALicence()
        {
            using var doc = PdfDocument.Open(SamplePdf());
            AssertRefused(doc.ToXlsx, "xlsx");
        }

        [Fact]
        public void ToPptxRefusesWithoutALicence()
        {
            using var doc = PdfDocument.Open(SamplePdf());
            AssertRefused(doc.ToPptx, "pptx");
        }
    }
}
