package com.pdfluent;

import java.awt.image.BufferedImage;

/**
 * An RGBA pixel buffer produced by rendering a PDF page.
 *
 * <p>Instances are returned by {@link PdfluentDocument#renderPage(int, double)}
 * and {@link PdfluentDocument#renderThumbnail(int, int)}.
 *
 * <h2>Pixel format</h2>
 * <p>Pixels are stored in row-major order as 4 bytes per pixel: {@code R G B A},
 * where each component is in the range {@code [0, 255]}. The total byte count is
 * {@code width * height * 4}.
 *
 * <h2>Thread safety</h2>
 * <p>{@code RenderedImage} is effectively immutable. The constructor is called
 * only from within the native loading path; the {@code pixels} array is never
 * shared externally before the object is published. Therefore this class is
 * thread-safe after construction — it can be passed to any thread without
 * additional synchronisation.
 *
 * <p>Note: {@link #getPixels()} returns a reference to the internal array.
 * Callers that mutate the returned array break the immutability contract.
 *
 * <h2>AutoCloseable</h2>
 * <p>{@code RenderedImage} does not hold any native resources and does not
 * implement {@link AutoCloseable}. Memory is managed entirely by the JVM GC.
 */
public class RenderedImage {

    private final int width;
    private final int height;
    private final byte[] pixels;

    /**
     * Package-private constructor — instances are created by {@link PdfluentDocument}.
     *
     * @param width  image width in pixels; must be {@code > 0}
     * @param height image height in pixels; must be {@code > 0}
     * @param pixels RGBA pixel data; must have length {@code width * height * 4}
     */
    RenderedImage(int width, int height, byte[] pixels) {
        this.width = width;
        this.height = height;
        this.pixels = pixels;
    }

    /**
     * Returns the image width in pixels.
     *
     * @return width; always {@code > 0}
     */
    public int getWidth() {
        return width;
    }

    /**
     * Returns the image height in pixels.
     *
     * @return height; always {@code > 0}
     */
    public int getHeight() {
        return height;
    }

    /**
     * Returns the raw RGBA pixel data.
     *
     * <p>The array has length {@code getWidth() * getHeight() * 4}. Bytes are
     * laid out in row-major order: pixel {@code (x, y)} starts at byte offset
     * {@code (y * width + x) * 4}. The four bytes at that offset are
     * {@code R}, {@code G}, {@code B}, {@code A} respectively.
     *
     * <p>The returned reference points to the internal array. Mutating it
     * has undefined behaviour with respect to future use of this object.
     *
     * @return RGBA pixel data
     */
    public byte[] getPixels() {
        return pixels;
    }

    /**
     * Convert to a {@link BufferedImage} with {@code TYPE_INT_ARGB} format.
     *
     * <p><strong>Note:</strong> {@link java.awt} is not available on Android.
     * Use {@link #getPixels()} directly when targeting Android.
     *
     * @return a new {@link BufferedImage} backed by a copy of the RGBA pixel data
     */
    public BufferedImage toBufferedImage() {
        BufferedImage img = new BufferedImage(width, height, BufferedImage.TYPE_INT_ARGB);
        for (int y = 0; y < height; y++) {
            for (int x = 0; x < width; x++) {
                int offset = (y * width + x) * 4;
                int r = pixels[offset]     & 0xFF;
                int g = pixels[offset + 1] & 0xFF;
                int b = pixels[offset + 2] & 0xFF;
                int a = pixels[offset + 3] & 0xFF;
                img.setRGB(x, y, (a << 24) | (r << 16) | (g << 8) | b);
            }
        }
        return img;
    }
}
