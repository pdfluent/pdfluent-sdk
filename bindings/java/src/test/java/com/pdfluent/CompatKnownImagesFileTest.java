// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

import org.junit.jupiter.api.Test;

import java.io.ByteArrayInputStream;
import java.awt.image.BufferedImage;
import java.awt.image.Raster;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

import javax.imageio.ImageIO;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Compat known-images as encoded files, decoded independently (#636).
 *
 * <p>{@link Document#extractImage} over {@code fixtures/known-images.pdf}
 * returns the image FILE (header facts plus encoded bytes), checked against
 * {@code fixtures/known-images.expected.json}: {@code ext} matches the
 * payload's magic bytes, and the payload decoded with {@code ImageIO} (the
 * JDK, not PDFluent) yields the expected dimensions, components, and
 * decoded-pixel sha256. Samples come from {@link Raster#getSamples} per
 * band, never through ARGB ints, so no colour conversion intervenes. A
 * lossy (JPEG) payload cannot match a pixel sha by construction, so it pins
 * only magic bytes, dimensions, and components.
 *
 * <p>Requires the native library, like {@link KnownImagesTest}:
 *
 * <pre>{@code
 * cargo build -p pdf-capi --release
 * export PDFLUENT_CAPI_LIB=$PWD/target/release/libpdf_capi.dylib
 * mvn test -f bindings/java/pom.xml -Dtest=CompatKnownImagesFileTest
 * }</pre>
 */
class CompatKnownImagesFileTest {

    private static final byte[] PNG_MAGIC =
        {(byte) 0x89, 'P', 'N', 'G', '\r', '\n', 0x1A, '\n'};
    private static final byte[] JPEG_MAGIC = {(byte) 0xFF, (byte) 0xD8, (byte) 0xFF};

    private static final class Want {
        final int width;
        final int height;
        final String colorspace;
        final int components;
        final int bpc;
        final String sha256;

        Want(int width, int height, String colorspace, int components, int bpc, String sha256) {
            this.width = width;
            this.height = height;
            this.colorspace = colorspace;
            this.components = components;
            this.bpc = bpc;
            this.sha256 = sha256;
        }
    }

    private static Path fixture(String name) {
        Path here = Paths.get("").toAbsolutePath();
        for (Path at = here; at != null; at = at.getParent()) {
            Path candidate = at.resolve("fixtures").resolve(name);
            if (Files.exists(candidate)) {
                return candidate;
            }
        }
        throw new IllegalStateException("fixtures/" + name + " not found above " + here);
    }

    private static List<Want> expected() throws Exception {
        String json = Files.readString(fixture("known-images.expected.json"));
        Pattern obj = Pattern.compile(
            "\\{[^}]*?\"bpc\"\\s*:\\s*(\\d+)[^}]*?\"colorspace\"\\s*:\\s*\"([^\"]+)\""
            + "[^}]*?\"components\"\\s*:\\s*(\\d+)[^}]*?\"height\"\\s*:\\s*(\\d+)"
            + "[^}]*?\"sha256\"\\s*:\\s*\"([^\"]+)\"[^}]*?\"width\"\\s*:\\s*(\\d+)[^}]*?\\}",
            Pattern.DOTALL);
        Matcher m = obj.matcher(json);
        List<Want> out = new ArrayList<>();
        while (m.find()) {
            out.add(new Want(
                Integer.parseInt(m.group(6)),
                Integer.parseInt(m.group(4)),
                m.group(2),
                Integer.parseInt(m.group(3)),
                Integer.parseInt(m.group(1)),
                m.group(5)));
        }
        assertFalse(out.isEmpty(), "no images parsed from expected.json");
        assertEquals(2, out.size());
        out.sort(Comparator.comparingInt((Want w) -> w.width).thenComparingInt(w -> w.height));
        return out;
    }

    private static String norm(String name) {
        String t = name.toLowerCase().replace("device", "").replace("grey", "gray");
        StringBuilder kept = new StringBuilder();
        for (int i = 0; i < t.length(); i++) {
            char c = t.charAt(i);
            if ((c >= 'a' && c <= 'z') || (c >= '0' && c <= '9')) {
                kept.append(c);
            }
        }
        return kept.toString();
    }

    private static String hex(byte[] digest) {
        StringBuilder out = new StringBuilder(digest.length * 2);
        for (byte b : digest) {
            out.append(Character.forDigit((b >> 4) & 0xF, 16));
            out.append(Character.forDigit(b & 0xF, 16));
        }
        return out.toString();
    }

    private static boolean startsWith(byte[] data, byte[] magic) {
        if (data.length < magic.length) {
            return false;
        }
        for (int i = 0; i < magic.length; i++) {
            if (data[i] != magic[i]) {
                return false;
            }
        }
        return true;
    }

    @Test
    void extractImageReturnsTheExpectedEncodedFile() throws Exception {
        List<Want> want = expected();
        try (Document doc = new Document(fixture("known-images.pdf").toString())) {
            List<ImageRef> refs = doc.getPageImages(0);
            assertEquals(2, refs.size());
            for (ImageRef r : refs) {
                Want w = null;
                for (Want c : want) {
                    if (c.width == r.getWidth() && c.height == r.getHeight()) {
                        w = c;
                    }
                }
                assertNotNull(w, "unexpected size " + r.getWidth() + "x" + r.getHeight());
                ImageData data = doc.extractImage(r.getXref());
                String ext = data.getExt();
                assertTrue(ext.equals("png") || ext.equals("jpeg"),
                    "ext is a file kind, got '" + ext + "'");
                assertEquals(w.width, data.getWidth());
                assertEquals(w.height, data.getHeight());
                assertEquals(w.components, data.getColorspace());
                assertEquals(norm(w.colorspace), norm(data.getColorspaceName()));
                assertEquals(w.bpc, data.getBits());
                byte[] file = data.getData();
                if (ext.equals("png")) {
                    assertTrue(startsWith(file, PNG_MAGIC), "png ext matches PNG magic");
                    BufferedImage img = ImageIO.read(new ByteArrayInputStream(file));
                    assertNotNull(img, "ImageIO decodes the payload");
                    assertEquals(w.width, img.getWidth());
                    assertEquals(w.height, img.getHeight());
                    Raster raster = img.getRaster();
                    assertEquals(w.components, raster.getNumBands());
                    int[] row = new int[w.width];
                    byte[] samples = new byte[w.width * w.height * w.components];
                    for (int y = 0; y < w.height; y++) {
                        for (int b = 0; b < w.components; b++) {
                            raster.getSamples(0, y, w.width, 1, b, row);
                            for (int x = 0; x < w.width; x++) {
                                samples[(y * w.width + x) * w.components + b] = (byte) row[x];
                            }
                        }
                    }
                    MessageDigest sha = MessageDigest.getInstance("SHA-256");
                    assertEquals(w.sha256, hex(sha.digest(samples)));
                } else {
                    assertTrue(startsWith(file, JPEG_MAGIC), "jpeg ext matches JPEG magic");
                    BufferedImage img = ImageIO.read(new ByteArrayInputStream(file));
                    assertNotNull(img, "ImageIO decodes the payload");
                    assertEquals(w.width, img.getWidth());
                    assertEquals(w.height, img.getHeight());
                }
            }
        }
    }
}
