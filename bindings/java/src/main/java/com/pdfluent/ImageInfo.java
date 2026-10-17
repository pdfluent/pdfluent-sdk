// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.
package com.pdfluent;

/**
 * One image stored on a page, as {@link PdfluentDocument#pageImages(int)} lists it.
 *
 * <p>Instances are immutable value objects created by {@code PdfluentDocument}.
 */
public class ImageInfo {

    private final int id;
    private final int width;
    private final int height;
    private final int bitsPerComponent;
    private final int components;
    private final String colorSpace;
    private final String colorSpaceName;
    private final String filter;
    private final String name;
    private final boolean hasMask;

    /**
     * Package-private constructor — instances are created by {@link PdfluentDocument}.
     */
    ImageInfo(
            int id,
            int width,
            int height,
            int bitsPerComponent,
            int components,
            String colorSpace,
            String colorSpaceName,
            String filter,
            String name,
            boolean hasMask) {
        this.id = id;
        this.width = width;
        this.height = height;
        this.bitsPerComponent = bitsPerComponent;
        this.components = components;
        this.colorSpace = colorSpace;
        this.colorSpaceName = colorSpaceName;
        this.filter = filter;
        this.name = name;
        this.hasMask = hasMask;
    }

    /**
     * Returns the handle for {@link PdfluentDocument#extractImageData(int)} and
     * {@link PdfluentDocument#decodeImagePixels(int)}.
     *
     * @return image handle
     */
    public int getId() {
        return id;
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
     * Returns the first filter name, normalised, or empty for raw samples.
     *
     * @return filter name
     */
    public String getFilter() {
        return filter;
    }

    /**
     * Returns the resource name the page lists the image under.
     *
     * @return resource name
     */
    public String getName() {
        return name;
    }

    /**
     * Returns whether the image carries a soft mask.
     *
     * @return {@code true} when masked
     */
    public boolean hasMask() {
        return hasMask;
    }
}
