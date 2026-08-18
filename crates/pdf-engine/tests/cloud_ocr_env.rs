// All `OcrBackend`/`OcrError` uses are gated behind the `ocr-*` feature
// flags below; without any of them enabled the import + helper fns look
// dead. Allow that — features are external knobs.
#![allow(dead_code, unused_imports)]

#[cfg(not(target_arch = "wasm32"))]
use pdf_engine::{OcrBackend, OcrError};

#[cfg(feature = "ocr-aws")]
use pdf_engine::AwsTextractBackend;
#[cfg(feature = "ocr-azure")]
use pdf_engine::AzureDocIntelBackend;
#[cfg(feature = "ocr-google")]
use pdf_engine::GoogleVisionBackend;
#[cfg(feature = "ocr-mistral")]
use pdf_engine::MistralOcrBackend;

#[cfg(all(feature = "ocr-mistral", not(target_arch = "wasm32")))]
#[test]
fn mistral_env_live_smoke() {
    if std::env::var_os("MISTRAL_API_KEY").is_none() {
        eprintln!(
            "SKIPPED (not a pass): precondition not met at {}:{}",
            file!(),
            line!()
        );
        return;
    }

    let backend = MistralOcrBackend::from_env().expect("Mistral backend from env");
    let (pixels, width, height) = test_text_image("TEST 123");

    match backend.recognize(&pixels, width, height) {
        Ok(result) => assert!(result.confidence >= 0.0),
        Err(OcrError::RecognitionFailed(message)) if is_soft_auth_failure(&message) => {}
        Err(error) => panic!("live Mistral OCR request failed: {error}"),
    }
}

#[cfg(all(feature = "ocr-google", not(target_arch = "wasm32")))]
#[test]
fn google_env_live_smoke() {
    if std::env::var_os("GOOGLE_VISION_API_KEY").is_none()
        && std::env::var_os("GOOGLE_APPLICATION_CREDENTIALS").is_none()
    {
        return;
    }

    let backend = GoogleVisionBackend::from_env().expect("Google backend from env");
    let (pixels, width, height) = test_text_image("TEST 123");

    match backend.recognize(&pixels, width, height) {
        Ok(result) => assert!(result.confidence >= 0.0),
        Err(OcrError::RecognitionFailed(message)) if is_soft_auth_failure(&message) => {}
        Err(error) => panic!("live Google Vision OCR request failed: {error}"),
    }
}

#[cfg(all(feature = "ocr-aws", not(target_arch = "wasm32")))]
#[test]
fn aws_env_live_smoke() {
    let has_explicit_env = std::env::var_os("AWS_REGION").is_some()
        && std::env::var_os("AWS_ACCESS_KEY_ID").is_some()
        && std::env::var_os("AWS_SECRET_ACCESS_KEY").is_some();
    let has_profile = std::env::var_os("AWS_PROFILE").is_some();
    if !has_explicit_env && !has_profile {
        return;
    }

    let backend = AwsTextractBackend::from_env().expect("AWS backend from env");
    let (pixels, width, height) = test_text_image("TEST 123");

    match backend.recognize(&pixels, width, height) {
        Ok(result) => assert!(result.confidence >= 0.0),
        Err(OcrError::RecognitionFailed(message)) if is_soft_auth_failure(&message) => {}
        Err(error) => panic!("live AWS Textract OCR request failed: {error}"),
    }
}

#[cfg(all(feature = "ocr-azure", not(target_arch = "wasm32")))]
#[test]
fn azure_env_live_smoke() {
    if std::env::var_os("AZURE_DOCUMENT_INTELLIGENCE_ENDPOINT").is_none()
        || std::env::var_os("AZURE_DOCUMENT_INTELLIGENCE_KEY").is_none()
    {
        return;
    }

    let backend = AzureDocIntelBackend::from_env().expect("Azure backend from env");
    let (pixels, width, height) = test_text_image("TEST 123");

    match backend.recognize(&pixels, width, height) {
        Ok(result) => assert!(result.confidence >= 0.0),
        Err(OcrError::RecognitionFailed(message)) if is_soft_auth_failure(&message) => {}
        Err(error) => panic!("live Azure OCR request failed: {error}"),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn is_soft_auth_failure(message: &str) -> bool {
    let lowercase = message.to_ascii_lowercase();
    lowercase.contains("401")
        || lowercase.contains("403")
        || lowercase.contains("unauthorized")
        || lowercase.contains("forbidden")
        || lowercase.contains("accessdenied")
        || lowercase.contains("access denied")
        || lowercase.contains("unrecognizedclient")
        || lowercase.contains("signaturedoesnotmatch")
        || lowercase.contains("invalidsubscriptionkey")
        || lowercase.contains("permission")
}

#[cfg(not(target_arch = "wasm32"))]
fn test_text_image(text: &str) -> (Vec<u8>, u32, u32) {
    let scale = 8usize;
    let glyph_width = 5usize;
    let glyph_height = 7usize;
    let spacing = 2usize;
    let margin = 12usize;
    let width = margin * 2
        + text
            .chars()
            .map(|ch| match ch {
                ' ' => scale * 3,
                _ => glyph_width * scale + spacing * scale,
            })
            .sum::<usize>();
    let height = margin * 2 + glyph_height * scale;
    let mut pixels = vec![255u8; width * height * 3];
    let mut cursor_x = margin;

    for ch in text.chars() {
        if ch == ' ' {
            cursor_x += scale * 3;
            continue;
        }

        draw_glyph(
            &mut pixels,
            width,
            cursor_x,
            margin,
            scale,
            glyph_pattern(ch),
        );
        cursor_x += glyph_width * scale + spacing * scale;
    }

    (pixels, width as u32, height as u32)
}

#[cfg(not(target_arch = "wasm32"))]
fn draw_glyph(
    pixels: &mut [u8],
    image_width: usize,
    offset_x: usize,
    offset_y: usize,
    scale: usize,
    glyph: [&str; 7],
) {
    for (row, pattern) in glyph.into_iter().enumerate() {
        for (col, bit) in pattern.bytes().enumerate() {
            if bit != b'#' {
                continue;
            }

            for dy in 0..scale {
                for dx in 0..scale {
                    let x = offset_x + col * scale + dx;
                    let y = offset_y + row * scale + dy;
                    let idx = (y * image_width + x) * 3;
                    pixels[idx] = 0;
                    pixels[idx + 1] = 0;
                    pixels[idx + 2] = 0;
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn glyph_pattern(ch: char) -> [&'static str; 7] {
    match ch {
        '1' => [
            "..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###.",
        ],
        '2' => [
            ".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####",
        ],
        '3' => [
            ".###.", "#...#", "....#", "..##.", "....#", "#...#", ".###.",
        ],
        'E' => [
            "#####", "#....", "#....", "####.", "#....", "#....", "#####",
        ],
        'S' => [
            ".####", "#....", "#....", ".###.", "....#", "....#", "####.",
        ],
        'T' => [
            "#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#..",
        ],
        _ => panic!("unsupported glyph: {ch}"),
    }
}
