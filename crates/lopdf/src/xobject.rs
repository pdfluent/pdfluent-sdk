use crate::*;
use crate::{Dictionary, Stream};

#[cfg(feature = "embed_image")]
use image::{self, ColorType, ImageFormat};

#[cfg(feature = "embed_image")]
use std::path::Path;

#[cfg(feature = "embed_image")]
use crate::Result;

#[derive(Debug, Clone)]
pub struct PdfImage<'a> {
    pub id: ObjectId,
    pub width: i64,
    pub height: i64,
    pub color_space: Option<String>,
    pub filters: Option<Vec<String>>,
    pub bits_per_component: Option<i64>,
    /// Image Data
    pub content: &'a [u8],
    /// Origin Stream Dictionary
    pub origin_dict: &'a Dictionary,
}

pub fn form(boundingbox: Vec<f32>, matrix: Vec<f32>, content: Vec<u8>) -> Stream {
    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"XObject".to_vec()));
    dict.set("Subtype", Object::Name(b"Form".to_vec()));
    dict.set(
        "BBox",
        Object::Array(boundingbox.into_iter().map(Object::Real).collect()),
    );
    dict.set("Matrix", Object::Array(matrix.into_iter().map(Object::Real).collect()));
    let mut xobject = Stream::new(dict, content);
    // Ignore any compression error.
    let _ = xobject.compress();
    xobject
}

#[cfg(feature = "embed_image")]
pub fn image<P: AsRef<Path>>(path: P) -> Result<Stream> {
    use std::fs::File;
    use std::io::prelude::*;

    let mut file = File::open(&path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    image_from(buffer)
}

#[cfg(feature = "embed_image")]
pub fn image_from(buffer: Vec<u8>) -> Result<Stream> {
    let ((width, height), color_type) = get_dimensions_and_color_type(&buffer)?;

    let (bpc, color_space) = match color_type {
        // 8-bit per channel types
        ColorType::L8 => (8, b"DeviceGray".to_vec()),
        ColorType::La8 => (8, b"DeviceGray".to_vec()),
        ColorType::Rgb8 => (8, b"DeviceRGB".to_vec()),
        ColorType::Rgba8 => (8, b"DeviceRGB".to_vec()),
        // 16-bit per channel types
        ColorType::L16 => (16, b"DeviceGray".to_vec()),
        ColorType::La16 => (16, b"DeviceGray".to_vec()),
        ColorType::Rgb16 => (16, b"DeviceRGB".to_vec()),
        ColorType::Rgba16 => (16, b"DeviceRGB".to_vec()),
        // f32 not supported, maybe JPXDecode?
        ColorType::Rgb32F => {
            return Err(Error::Unimplemented("ColorType::Rgb32F is not supported"));
        }
        ColorType::Rgba32F => {
            return Err(Error::Unimplemented("ColorType::Rgba32F is not supported"));
        }
        // The above ColorType is all the types currently supported by the image crate
        // But ColorType is #[non_exhaustive], there may be new types supported in the future
        _ => {
            return Err(Error::Unimplemented(
                "The image crate supports a new color type, but lopdf has not been updated yet",
            ));
        }
    };

    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"XObject".to_vec()));
    dict.set("Subtype", Object::Name(b"Image".to_vec()));
    dict.set("Width", width);
    dict.set("Height", height);
    dict.set("ColorSpace", Object::Name(color_space));
    dict.set("BitsPerComponent", bpc);

    let format = image::guess_format(&buffer)?;
    if format == ImageFormat::Jpeg {
        // JPEG do not need to be decoded
        dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
        Ok(Stream::new(dict, buffer))
    } else {
        // Other formats need to be decoded
        let img = image::load_from_memory(&buffer)?;
        let content = match img.color() {
            // can be used directly
            ColorType::L8 => img.into_bytes(),
            // need to remove alpha channel
            ColorType::La8 => img.into_luma8().into_raw(),
            // can be used directly
            ColorType::Rgb8 => img.into_bytes(),
            // need to remove alpha channel
            ColorType::Rgba8 => img.into_rgb8().into_raw(),
            // need to convert each 16-bit pixel to big-endian bytes
            ColorType::L16 => img
                .into_luma16()
                .into_raw()
                .iter()
                .flat_map(|&pixel| pixel.to_be_bytes()) // convert each 16-bit pixel to big-endian bytes
                .collect(),
            // need to remove alpha channel, then convert each 16-bit pixel to big-endian bytes
            ColorType::La16 => img
                .into_luma16() // remove alpha channel
                .into_raw()
                .iter()
                .flat_map(|&pixel| pixel.to_be_bytes()) // convert each 16-bit pixel to big-endian bytes
                .collect(),
            // need to convert each 16-bit pixel to big-endian bytes
            ColorType::Rgb16 => img
                .into_rgb16()
                .into_raw()
                .iter()
                .flat_map(|&pixel| pixel.to_be_bytes()) // convert each 16-bit pixel to big-endian bytes
                .collect(),
            // need to remove alpha channel, then convert each 16-bit pixel to big-endian bytes
            ColorType::Rgba16 => img
                .into_rgb16() // remove alpha channel
                .into_raw()
                .iter()
                .flat_map(|&pixel| pixel.to_be_bytes()) // convert each 16-bit pixel to big-endian bytes
                .collect(),
            // f32 not supported, maybe JPXDecode?
            ColorType::Rgb32F => {
                return Err(Error::Unimplemented("ColorType::Rgb32F is not supported"));
            }
            ColorType::Rgba32F => {
                return Err(Error::Unimplemented("ColorType::Rgba32F is not supported"));
            }
            // The above ColorType is all the types currently supported by the image crate
            // But ColorType is #[non_exhaustive], there may be new types supported in the future
            _ => {
                return Err(Error::Unimplemented(
                    "The image library supports a new color type, but lopdf has not been updated yet",
                ));
            }
        };

        let mut img_object = Stream::new(dict, content);
        // Ignore any compression error.
        let _ = img_object.compress();
        Ok(img_object)
    }
}

