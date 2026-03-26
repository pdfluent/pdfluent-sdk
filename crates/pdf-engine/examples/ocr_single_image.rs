#[cfg(any(feature = "ocr", feature = "ocr-onnx"))]
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(any(feature = "ocr", feature = "ocr-onnx"))]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use image::ImageReader;

    let image_path = std::env::args().nth(1).ok_or(
        "usage: cargo run -p pdf-engine --features <ocr|ocr-onnx> --example ocr_single_image -- <image-path>",
    )?;

    let image = ImageReader::open(&image_path)?.decode()?.to_rgb8();

    #[cfg(feature = "ocr")]
    {
        use pdf_engine::{OcrBackend, OcrsBackend};

        let backend = OcrsBackend::try_default()?;
        let result = backend.recognize(image.as_raw(), image.width(), image.height())?;
        print!("{}", result.text);
        return Ok(());
    }

    #[cfg(all(not(feature = "ocr"), feature = "ocr-onnx"))]
    {
        use pdf_ocr::{OcrEngine, PaddleOcrEngine};

        let engine = PaddleOcrEngine::new()?;
        let result = engine.recognize(image.as_raw(), image.width(), image.height(), 200)?;
        print!("{}", result.full_text());
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err("No OCR backend feature enabled".into())
}

#[cfg(not(any(feature = "ocr", feature = "ocr-onnx")))]
fn main() {
    eprintln!("The ocr_single_image example requires --features ocr or --features ocr-onnx");
    std::process::exit(1);
}
