using System;
using System.IO;
using System.Text;
using System.Text.Json;
using Xunit;

namespace PDFluent.Tests
{
    /// <summary>
    /// xUnit tests for the plain embedded-image API on <see cref="PdfDocument"/>.
    ///
    /// <see cref="PdfDocument.PageImages(int)"/> lists a page's stored images,
    /// <see cref="PdfDocument.ExtractImageData(uint)"/> returns one image's
    /// encoded bytes with header facts, and
    /// <see cref="PdfDocument.DecodeImagePixels(uint)"/> decodes to raw samples.
    /// The fixture is built in the test: one page holding a raw 2x2 grey image,
    /// so the known pixels below are the samples the builder wrote.
    ///
    /// These tests require the native library to be built and available, like
    /// <see cref="PdfDocumentTests"/>: <c>cargo build -p pdf-capi --release</c>,
    /// then <c>dotnet test bindings/dotnet/test/PDFluent.Tests/</c>.
    /// </summary>
    public class ImageExtractionTests
    {
        private static readonly byte[] GraySamples = { 0, 128, 192, 255 };

        private static byte[] CreateGrayImagePdf()
        {
            byte[] content = { (byte)'q', (byte)'\n', (byte)'Q', (byte)'\n' };
            var objects = new System.Collections.Generic.List<byte[]>
            {
                Encoding.ASCII.GetBytes("<< /Type /Catalog /Pages 2 0 R >>"),
                Encoding.ASCII.GetBytes("<< /Type /Pages /Kids [3 0 R] /Count 1 >>"),
                Encoding.ASCII.GetBytes(
                    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources 5 0 R >>"),
                Concat(
                    Encoding.ASCII.GetBytes("<< /Length " + content.Length + " >>\nstream\n"),
                    content,
                    Encoding.ASCII.GetBytes("\nendstream")),
                Encoding.ASCII.GetBytes("<< /XObject << /G0 6 0 R >> >>"),
                Concat(
                    Encoding.ASCII.GetBytes(
                        "<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceGray"
                        + " /BitsPerComponent 8 /Length " + GraySamples.Length + " >>\nstream\n"),
                    GraySamples,
                    Encoding.ASCII.GetBytes("\nendstream")),
            };
            using var pdf = new MemoryStream();
            var offsets = new System.Collections.Generic.List<int>();
            Write(pdf, "%PDF-1.7\n");
            for (int i = 0; i < objects.Count; i++)
            {
                offsets.Add((int)pdf.Length);
                Write(pdf, (i + 1) + " 0 obj\n");
                pdf.Write(objects[i], 0, objects[i].Length);
                Write(pdf, "\nendobj\n");
            }
            long xrefAt = pdf.Length;
            Write(pdf, "xref\n0 " + (objects.Count + 1) + "\n");
            Write(pdf, "0000000000 65535 f \n");
            foreach (int offset in offsets)
            {
                Write(pdf, offset.ToString("D10") + " 00000 n \n");
            }
            Write(pdf, "trailer\n<< /Size " + (objects.Count + 1) + " /Root 1 0 R >>\nstartxref\n"
                + xrefAt + "\n%%EOF\n");
            return pdf.ToArray();
        }

        private static void Write(MemoryStream stream, string text)
        {
            byte[] bytes = Encoding.ASCII.GetBytes(text);
            stream.Write(bytes, 0, bytes.Length);
        }

        private static byte[] Concat(params byte[][] parts)
        {
            using var stream = new MemoryStream();
            foreach (byte[] part in parts)
            {
                stream.Write(part, 0, part.Length);
            }
            return stream.ToArray();
        }

        private static uint ListingId(string json)
        {
            using JsonDocument doc = JsonDocument.Parse(json);
            return doc.RootElement[0].GetProperty("id").GetUInt32();
        }

        [Fact]
        public void PageImagesListsTheStoredImage()
        {
            using var doc = PdfDocument.Open(CreateGrayImagePdf());
            string json = doc.PageImages(0);
            using JsonDocument parsed = JsonDocument.Parse(json);
            Assert.Equal(1, parsed.RootElement.GetArrayLength());
            JsonElement image = parsed.RootElement[0];
            Assert.Equal(2, image.GetProperty("width").GetInt32());
            Assert.Equal(2, image.GetProperty("height").GetInt32());
            Assert.Equal(8, image.GetProperty("bits_per_component").GetInt32());
            Assert.Equal(1, image.GetProperty("components").GetInt32());
            Assert.Equal("DeviceGray", image.GetProperty("color_space").GetString());
            Assert.Equal("G0", image.GetProperty("name").GetString());
            Assert.False(image.GetProperty("has_mask").GetBoolean());
        }

        [Fact]
        public void PageImagesPastTheLastPageThrows()
        {
            using var doc = PdfDocument.Open(CreateGrayImagePdf());
            Assert.Throws<PdfluentException>(() => doc.PageImages(1));
        }

        [Fact]
        public void ExtractImageDataReturnsPngBytesWithHeader()
        {
            using var doc = PdfDocument.Open(CreateGrayImagePdf());
            uint id = ListingId(doc.PageImages(0));
            ExtractedImage image = doc.ExtractImageData(id);
            Assert.Equal(2, image.Width);
            Assert.Equal(2, image.Height);
            Assert.Equal(8, image.BitsPerComponent);
            Assert.Equal(1, image.Components);
            Assert.Equal("DeviceGray", image.ColorSpace);
            Assert.Equal("png", image.Encoding);
            Assert.False(image.Masked);
            Assert.True(image.Data.Length > 4);
            Assert.Equal(0x89, image.Data[0]);
            Assert.Equal((byte)'P', image.Data[1]);
            Assert.Equal((byte)'N', image.Data[2]);
            Assert.Equal((byte)'G', image.Data[3]);
        }

        [Fact]
        public void ExtractImageDataRejectsAnUnknownId()
        {
            using var doc = PdfDocument.Open(CreateGrayImagePdf());
            Assert.Throws<PdfluentException>(() => doc.ExtractImageData(9999));
        }

        [Fact]
        public void DecodeImagePixelsReturnsTheFixtureSamples()
        {
            using var doc = PdfDocument.Open(CreateGrayImagePdf());
            uint id = ListingId(doc.PageImages(0));
            DecodedImage image = doc.DecodeImagePixels(id);
            Assert.Equal(2, image.Width);
            Assert.Equal(2, image.Height);
            Assert.Equal(1, image.Components);
            Assert.Equal("DeviceGray", image.ColorSpace);
            Assert.Equal(GraySamples, image.Pixels);
        }

        [Fact]
        public void DecodeImagePixelsRejectsAnUnknownId()
        {
            using var doc = PdfDocument.Open(CreateGrayImagePdf());
            Assert.Throws<PdfluentException>(() => doc.DecodeImagePixels(9999));
        }
    }
}