/// Get the `dimensions` and `color type` without decode, for performance
#[cfg(feature = "embed_image")]
fn get_dimensions_and_color_type(buffer: &Vec<u8>) -> Result<((u32, u32), ColorType)> {
    use image::{ImageDecoder, ImageReader};

    let reader = ImageReader::new(std::io::Cursor::new(buffer));
    let decoder = reader.with_guessed_format()?.into_decoder()?;

    let dimensions = decoder.dimensions();
    let color_type = decoder.color_type();

    Ok((dimensions, color_type))
}

/// Write a small image of the given colour type to `dir`, and return its path.
///
/// The three tests below carried `#[ignore]` because they read
/// `assets/pdf_icon.jpg` and `assets/supported_color_type/`, neither of which
/// this fork ships. An ignored test reports as neither pass nor failure, so all
/// three had been running nowhere -- and `insert_image` in particular covers
/// the JPEG passthrough that `image_from` does without re-encoding.
///
/// The `image` crate is already a dependency of this feature, so the fixtures
/// are generated rather than committed. That also makes the colour-type test
/// state which types it covers, instead of it depending on whatever happens to
/// sit in a directory.
#[cfg(all(test, feature = "embed_image"))]
fn write_test_image(dir: &std::path::Path, name: &str, colour: image::ColorType) -> std::path::PathBuf {
    use image::{DynamicImage, GrayAlphaImage, GrayImage, RgbImage, RgbaImage};

    let (w, h) = (4u32, 3u32);
    let img = match colour {
        image::ColorType::L8 => {
            DynamicImage::ImageLuma8(GrayImage::from_fn(w, h, |x, _| image::Luma([(x * 60) as u8])))
        }
        image::ColorType::La8 => DynamicImage::ImageLumaA8(GrayAlphaImage::from_fn(w, h, |x, _| {
            image::LumaA([(x * 60) as u8, 255])
        })),
        image::ColorType::Rgb8 => DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, y| {
            image::Rgb([(x * 60) as u8, (y * 80) as u8, 30])
        })),
        image::ColorType::Rgba8 => DynamicImage::ImageRgba8(RgbaImage::from_fn(w, h, |x, y| {
            image::Rgba([(x * 60) as u8, (y * 80) as u8, 30, 255])
        })),
        other => panic!("the test does not build {other:?}"),
    };

    let path = dir.join(name);
    img.save(&path).expect("writing the generated fixture");
    path
}

