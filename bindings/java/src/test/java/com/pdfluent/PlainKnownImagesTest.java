// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

package com.pdfluent;

import org.junit.jupiter.api.Test;

import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Plain known-images on the shared construction fixture (#643).
 *
 * <p>{@link PdfluentDocument#pageImages(int)} plus
 * {@link PdfluentDocument#decodeImagePixels(int)} over
 * {@code fixtures/known-images.pdf}, checked against
 * {@code fixtures/known-images.expected.json}: count, dimensions, colour
 * space, components, bpc, and the decoded-pixel sha256.
 * {@link PdfluentDocument#extractImageData(int)} rides along to pin the
 * encoded-vs-pixels split (PNG bytes, not samples).
 */
class PlainKnownImagesTest {

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

    @Test
    void pageImagesListsTheSharedFixture() throws Exception {
        List<Want> want = expected();
        try (PdfluentDocument doc = PdfluentDocument.open(fixture("known-images.pdf"))) {
            List<ImageInfo> images = new ArrayList<>(doc.pageImages(0));
            assertEquals(want.size(), images.size());
            assertEquals(2, images.size());
            images.sort(Comparator.comparingInt(ImageInfo::getWidth).thenComparingInt(ImageInfo::getHeight));
            for (int i = 0; i < want.size(); i++) {
                Want w = want.get(i);
                ImageInfo info = images.get(i);
                assertEquals(w.width, info.getWidth());
                assertEquals(w.height, info.getHeight());
                assertEquals(norm(w.colorspace), norm(info.getColorSpace()));
                assertEquals(w.components, info.getComponents());
                assertEquals(w.bpc, info.getBitsPerComponent());
            }
        }
    }

    @Test
    void decodeImagePixelsMatchesTheExpectedSha() throws Exception {
        List<Want> want = expected();
        try (PdfluentDocument doc = PdfluentDocument.open(fixture("known-images.pdf"))) {
            List<ImageInfo> images = doc.pageImages(0);
            assertEquals(2, images.size());
            for (ImageInfo info : images) {
                Want w = null;
                for (Want c : want) {
                    if (c.width == info.getWidth() && c.height == info.getHeight()) {
                        w = c;
                    }
                }
                assertNotNull(w, "unexpected size " + info.getWidth() + "x" + info.getHeight());
                DecodedImage decoded = doc.decodeImagePixels(info.getId());
                assertEquals(w.width, decoded.getWidth());
                assertEquals(w.height, decoded.getHeight());
                assertEquals(w.components, decoded.getComponents());
                assertEquals(norm(w.colorspace), norm(decoded.getColorSpace()));
                byte[] pixels = decoded.getPixels();
                assertEquals(w.width * w.height * w.components, pixels.length);
                MessageDigest sha = MessageDigest.getInstance("SHA-256");
                assertEquals(w.sha256, hex(sha.digest(pixels)));
            }
        }
    }

    @Test
    void extractImageDataIsPngNotPixels() throws Exception {
        try (PdfluentDocument doc = PdfluentDocument.open(fixture("known-images.pdf"))) {
            List<ImageInfo> images = doc.pageImages(0);
            assertEquals(2, images.size());
            for (ImageInfo info : images) {
                ExtractedImage extracted = doc.extractImageData(info.getId());
                assertEquals("png", extracted.getEncoding());
                byte[] data = extracted.getData();
                assertTrue(data.length > 4);
                assertEquals((byte) 0x89, data[0]);
                assertEquals((byte) 'P', data[1]);
                assertEquals((byte) 'N', data[2]);
                assertEquals((byte) 'G', data[3]);
                DecodedImage decoded = doc.decodeImagePixels(info.getId());
                assertNotEquals(data.length, decoded.getPixels().length);
            }
        }
    }
}
