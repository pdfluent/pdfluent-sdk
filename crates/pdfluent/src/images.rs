//! Embedded-image extraction on the plain facade.
//!
//! Listing ([`PdfDocument::page_images`]), encoded extraction
//! ([`PdfDocument::extract_image_data`]) and pixel decoding
//! ([`PdfDocument::decode_image_pixels`]) over the document's stored images.
//! All three delegate to the shared image core in
//! [`crate::compat::pixmap`]: one implementation, two surfaces. The plain
//! spelling differs only at the boundary — 1-based pages with
//! [`crate::Error::PageOutOfRange`], unsigned sizes, owned view structs —
//! and introduces no new error variants.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::compat::pixmap::{self, Colorspace as CompatColorspace};
use crate::document::PdfDocument;
use crate::error::{Error, Result};

/// Handle of one stored image: its object number in the document.
///
/// Stable within the document; listed by [`PdfDocument::page_images`] and
/// consumed by [`PdfDocument::extract_image_data`] and
/// [`PdfDocument::decode_image_pixels`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageId(u32);

impl ImageId {
    /// The underlying object number.
    pub fn as_u32(self) -> u32 {
        self.0
    }
}

impl From<u32> for ImageId {
    /// Wrap a raw object number.
    fn from(id: u32) -> Self {
        Self(id)
    }
}

/// Decoded pixel layout of a stored image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImageColorSpace {
    /// One grey component.
    Gray,
    /// Red, green, and blue components.
    Rgb,
    /// Cyan, magenta, yellow, and black components.
    Cmyk,
}

impl ImageColorSpace {
    /// Recorded colour-space name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Gray => "DeviceGray",
            Self::Rgb => "DeviceRGB",
            Self::Cmyk => "DeviceCMYK",
        }
    }

    /// Components per pixel.
    pub fn components(self) -> u8 {
        match self {
            Self::Gray => 1,
            Self::Rgb => 3,
            Self::Cmyk => 4,
        }
    }
}

/// Classify a component count the way the shared decode core does: 1 reads
/// grey, 4 reads CMYK, and anything else reads RGB.
fn color_space_from_comps(comps: i32) -> ImageColorSpace {
    match comps {
        1 => ImageColorSpace::Gray,
        4 => ImageColorSpace::Cmyk,
        _ => ImageColorSpace::Rgb,
    }
}

impl From<CompatColorspace> for ImageColorSpace {
    /// Map the shared core's colour space onto the plain spelling.
    fn from(colorspace: CompatColorspace) -> Self {
        match colorspace {
            CompatColorspace::Gray => Self::Gray,
            CompatColorspace::Rgb => Self::Rgb,
            CompatColorspace::Cmyk => Self::Cmyk,
        }
    }
}

/// Read-only view of one image stored on a page.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ImageInfo {
    /// Handle for extraction and decoding.
    pub id: ImageId,
    /// Image width, in pixels.
    pub width: u32,
    /// Image height, in pixels.
    pub height: u32,
    /// Bits per component.
    pub bits_per_component: u8,
    /// Components per pixel.
    pub components: u8,
    /// Decoded pixel layout.
    pub color_space: ImageColorSpace,
    /// Colour-space name recorded in the image dictionary.
    pub color_space_name: String,
    /// First filter name, normalised, or empty for raw samples.
    pub filter: String,
    /// Resource name the page lists the image under.
    pub name: String,
    /// Whether the image carries a soft mask.
    pub has_mask: bool,
}

/// Read-only view of one extracted image: header facts plus encoded bytes.
///
/// JPEG payloads pass through untouched; everything else arrives decoded
/// and re-encoded as PNG, preserving grey and stencil bit depth.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ExtractedImage {
    /// Image width, in pixels.
    pub width: u32,
    /// Image height, in pixels.
    pub height: u32,
    /// Bits per component.
    pub bits_per_component: u8,
    /// Components per pixel.
    pub components: u8,
    /// Decoded pixel layout.
    pub color_space: ImageColorSpace,
    /// Colour-space name recorded in the image dictionary.
    pub color_space_name: String,
    /// Encoding of [`data`](Self::data): `"jpeg"` or `"png"`.
    pub encoding: String,
    /// Encoded image bytes.
    pub data: Vec<u8>,
    /// Whether the image is masked.
    pub masked: bool,
    /// Horizontal resolution, in dots per inch.
    pub xres: u32,
    /// Vertical resolution, in dots per inch.
    pub yres: u32,
}