#[cfg(all(feature = "embed_image", not(feature = "async")))]
#[test]
fn insert_image() {
    use super::xobject;
    let dir = tempfile::tempdir().unwrap();
    let jpeg = write_test_image(dir.path(), "icon.jpg", image::ColorType::Rgb8);

    // Not assets/example.pdf: its first page carries no /Contents, so
    // insert_image returns DictKey("Contents") on it. Upstream's version would
    // have failed the same way had it ever run -- the missing fixture hid a
    // second reason it could not pass.
    let mut doc = crate::creator::tests::create_document();
    let pages = doc.get_pages();
    let page_id = *pages.get(&1).unwrap_or_else(|| panic!("Page {} not exist.", 1));
    let img = xobject::image(&jpeg).unwrap();
    doc.insert_image(page_id, img, (100.0, 210.0), (400.0, 225.0)).unwrap();

    // Upstream's version saved a PDF into the working directory and asserted
    // nothing, so it could only fail by panicking. The image has to end up
    // referenced from the page, which is the part insert_image exists to do.
    let (resources, _) = doc.get_page_resources(page_id).expect("page resources");
    let xobjects = resources
        .expect("a resources dictionary")
        .get(b"XObject")
        .and_then(|o| o.as_dict())
        .expect("an /XObject dictionary on the page");
    assert_eq!(xobjects.len(), 1, "exactly the image just inserted");

    let (_, entry) = xobjects.iter().next().unwrap();
    let img_id = entry.as_reference().unwrap();
    let stream = doc.get_object(img_id).unwrap().as_stream().unwrap();
    assert_eq!(stream.dict.get(b"Subtype").unwrap(), &Object::Name(b"Image".to_vec()));
    // JPEG data is passed through rather than re-encoded, so the filter must say so.
    assert_eq!(
        stream.dict.get(b"Filter").unwrap(),
        &Object::Name(b"DCTDecode".to_vec())
    );

    // Registering the image is only half of it; the page content has to draw it.
    // Without this the test passes against an insert_image that emits no `Do`.
    let content = doc.get_and_decode_page_content(page_id).unwrap();
    let drawn: Vec<_> = content.operations.iter().filter(|op| op.operator == "Do").collect();
    assert_eq!(drawn.len(), 1, "the image is drawn exactly once");
    assert_eq!(
        drawn[0].operands,
        vec![Object::Name(format!("X{}", img_id.0).into_bytes())],
        "the Do operator names the XObject that was registered"
    );

    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    assert!(out.starts_with(b"%PDF-"));
}

#[cfg(all(feature = "embed_image", feature = "async"))]
#[tokio::test]
async fn insert_image() {
    use super::xobject;
    let dir = tempfile::tempdir().unwrap();
    let jpeg = write_test_image(dir.path(), "icon.jpg", image::ColorType::Rgb8);

    let mut doc = crate::creator::tests::create_document();
    let pages = doc.get_pages();
    let page_id = *pages.get(&1).unwrap_or_else(|| panic!("Page {} not exist.", 1));
    let img = xobject::image(&jpeg).unwrap();
    doc.insert_image(page_id, img, (100.0, 210.0), (400.0, 225.0)).unwrap();

    let (resources, _) = doc.get_page_resources(page_id).expect("page resources");
    let xobjects = resources
        .expect("a resources dictionary")
        .get(b"XObject")
        .and_then(|o| o.as_dict())
        .expect("an /XObject dictionary on the page");
    assert_eq!(xobjects.len(), 1);

    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    assert!(out.starts_with(b"%PDF-"));
}

#[cfg(feature = "embed_image")]
#[test]
fn embed_supported_color_type() -> Result<()> {
    use content::{Content, Operation};
    use image::GenericImageView;

    // Named rather than read from a directory: the point of the test is which
    // colour types survive the conversion, and a directory listing does not say.
    let dir = tempfile::tempdir()?;
    let img_paths = [
        ("gray.png", image::ColorType::L8),
        ("gray_alpha.png", image::ColorType::La8),
        ("rgb.png", image::ColorType::Rgb8),
        ("rgba.png", image::ColorType::Rgba8),
    ]
    .map(|(name, colour)| write_test_image(dir.path(), name, colour));

    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let mut page_ids = vec![];

    for img_path in img_paths {
        let img = image::open(&img_path)?;
        let (width, height) = img.dimensions();
        let color_type = img.color();
        println!("Image: {img_path:?}, width: {width}, height: {height}, color type: {color_type:?}");

        let image_stream = xobject::image(img_path)?;

        let img_id = doc.add_object(image_stream);
        let img_name = format!("X{}", img_id.0);

        let cm_operation = Operation::new(
            "cm",
            vec![width.into(), 0.into(), 0.into(), height.into(), 0.into(), 0.into()],
        );

        let do_operation = Operation::new("Do", vec![Object::Name(img_name.as_bytes().to_vec())]);
        let content = Content {
            operations: vec![cm_operation, do_operation],
        };

        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode()?));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "MediaBox" => vec![0.into(), 0.into(), width.into(), height.into()],
        });

        doc.add_xobject(page_id, img_name.as_bytes(), img_id)?;
        // add page to doc
        page_ids.push(page_id);
    }

    let pages_dict = dictionary! {
        "Type" => "Pages",
        "Count" => page_ids.len() as u32,
        "Kids" => page_ids.into_iter().map(Object::Reference).collect::<Vec<_>>(),
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages_dict));

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    doc.compress();

    doc.save("supported_color_type.pdf")?;
    Ok(())
}
