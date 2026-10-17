// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.
package com.pdfluent;

/**
 * One extracted image: header facts plus encoded bytes.
 *
 * <p>JPEG payloads pass through untouched; everything else arrives decoded
 * and re-encoded as PNG. Instances are immutable value objects created by
 * {@link PdfluentDocument#extractImageData(int)}.
 */
public class ExtractedImage {

    private final int width;
    private final int height;
    private final int bitsPerComponent;
    private final int components;
    private final String colorSpace;
    private final String colorSpaceName;
    private final String encoding;
    private final byte[] data;
    private final boolean masked;
    private final int xres;
    private final int yres;

    /**
     * Package-private constructor — instances are created by {@link PdfluentDocument}.
     */
    ExtractedImage(
            int width,
            int height,
            int bitsPerComponent,
            int components,
            String colorSpace,
            String colorSpaceName,
            String encoding,
            byte[] data,
            boolean masked,
            int xres,
            int yres) {
        this.width = width;
        this.height = height;
        this.bitsPerComponent = bitsPerComponent;
        this.components = components;
        this.colorSpace = colorSpace;
        this.colorSpaceName = colorSpaceName;
        this.encoding = encoding;
        this.data = data;
        this.masked = masked;
        this.xres = xres;
        this.yres = yres;
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
     * Returns the bits per component.
     *
     * @return bits per component
     */
    public int getBitsPerComponent() {
        return bitsPerComponent;
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
     * Returns the colour-space name recorded in the image dictionary.
     *
     * @return recorded colour-space name
     */
    public String getColorSpaceName() {
        return colorSpaceName;
    }

    /**
     * Returns the encoding of {@link #getData()}: {@code "jpeg"} or {@code "png"}.
     *
     * @return encoding name
     */
    public String getEncoding() {
        return encoding;
    }

    /**
     * Returns the encoded image bytes.
     *
     * <p>The returned reference points to the internal array. Mutating it
     * has undefined behaviour with respect to future use of this object.
     *
     * @return encoded image bytes
     */
    public byte[] getData() {
        return data;
    }

    /**
     * Returns whether the image is masked.
     *
     * @return {@code true} when masked
     */
    public boolean isMasked() {
        return masked;
    }

    /**
     * Returns the horizontal resolution in dots per inch.
     *
     * @return horizontal resolution
     */
    public int getXres() {
        return xres;
    }

    /**
     * Returns the vertical resolution in dots per inch.
     *
     * @return vertical resolution
     */
    public int getYres() {
        return yres;
    }
}
