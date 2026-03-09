//! Model loading, downloading, and caching for PaddleOCR.

use std::io::Write;
use std::path::{Path, PathBuf};

use ort::session::Session;

use super::dictionary::Dictionary;

/// HuggingFace repository base URL for PaddleOCR ONNX models.
const HF_BASE_URL: &str = "https://huggingface.co/monkt/paddleocr-onnx/resolve/main";

/// Error type for model operations.
#[derive(Debug)]
pub enum ModelError {
    Io(std::io::Error),
    Download(String),
    Session(String),
    Dictionary(String),
}

impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "model I/O error: {e}"),
            Self::Download(msg) => write!(f, "model download error: {msg}"),
            Self::Session(msg) => write!(f, "session creation error: {msg}"),
            Self::Dictionary(msg) => write!(f, "dictionary error: {msg}"),
        }
    }
}

impl std::error::Error for ModelError {}

impl From<std::io::Error> for ModelError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Loaded ONNX sessions and dictionary for PaddleOCR inference.
pub struct ModelSessions {
    pub detection: Session,
    pub recognition: Session,
    pub classifier: Option<Session>,
    pub dictionary: Dictionary,
}

/// Detection model variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectionModel {
    /// PP-OCRv3 (2.3 MB) — lightweight, good for most documents.
    V3,
    /// PP-OCRv5 (84 MB) — best quality.
    V5,
}

/// Recognition language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    Latin,
    Chinese,
    Japanese,
    Korean,
    Arabic,
}

impl Language {
    /// Language code string.
    pub fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Latin => "latin",
            Self::Chinese => "ch",
            Self::Japanese => "japan",
            Self::Korean => "korean",
            Self::Arabic => "arabic",
        }
    }

    /// HuggingFace directory path for this language (used for downloads).
    fn hf_dir(self) -> &'static str {
        match self {
            Self::English => "languages/english",
            Self::Latin => "languages/latin",
            Self::Chinese => "languages/chinese",
            Self::Japanese => "languages/japan",
            Self::Korean => "languages/korean",
            Self::Arabic => "languages/arabic",
        }
    }

    /// Local directory name for cached models.
    fn local_dir(self) -> &'static str {
        match self {
            Self::English => "english",
            Self::Latin => "latin",
            Self::Chinese => "chinese",
            Self::Japanese => "japan",
            Self::Korean => "korean",
            Self::Arabic => "arabic",
        }
    }
}

/// Configuration for the PaddleOCR engine.
pub struct PaddleOcrConfig {
    /// Directory containing ONNX model files.
    pub model_dir: PathBuf,
    /// Detection model variant.
    pub detection_model: DetectionModel,
    /// Languages for recognition (determines which rec model + dict to load).
    pub languages: Vec<Language>,
    /// Whether to use the angle classifier (for rotated text).
    pub use_angle_classifier: bool,
    /// Maximum side length for detection input (default 960).
    pub max_side_len: u32,
    /// Detection threshold (default 0.3).
    pub det_threshold: f32,
    /// Box threshold (default 0.6).
    pub box_threshold: f32,
    /// Recognition batch size (default 8).
    pub rec_batch_size: usize,
    /// Number of ONNX Runtime intra-op threads (default 4).
    pub num_threads: usize,
}

impl Default for PaddleOcrConfig {
    fn default() -> Self {
        Self {
            model_dir: default_cache_dir(),
            detection_model: DetectionModel::V3,
            languages: vec![Language::Latin],
            use_angle_classifier: false,
            max_side_len: 960,
            det_threshold: 0.3,
            box_threshold: 0.6,
            rec_batch_size: 8,
            num_threads: 4,
        }
    }
}

fn default_cache_dir() -> PathBuf {
    dirs_next::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("xfa")
        .join("ocr-models")
}

/// Get the local path for the detection model.
pub fn detection_model_path(config: &PaddleOcrConfig) -> PathBuf {
    let dir = match config.detection_model {
        DetectionModel::V3 => "det_v3",
        DetectionModel::V5 => "det_v5",
    };
    config.model_dir.join(dir).join("det.onnx")
}

/// Get the local path for the recognition model (first language).
pub fn recognition_model_path(config: &PaddleOcrConfig) -> PathBuf {
    let lang = config.languages.first().copied().unwrap_or(Language::Latin);
    config.model_dir.join(lang.local_dir()).join("rec.onnx")
}

/// Get the local path for the dictionary file (first language).
pub fn dictionary_path(config: &PaddleOcrConfig) -> PathBuf {
    let lang = config.languages.first().copied().unwrap_or(Language::Latin);
    config.model_dir.join(lang.local_dir()).join("dict.txt")
}

/// Get the local path for the angle classifier model.
pub fn classifier_model_path(config: &PaddleOcrConfig) -> PathBuf {
    config.model_dir.join("cls").join("cls.onnx")
}

/// Check if required models are available locally.
pub fn models_available(config: &PaddleOcrConfig) -> bool {
    let det = detection_model_path(config).exists();
    let rec = recognition_model_path(config).exists();
    let dict = dictionary_path(config).exists();
    let cls = if config.use_angle_classifier {
        classifier_model_path(config).exists()
    } else {
        true
    };
    det && rec && dict && cls
}

