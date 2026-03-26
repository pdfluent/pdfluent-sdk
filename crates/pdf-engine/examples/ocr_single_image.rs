#[cfg(feature = "ocr")]
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(feature = "ocr")]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use image::ImageReader;
    use pdf_engine::{OcrBackend, OcrsBackend};

    let image_path = std::env::args().nth(1).ok_or(
        "usage: cargo run -p pdf-engine --features ocr --example ocr_single_image -- <image-path>",
    )?;

    let image = ImageReader::open(&image_path)?.decode()?.to_rgb8();
    let backend = OcrsBackend::try_default()?;
    let result = backend.recognize(image.as_raw(), image.width(), image.height())?;

    print!("{}", result.text);
    Ok(())
}

#[cfg(not(feature = "ocr"))]
fn main() {
    eprintln!("The ocr_single_image example requires --features ocr");
    std::process::exit(1);
}
