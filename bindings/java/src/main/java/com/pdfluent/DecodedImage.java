// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.
package com.pdfluent;

/**
 * One decoded image: raw samples, top row first.
 *
 * <p>Palettes and stencils are expanded. Instances are immutable value
 * objects created by {@link PdfluentDocument#decodeImagePixels(int)}.
 */
public class DecodedImage {

    private final int width;
    private final int height;
    private final int components;
    private final String colorSpace;
    private final byte[] pixels;

    /**
     * Package-private constructor — instances are created by {@link PdfluentDocument}.
     */
    DecodedImage(int width, int height, int components, String colorSpace, byte[] pixels) {
        this.width = width;
        this.height = height;
        this.components = components;
        this.colorSpace = colorSpace;
        this.pixels = pixels;
    }

    /**
     * Returns the image width in pixels.
     *
     * @return width
     */
    public int getWidth() {
        return width;
    }

    /**
     * Returns the image height in pixels.
     *
     * @return height
     */
    public int getHeight() {
        return height;
    }

    /**
     * Returns the components per pixel.
     *
     * @return components per pixel
     */
    public int getComponents() {
        return components;
    }

    /**
     * Returns the decoded pixel layout: {@code "DeviceGray"}, {@code "DeviceRGB"}
     * or {@code "DeviceCMYK"}.
     *
     * @return decoded pixel layout
     */
    public String getColorSpace() {
        return colorSpace;
    }

    /**
     * Returns the raw samples, {@link #getComponents()} bytes per pixel.
     *
     * <p>The returned reference points to the internal array. Mutating it
     * has undefined behaviour with respect to future use of this object.
     *
     * @return raw samples
     */
    public byte[] getPixels() {
        return pixels;
    }
}
