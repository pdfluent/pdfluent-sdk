package com.xfa.pdf;

import java.awt.image.BufferedImage;
import java.nio.ByteBuffer;

/**
 * A rendered page image with RGBA pixel data.
 */
public class RenderedImage {
    private final int width;
    private final int height;
    private final byte[] pixels;

    RenderedImage(int width, int height, byte[] pixels) {
        this.width = width;
        this.height = height;
        this.pixels = pixels;
    }

    /** Width in pixels. */
    public int getWidth() {
        return width;
    }

    /** Height in pixels. */
    public int getHeight() {
        return height;
    }

    /** Raw RGBA pixel data (4 bytes per pixel, row-major). */
    public byte[] getPixels() {
        return pixels;
    }

    /**
     * Convert to a BufferedImage (TYPE_INT_ARGB).
     * Requires java.awt — not available on Android.
     */
    public BufferedImage toBufferedImage() {
        BufferedImage img = new BufferedImage(width, height, BufferedImage.TYPE_INT_ARGB);
        for (int y = 0; y < height; y++) {
            for (int x = 0; x < width; x++) {
                int offset = (y * width + x) * 4;
                int r = pixels[offset] & 0xFF;
                int g = pixels[offset + 1] & 0xFF;
                int b = pixels[offset + 2] & 0xFF;
                int a = pixels[offset + 3] & 0xFF;
                int argb = (a << 24) | (r << 16) | (g << 8) | b;
                img.setRGB(x, y, argb);
            }
        }
        return img;
    }
}
