#[cfg(not(any(feature = "ocr", feature = "ocr-onnx")))]
fn main() {
    eprintln!("This example requires `--features ocr` or `--features ocr-onnx`.");
    std::process::exit(1);
}

#[cfg(feature = "ocr-onnx")]
use pdf_engine::ocr::PaddleOnnxBackend;
#[cfg(feature = "ocr")]
use pdf_engine::OcrsBackend;
#[cfg(any(feature = "ocr", feature = "ocr-onnx"))]
use pdf_engine::{OcrBackend, PdfDocument};
#[cfg(feature = "ocr-onnx")]
use pdf_ocr::paddle::{DetectionModel, Language, PaddleOcrConfig};
#[cfg(any(feature = "ocr", feature = "ocr-onnx"))]
use std::fs;
#[cfg(any(feature = "ocr", feature = "ocr-onnx"))]
use std::path::PathBuf;

#[cfg(any(feature = "ocr", feature = "ocr-onnx"))]
const DEFAULT_CASES: &[&str] = &[
    "corpus/fw4v.pdf",
    "corpus/fw8ben.pdf",
    "corpus/f982.pdf",
    "corpus/sf181.pdf",
    "corpus/f461.pdf",
];

#[cfg(any(feature = "ocr", feature = "ocr-onnx"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let positional_args = args
        .iter()
        .filter(|arg| !arg.starts_with("--backend="))
        .cloned()
        .collect::<Vec<_>>();

    let mut paths = Vec::new();
    let mut backend_name = String::new();
    let backend = select_backend(&args, &mut backend_name)?;

    if positional_args.is_empty() {
        paths.extend(DEFAULT_CASES.iter().map(PathBuf::from));
    } else {
        paths.extend(positional_args.into_iter().map(PathBuf::from));
    }

    println!("backend={backend_name}");
    println!("filename\ttext_length\tocr_length\tsimilarity%");

    let mut total = 0.0f64;
    let mut count = 0usize;

    for path in paths {
        let data = fs::read(&path)?;
        let doc = PdfDocument::open(data)?;
        let expected = normalize_text(&doc.extract_text(0)?);
        let ocr = normalize_text(&doc.ocr_page(0, backend.as_ref(), 150.0)?.text);
        let similarity = levenshtein_similarity(&expected, &ocr);
        let label = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown");

        println!(
            "{label}\t{}\t{}\t{:.1}",
            expected.chars().count(),
            ocr.chars().count(),
            similarity * 100.0
        );

        total += similarity;
        count += 1;
    }

    let average = if count == 0 {
        0.0
    } else {
        total / count as f64
    };
    println!("average\t-\t-\t{:.1}", average * 100.0);

    Ok(())
}

#[cfg(any(feature = "ocr", feature = "ocr-onnx"))]
fn select_backend(
    args: &[String],
    backend_name: &mut String,
) -> Result<Box<dyn OcrBackend>, Box<dyn std::error::Error>> {
    let requested = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--backend="))
        .unwrap_or(default_backend_name());

    match requested {
        "ocrs" => {
            #[cfg(feature = "ocr")]
            {
                *backend_name = "ocrs".to_string();
                return Ok(Box::new(OcrsBackend::try_default()?));
            }
            #[cfg(not(feature = "ocr"))]
            return Err("backend `ocrs` requires `--features ocr`".into());
        }
        "paddle-onnx" => {
            #[cfg(feature = "ocr-onnx")]
            {
                *backend_name = "paddle-onnx".to_string();
                return Ok(Box::new(build_paddle_backend()?));
            }
            #[cfg(not(feature = "ocr-onnx"))]
            return Err("backend `paddle-onnx` requires `--features ocr-onnx`".into());
        }
        other => Err(format!("unsupported backend `{other}`").into()),
    }
}

#[cfg(any(feature = "ocr", feature = "ocr-onnx"))]
fn default_backend_name() -> &'static str {
    #[cfg(feature = "ocr-onnx")]
    {
        return "paddle-onnx";
    }
    #[cfg(all(feature = "ocr", not(feature = "ocr-onnx")))]
    {
        return "ocrs";
    }
    #[allow(unreachable_code)]
    "ocrs"
}

#[cfg(feature = "ocr-onnx")]
fn build_paddle_backend() -> Result<PaddleOnnxBackend, Box<dyn std::error::Error>> {
    let mut config = PaddleOcrConfig::default();

    if matches!(
        std::env::var("PADDLE_DET").ok().as_deref(),
        Some("v5") | Some("V5")
    ) {
        config.detection_model = DetectionModel::V5;
    }

    config.languages = match std::env::var("PADDLE_LANG").ok().as_deref() {
        Some("english") | Some("en") => vec![Language::English],
        Some("latin") => vec![Language::Latin],
        Some("japanese") | Some("jp") => vec![Language::Japanese],
        Some("korean") | Some("ko") => vec![Language::Korean],
        Some("arabic") | Some("ar") => vec![Language::Arabic],
        _ => config.languages,
    };

    if matches!(
        std::env::var("PADDLE_CLS").ok().as_deref(),
        Some("1") | Some("true") | Some("yes")
    ) {
        config.use_angle_classifier = true;
    }

    Ok(PaddleOnnxBackend::with_config(config)?)
}

#[cfg(any(feature = "ocr", feature = "ocr-onnx"))]
fn normalize_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(any(feature = "ocr", feature = "ocr-onnx"))]
fn levenshtein_similarity(expected: &str, actual: &str) -> f64 {
    let expected_chars = expected.chars().collect::<Vec<_>>();
    let actual_chars = actual.chars().collect::<Vec<_>>();
    let max_len = expected_chars.len().max(actual_chars.len());
    if max_len == 0 {
        return 1.0;
    }

    let dist = levenshtein_distance(&expected_chars, &actual_chars);
    1.0 - dist as f64 / max_len as f64
}

#[cfg(any(feature = "ocr", feature = "ocr-onnx"))]
fn levenshtein_distance(expected: &[char], actual: &[char]) -> usize {
    if expected.is_empty() {
        return actual.len();
    }
    if actual.is_empty() {
        return expected.len();
    }

    let mut prev = (0..=actual.len()).collect::<Vec<_>>();
    let mut curr = vec![0usize; actual.len() + 1];

    for (i, expected_char) in expected.iter().enumerate() {
        curr[0] = i + 1;

        for (j, actual_char) in actual.iter().enumerate() {
            let cost = usize::from(expected_char != actual_char);
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }

        std::mem::swap(&mut prev, &mut curr);
    }

    prev[actual.len()]
}
