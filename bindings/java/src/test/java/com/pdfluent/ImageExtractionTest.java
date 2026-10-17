// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.
package com.pdfluent;

import org.junit.jupiter.api.Test;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;

import static org.junit.jupiter.api.Assertions.*;

/**
 * JUnit 5 tests for the plain embedded-image API on {@link PdfluentDocument}.
 *
 * <p>{@link PdfluentDocument#pageImages(int)} lists a page's stored images,
 * {@link PdfluentDocument#extractImageData(int)} returns one image's encoded
 * bytes with header facts, and {@link PdfluentDocument#decodeImagePixels(int)}
 * decodes to raw samples. The fixture is built in the test: one page holding
 * a raw 2x2 grey image, so the known pixels below are the samples the builder
 * wrote.
 *
 * <p>Requires the native library, like {@link PdfluentDocumentTest}:
 *
 * <pre>{@code
 * cargo build -p pdf-java --release
 * export PDFLUENT_NATIVE_LIB=target/release/libpdfluent_java.dylib
 * mvn test -f bindings/java/pom.xml -Dtest=ImageExtractionTest
 * }</pre>
 */
class ImageExtractionTest {

    private static final byte[] GRAY_SAMPLES = {0, (byte) 128, (byte) 192, (byte) 255};

    /**
     * One page with a raw 2x2 DeviceGray image under {@code /G0}.
     */
    private static byte[] grayImagePdf() {
        try {
            byte[] content = {'q', '\n', 'Q', '\n'};
            List<byte[]> objects = new ArrayList<>();
            objects.add("<< /Type /Catalog /Pages 2 0 R >>".getBytes(StandardCharsets.US_ASCII));
            objects.add("<< /Type /Pages /Kids [3 0 R] /Count 1 >>".getBytes(StandardCharsets.US_ASCII));
            objects.add(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources 5 0 R >>"
                    .getBytes(StandardCharsets.US_ASCII));
            ByteArrayOutputStream stream = new ByteArrayOutputStream();
            stream.write(
                ("<< /Length " + content.length + " >>\nstream\n").getBytes(StandardCharsets.US_ASCII));
            stream.write(content);
            stream.write("\nendstream".getBytes(StandardCharsets.US_ASCII));
            objects.add(stream.toByteArray());
            objects.add("<< /XObject << /G0 6 0 R >> >>".getBytes(StandardCharsets.US_ASCII));
            ByteArrayOutputStream image = new ByteArrayOutputStream();
            image.write(
                ("<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceGray"
                        + " /BitsPerComponent 8 /Length "
                        + GRAY_SAMPLES.length
                        + " >>\nstream\n")
                    .getBytes(StandardCharsets.US_ASCII));
            image.write(GRAY_SAMPLES);
            image.write("\nendstream".getBytes(StandardCharsets.US_ASCII));
            objects.add(image.toByteArray());

            ByteArrayOutputStream pdf = new ByteArrayOutputStream();
            pdf.write("%PDF-1.7\n".getBytes(StandardCharsets.US_ASCII));
            List<Integer> offsets = new ArrayList<>();
            for (int i = 0; i < objects.size(); i++) {
                offsets.add(pdf.size());
                pdf.write(((i + 1) + " 0 obj\n").getBytes(StandardCharsets.US_ASCII));
                pdf.write(objects.get(i));
                pdf.write("\nendobj\n".getBytes(StandardCharsets.US_ASCII));
            }
            int xrefAt = pdf.size();
            pdf.write(("xref\n0 " + (objects.size() + 1) + "\n").getBytes(StandardCharsets.US_ASCII));
            pdf.write("0000000000 65535 f \n".getBytes(StandardCharsets.US_ASCII));
            for (int offset : offsets) {
                pdf.write(String.format("%010d 00000 n \n", offset).getBytes(StandardCharsets.US_ASCII));
            }
            pdf.write(
                ("trailer\n<< /Size "
                        + (objects.size() + 1)
                        + " /Root 1 0 R >>\nstartxref\n"
                        + xrefAt
                        + "\n%%EOF\n")
                    .getBytes(StandardCharsets.US_ASCII));
            return pdf.toByteArray();
        } catch (IOException e) {
            throw new AssertionError("fixture builder writes to memory", e);
        }
    }

    @Test
    void pageImagesListsTheStoredImage() {
        try (PdfluentDocument doc = PdfluentDocument.open(grayImagePdf())) {
            List<ImageInfo> images = doc.pageImages(0);
            assertEquals(1, images.size());
            ImageInfo image = images.get(0);
            assertEquals(2, image.getWidth());
            assertEquals(2, image.getHeight());
            assertEquals(8, image.getBitsPerComponent());
            assertEquals(1, image.getComponents());
            assertEquals("DeviceGray", image.getColorSpace());
            assertEquals("G0", image.getName());
            assertFalse(image.hasMask());
        }
    }

    @Test
    void pageImagesPastTheLastPageThrowsPageRange() {
        try (PdfluentDocument doc = PdfluentDocument.open(grayImagePdf())) {
            assertThrows(PdfluentPageRangeException.class, () -> doc.pageImages(1));
        }
    }

    @Test
    void extractImageDataReturnsPngBytesWithHeader() {
        try (PdfluentDocument doc = PdfluentDocument.open(grayImagePdf())) {
            int id = doc.pageImages(0).get(0).getId();
            ExtractedImage image = doc.extractImageData(id);
            assertEquals(2, image.getWidth());
            assertEquals(2, image.getHeight());
            assertEquals("png", image.getEncoding());
            assertFalse(image.isMasked());
            byte[] data = image.getData();
            assertTrue(data.length > 4);
            assertEquals((byte) 0x89, data[0]);
            assertEquals((byte) 'P', data[1]);
            assertEquals((byte) 'N', data[2]);
            assertEquals((byte) 'G', data[3]);
        }
    }

    @Test
    void extractImageDataRejectsAnUnknownId() {
        try (PdfluentDocument doc = PdfluentDocument.open(grayImagePdf())) {
            assertThrows(PdfluentException.class, () -> doc.extractImageData(9999));
        }
    }

    @Test
    void decodeImagePixelsReturnsTheFixtureSamples() {
        try (PdfluentDocument doc = PdfluentDocument.open(grayImagePdf())) {
            int id = doc.pageImages(0).get(0).getId();
            DecodedImage image = doc.decodeImagePixels(id);
            assertEquals(2, image.getWidth());
            assertEquals(2, image.getHeight());
            assertEquals(1, image.getComponents());
            assertEquals("DeviceGray", image.getColorSpace());
            assertArrayEquals(GRAY_SAMPLES, image.getPixels());
        }
    }

    @Test
    void decodeImagePixelsRejectsAnUnknownId() {
        try (PdfluentDocument doc = PdfluentDocument.open(grayImagePdf())) {
            assertThrows(PdfluentException.class, () -> doc.decodeImagePixels(9999));
        }
    }
}