/// Download required models from HuggingFace to the model directory.
pub fn download_models(config: &PaddleOcrConfig) -> Result<(), ModelError> {
    let det_hf_dir = match config.detection_model {
        DetectionModel::V3 => "detection/v3",
        DetectionModel::V5 => "detection/v5",
    };

    download_file_if_missing(
        &format!("{det_hf_dir}/det.onnx"),
        &detection_model_path(config),
    )?;

    let lang = config.languages.first().copied().unwrap_or(Language::Latin);
    download_file_if_missing(
        &format!("{}/rec.onnx", lang.hf_dir()),
        &recognition_model_path(config),
    )?;
    download_file_if_missing(
        &format!("{}/dict.txt", lang.hf_dir()),
        &dictionary_path(config),
    )?;

    if config.use_angle_classifier {
        download_file_if_missing("preprocessing/cls.onnx", &classifier_model_path(config))?;
    }

    Ok(())
}

fn download_file_if_missing(hf_path: &str, local_path: &Path) -> Result<(), ModelError> {
    if local_path.exists() {
        return Ok(());
    }

    if let Some(parent) = local_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let url = format!("{HF_BASE_URL}/{hf_path}");
    eprintln!("Downloading {url} ...");

    let agent = ureq::Agent::new_with_defaults();
    let response = agent
        .get(&url)
        .call()
        .map_err(|e| ModelError::Download(format!("{url}: {e}")))?;

    let body = response.into_body();
    let mut reader = body.into_reader();
    let mut file = std::fs::File::create(local_path)?;
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = std::io::Read::read(&mut reader, &mut buf)
            .map_err(|e| ModelError::Download(format!("read error: {e}")))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
    }

    eprintln!("  → saved to {}", local_path.display());
    Ok(())
}

/// Load ONNX sessions for all required models.
pub fn load_sessions(config: &PaddleOcrConfig) -> Result<ModelSessions, ModelError> {
    let detection = create_session(&detection_model_path(config), config.num_threads)?;
    let recognition = create_session(&recognition_model_path(config), config.num_threads)?;

    let classifier = if config.use_angle_classifier {
        Some(create_session(
            &classifier_model_path(config),
            config.num_threads,
        )?)
    } else {
        None
    };

    let dictionary = Dictionary::from_file(&dictionary_path(config))
        .map_err(|e| ModelError::Dictionary(e.to_string()))?;

    Ok(ModelSessions {
        detection,
        recognition,
        classifier,
        dictionary,
    })
}

fn create_session(model_path: &Path, num_threads: usize) -> Result<Session, ModelError> {
    let session = Session::builder()
        .map_err(|e| ModelError::Session(e.to_string()))?
        .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
        .map_err(|e| ModelError::Session(e.to_string()))?
        .with_intra_threads(num_threads)
        .map_err(|e| ModelError::Session(e.to_string()))?
        .commit_from_file(model_path)
        .map_err(|e| ModelError::Session(format!("{}: {e}", model_path.display())))?;
    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_sensible() {
        let config = PaddleOcrConfig::default();
        assert_eq!(config.max_side_len, 960);
        assert!((config.det_threshold - 0.3).abs() < f32::EPSILON);
        assert_eq!(config.rec_batch_size, 8);
        assert_eq!(config.num_threads, 4);
        assert!(!config.use_angle_classifier);
    }

    #[test]
    fn language_code_mapping() {
        assert_eq!(Language::English.code(), "en");
        assert_eq!(Language::Latin.code(), "latin");
        assert_eq!(Language::Chinese.code(), "ch");
        assert_eq!(Language::Japanese.code(), "japan");
        assert_eq!(Language::Korean.code(), "korean");
        assert_eq!(Language::Arabic.code(), "arabic");
    }

    #[test]
    fn model_path_construction_v3() {
        let config = PaddleOcrConfig {
            model_dir: PathBuf::from("/tmp/models"),
            detection_model: DetectionModel::V3,
            languages: vec![Language::English],
            ..Default::default()
        };
        let det_path = detection_model_path(&config);
        assert!(det_path.to_str().unwrap().contains("det_v3"));
        assert!(det_path.to_str().unwrap().ends_with("det.onnx"));
    }

    #[test]
    fn model_path_construction_v5() {
        let config = PaddleOcrConfig {
            model_dir: PathBuf::from("/tmp/models"),
            detection_model: DetectionModel::V5,
            languages: vec![Language::Latin],
            ..Default::default()
        };
        let det_path = detection_model_path(&config);
        assert!(det_path.to_str().unwrap().contains("det_v5"));

        let rec_path = recognition_model_path(&config);
        assert!(rec_path.to_str().unwrap().contains("latin"));
        assert!(rec_path.to_str().unwrap().ends_with("rec.onnx"));

        let dict_path = dictionary_path(&config);
        assert!(dict_path.to_str().unwrap().contains("latin"));
        assert!(dict_path.to_str().unwrap().ends_with("dict.txt"));
    }

    #[test]
    fn models_not_available_when_missing() {
        let config = PaddleOcrConfig {
            model_dir: PathBuf::from("/nonexistent/path"),
            ..Default::default()
        };
        assert!(!models_available(&config));
    }
}