/// Read-only view of one decoded image: raw samples, top row first.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DecodedImage {
    /// Image width, in pixels.
    pub width: u32,
    /// Image height, in pixels.
    pub height: u32,
    /// Components per pixel.
    pub components: u8,
    /// Decoded pixel layout.
    pub color_space: ImageColorSpace,
    /// Raw samples, [`components`](Self::components) bytes per pixel.
    pub pixels: Vec<u8>,
}

/// Clamp a recorded signed size into the unsigned facade shape.
fn clamp_u32(value: i32) -> u32 {
    u32::try_from(value).unwrap_or(0)
}

/// Clamp a recorded signed count into the unsigned facade shape.
fn clamp_u8(value: i32) -> u8 {
    u8::try_from(value).unwrap_or(0)
}

impl PdfDocument {
    /// List the images stored on a page (1-based).
    ///
    /// One [`ImageInfo`] per image XObject in the page's resources, in
    /// resource order. Form XObjects and other non-images are skipped.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pdfluent::PdfDocument;
    ///
    /// let doc = PdfDocument::open("report.pdf").unwrap();
    /// for image in doc.page_images(1).unwrap() {
    ///     println!("{}: {}x{}", image.name, image.width, image.height);
    /// }
    /// ```
    pub fn page_images(&self, page: usize) -> Result<Vec<ImageInfo>> {
        let total = self.engine().page_count();
        if page == 0 || page > total {
            return Err(Error::PageOutOfRange {
                requested: page,
                page_count: total,
            });
        }
        let id = pixmap::page_object_id(self.lopdf(), page - 1)?;
        let mut out = Vec::new();
        for row in pixmap::page_image_facts_in(self.lopdf(), id)? {
            let image = row.image;
            out.push(ImageInfo {
                id: ImageId(clamp_u32(image.xref)),
                width: clamp_u32(image.width),
                height: clamp_u32(image.height),
                bits_per_component: clamp_u8(image.bits),
                components: clamp_u8(row.comps),
                color_space: color_space_from_comps(row.comps),
                color_space_name: image.colorspace_name,
                filter: image.filter,
                name: image.name,
                has_mask: image.mask != 0,
            });
        }
        Ok(out)
    }

    /// Extract one stored image: header facts plus encoded bytes.
    ///
    /// Unknown ids and non-image objects surface the shared core's
    /// bad-reference error; no new error variant is introduced.
    pub fn extract_image_data(&self, id: ImageId) -> Result<ExtractedImage> {
        let xref = i32::try_from(id.as_u32()).map_err(|_| Error::InvalidPdf {
            byte_offset: None,
            reason: format!("image id {} is not an object number", id.as_u32()),
        })?;
        let data = pixmap::extract_image_in(self.lopdf(), xref)?;
        Ok(ExtractedImage {
            width: clamp_u32(data.width),
            height: clamp_u32(data.height),
            bits_per_component: clamp_u8(data.bits),
            components: clamp_u8(data.colorspace),
            color_space: color_space_from_comps(data.colorspace),
            color_space_name: data.colorspace_name,
            encoding: data.ext,
            data: data.data,
            masked: data.masked,
            xres: clamp_u32(data.xres),
            yres: clamp_u32(data.yres),
        })
    }

