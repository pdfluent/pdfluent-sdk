using System;
using System.Text;
using Xunit;

namespace XfaPdf.Tests
{
    /// <summary>
    /// xUnit tests for PdfDocument P/Invoke bindings.
    ///
    /// These tests require the native library to be built and available.
    /// Build the native library first:
    ///   cargo build -p pdf-capi --release
    ///
    /// Then run:
    ///   dotnet test bindings/dotnet/test/XfaPdf.Tests/
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
        public void InvalidPdfThrows()
        {
            Assert.Throws<PdfException>(() => PdfDocument.Open(new byte[] { 1, 2, 3 }));
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
            doc.Dispose(); // should not throw
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
        public void PdfExceptionHasStatus()
        {
            try
            {
                PdfDocument.Open(new byte[] { 1, 2, 3 });
                Assert.Fail("Should have thrown");
            }
            catch (PdfException ex)
            {
                Assert.Equal(PdfStatus.ErrorCorruptPdf, ex.Status);
            }
        }
    }
}