    /// Decode one stored image to raw samples, expanding palettes and stencils.
    ///
    /// Unknown ids and non-image objects surface the shared core's
    /// bad-reference error; no new error variant is introduced.
    pub fn decode_image_pixels(&self, id: ImageId) -> Result<DecodedImage> {
        let xref = i32::try_from(id.as_u32()).map_err(|_| Error::InvalidPdf {
            byte_offset: None,
            reason: format!("image id {} is not an object number", id.as_u32()),
        })?;
        let decoded = pixmap::pixmap_from_image_in(self.lopdf(), xref)?;
        let color_space = ImageColorSpace::from(decoded.colorspace());
        Ok(DecodedImage {
            width: clamp_u32(decoded.width()),
            height: clamp_u32(decoded.height()),
            components: color_space.components(),
            color_space,
            pixels: decoded.samples().to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use lopdf::{dictionary, Document, Object, Stream};

    use super::*;

    /// Grey samples of the fixture's `G0` image: 2x2, 8-bit, raw.
    const GRAY_SAMPLES: [u8; 4] = [0, 128, 192, 255];

    /// RGB samples of the fixture's `R0` image: 2x2, 8-bit, raw.
    const RGB_SAMPLES: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];

    /// One page with two raw images (`G0` grey, `R0` RGB), plus their ids.
    fn fixture() -> (PdfDocument, ImageId, ImageId) {
        let mut doc = Document::with_version("1.7");
        let gray_id = doc.add_object(Object::Stream(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => 2,
                "Height" => 2,
                "ColorSpace" => "DeviceGray",
                "BitsPerComponent" => 8,
            },
            GRAY_SAMPLES.to_vec(),
        )));
        let rgb_id = doc.add_object(Object::Stream(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => 2,
                "Height" => 2,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
            },
            RGB_SAMPLES.to_vec(),
        )));
        let resources_id = doc.add_object(dictionary! {
            "XObject" => dictionary! { "G0" => gray_id, "R0" => rgb_id },
        });
        let content_id = doc.add_object(Stream::new(dictionary! {}, b"q\nQ\n".to_vec()));
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => resources_id,
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page_id)],
                "Count" => 1,
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        let gray = ImageId(gray_id.0);
        let rgb = ImageId(rgb_id.0);
        let doc = PdfDocument::from_lopdf(doc).expect("fixture opens");
        (doc, gray, rgb)
    }

    #[test]
    fn page_images_lists_both_fixture_images() {
        let (doc, gray_id, rgb_id) = fixture();
        let images = doc.page_images(1).expect("list");
        assert_eq!(images.len(), 2);
        let gray = images.iter().find(|i| i.name == "G0").expect("grey row");
        assert_eq!(gray.id, gray_id);
        assert_eq!((gray.width, gray.height), (2, 2));
        assert_eq!(gray.bits_per_component, 8);
        assert_eq!(gray.components, 1);
        assert_eq!(gray.color_space, ImageColorSpace::Gray);
        assert!(!gray.has_mask);
        let rgb = images.iter().find(|i| i.name == "R0").expect("rgb row");
        assert_eq!(rgb.id, rgb_id);
        assert_eq!((rgb.width, rgb.height), (2, 2));
        assert_eq!(rgb.components, 3);
        assert_eq!(rgb.color_space, ImageColorSpace::Rgb);
    }

    #[test]
    fn page_images_rejects_out_of_range_pages() {
        let (doc, _, _) = fixture();
        for bad in [0, 2] {
            match doc.page_images(bad) {
                Err(Error::PageOutOfRange {
                    requested,
                    page_count,
                }) => {
                    assert_eq!(requested, bad);
                    assert_eq!(page_count, 1);
                }
                other => panic!("page {bad} must be out of range, got {other:?}"),
            }
        }
    }

    #[test]
    fn extract_image_data_returns_png_for_raw_gray() {
        let (doc, gray_id, _) = fixture();
        let image = doc.extract_image_data(gray_id).expect("extract");
        assert_eq!((image.width, image.height), (2, 2));
        assert_eq!(image.bits_per_component, 8);
        assert_eq!(image.components, 1);
        assert_eq!(image.color_space, ImageColorSpace::Gray);
        assert_eq!(image.encoding, "png");
        assert!(!image.masked);
        assert!(image.data.starts_with(&[0x89, b'P', b'N', b'G']));
    }

    #[test]
    fn extract_image_data_rejects_unknown_id() {
        let (doc, _, _) = fixture();
        assert!(doc.extract_image_data(ImageId::from(9999)).is_err());
    }

    #[test]
    fn decode_image_pixels_returns_raw_samples() {
        let (doc, gray_id, rgb_id) = fixture();
        let gray = doc.decode_image_pixels(gray_id).expect("decode grey");
        assert_eq!((gray.width, gray.height), (2, 2));
        assert_eq!(gray.components, 1);
        assert_eq!(gray.color_space, ImageColorSpace::Gray);
        assert_eq!(gray.pixels, GRAY_SAMPLES);
        let rgb = doc.decode_image_pixels(rgb_id).expect("decode rgb");
        assert_eq!(rgb.components, 3);
        assert_eq!(rgb.color_space, ImageColorSpace::Rgb);
        assert_eq!(rgb.pixels, RGB_SAMPLES);
    }

    #[test]
    fn decode_image_pixels_rejects_unknown_id() {
        let (doc, _, _) = fixture();
        assert!(doc.decode_image_pixels(ImageId::from(9999)).is_err());
    }
}
