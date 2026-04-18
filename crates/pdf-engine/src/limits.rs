//! Resource limits for PDF processing.
//!
//! These limits protect against adversarial inputs that could cause OOM, stack overflow,
//! CPU exhaustion, or zip bombs. All limits are enforced with clean error returns — no panics.
//!
//! # Default limits
//!
//! The defaults cover 99.9% of real-world PDFs while providing strong safety guarantees:
//!
//! | Resource | Default |
//! |---|---|
//! | PDF file size | 500 MB |
//! | Single decompressed stream | 256 MB |
//! | Total memory per document | 1 GB |
//! | Object reference depth | 100 levels |
//! | Content stream operators | 10,000,000 |
//! | Image pixel count | 256 megapixels (16384×16384) |
//! | XFA template nesting depth | 50 levels |
//! | FormCalc recursion depth | 200 levels |

/// Resource limits for a single PDF processing operation.
///
/// Construct via [`ProcessingLimits::default()`] for standard limits,
/// or use the builder methods to customize for your use case.
///
/// # Examples
///
/// ```rust
/// use pdf_engine::limits::ProcessingLimits;
///
/// // Default limits (recommended for server-side processing):
/// let limits = ProcessingLimits::default();
///
/// // Stricter limits for WASM/browser context:
/// let wasm_limits = ProcessingLimits::wasm();
///
/// // Custom limits:
/// let custom = ProcessingLimits::default()
///     .max_file_bytes(100 * 1024 * 1024)   // 100 MB
///     .max_stream_bytes(64 * 1024 * 1024);  // 64 MB per stream
/// ```
#[derive(Debug, Clone)]
pub struct ProcessingLimits {
    /// Maximum PDF file size in bytes. Default: 500 MB.
    pub max_file_bytes: u64,
    /// Maximum decompressed size of any single stream. Default: 256 MB.
    /// Prevents zip bombs via FlateDecode or LZWDecode.
    pub max_stream_bytes: u64,
    /// Maximum total memory allocated per document. Default: 1 GB.
    pub max_total_memory_bytes: u64,
    /// Maximum object reference depth (prevents stack overflow on recursive refs). Default: 100.
    pub max_object_depth: u32,
    /// Maximum content stream operators per page. Default: 10,000,000.
    pub max_operator_count: u64,
    /// Maximum pixel count per image (width × height). Default: 268,435,456 (16384²).
    pub max_image_pixels: u64,
    /// Maximum XFA template XML nesting depth. Default: 50.
    pub max_xfa_nesting_depth: u32,
    /// Maximum FormCalc recursion depth. Default: 200.
    pub max_formcalc_depth: u32,
}

impl Default for ProcessingLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 500 * 1024 * 1024,         // 500 MB
            max_stream_bytes: 256 * 1024 * 1024,        // 256 MB
            max_total_memory_bytes: 1024 * 1024 * 1024, // 1 GB
            max_object_depth: 100,
            max_operator_count: 10_000_000,
            max_image_pixels: 16384 * 16384,             // 268 MP
            max_xfa_nesting_depth: 50,
            max_formcalc_depth: 200,
        }
    }
}

impl ProcessingLimits {
    /// Create a new set of limits with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Strict limits for WASM/browser contexts with limited memory.
    ///
    /// - Max file: 50 MB
    /// - Max stream: 32 MB
    /// - Max total memory: 128 MB
    /// - Image pixels: 64 MP (8192×8192)
    pub fn wasm() -> Self {
        Self {
            max_file_bytes: 50 * 1024 * 1024,          // 50 MB
            max_stream_bytes: 32 * 1024 * 1024,         // 32 MB
            max_total_memory_bytes: 128 * 1024 * 1024,  // 128 MB
            max_object_depth: 50,
            max_operator_count: 1_000_000,
            max_image_pixels: 8192 * 8192,               // 64 MP
            max_xfa_nesting_depth: 30,
            max_formcalc_depth: 100,
        }
    }

    /// Unlimited: no resource limits (use only in trusted environments).
    pub fn unlimited() -> Self {
        Self {
            max_file_bytes: u64::MAX,
            max_stream_bytes: u64::MAX,
            max_total_memory_bytes: u64::MAX,
            max_object_depth: u32::MAX,
            max_operator_count: u64::MAX,
            max_image_pixels: u64::MAX,
            max_xfa_nesting_depth: u32::MAX,
            max_formcalc_depth: u32::MAX,
        }
    }

    /// Set maximum PDF file size.
    pub fn max_file_bytes(mut self, bytes: u64) -> Self {
        self.max_file_bytes = bytes;
        self
    }

    /// Set maximum decompressed stream size.
    pub fn max_stream_bytes(mut self, bytes: u64) -> Self {
        self.max_stream_bytes = bytes;
        self
    }

    /// Set maximum total memory per document.
    pub fn max_total_memory_bytes(mut self, bytes: u64) -> Self {
        self.max_total_memory_bytes = bytes;
        self
    }

    /// Set maximum object reference depth.
    pub fn max_object_depth(mut self, depth: u32) -> Self {
        self.max_object_depth = depth;
        self
    }

    /// Set maximum content stream operator count.
    pub fn max_operator_count(mut self, count: u64) -> Self {
        self.max_operator_count = count;
        self
    }

    /// Set maximum image pixel count (width × height).
    pub fn max_image_pixels(mut self, pixels: u64) -> Self {
        self.max_image_pixels = pixels;
        self
    }

    /// Set maximum XFA template nesting depth.
    pub fn max_xfa_nesting_depth(mut self, depth: u32) -> Self {
        self.max_xfa_nesting_depth = depth;
        self
    }

    /// Set maximum FormCalc recursion depth.
    pub fn max_formcalc_depth(mut self, depth: u32) -> Self {
        self.max_formcalc_depth = depth;
        self
    }

    /// Check if a file size is within limits. Returns `Err` with a descriptive message if exceeded.
    pub fn check_file_size(&self, bytes: u64) -> Result<(), LimitError> {
        if bytes > self.max_file_bytes {
            Err(LimitError::FileTooLarge {
                actual_bytes: bytes,
                limit_bytes: self.max_file_bytes,
            })
        } else {
            Ok(())
        }
    }

    /// Check if a decompressed stream size is within limits.
    pub fn check_stream_size(&self, bytes: u64) -> Result<(), LimitError> {
        if bytes > self.max_stream_bytes {
            Err(LimitError::StreamTooLarge {
                actual_bytes: bytes,
                limit_bytes: self.max_stream_bytes,
            })
        } else {
            Ok(())
        }
    }

    /// Check if image dimensions are within limits.
    pub fn check_image_pixels(&self, width: u64, height: u64) -> Result<(), LimitError> {
        let pixels = width.saturating_mul(height);
        if pixels > self.max_image_pixels {
            Err(LimitError::ImageTooLarge {
                width,
                height,
                pixels,
                limit_pixels: self.max_image_pixels,
            })
        } else {
            Ok(())
        }
    }

    /// Check if an object depth is within limits.
    pub fn check_object_depth(&self, depth: u32) -> Result<(), LimitError> {
        if depth > self.max_object_depth {
            Err(LimitError::ObjectDepthExceeded {
                depth,
                limit: self.max_object_depth,
            })
        } else {
            Ok(())
        }
    }
}

/// Error returned when a resource limit is exceeded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LimitError {
    /// PDF file exceeds the maximum allowed size.
    FileTooLarge { actual_bytes: u64, limit_bytes: u64 },
    /// A decompressed stream exceeds the maximum allowed size.
    StreamTooLarge { actual_bytes: u64, limit_bytes: u64 },
    /// An image exceeds the maximum allowed pixel count.
    ImageTooLarge { width: u64, height: u64, pixels: u64, limit_pixels: u64 },
    /// An object reference chain exceeds the maximum allowed depth.
    ObjectDepthExceeded { depth: u32, limit: u32 },
    /// A content stream has too many operators.
    TooManyOperators { count: u64, limit: u64 },
    /// XFA template nesting is too deep.
    XfaNestingTooDeep { depth: u32, limit: u32 },
    /// FormCalc recursion is too deep.
    FormCalcRecursionTooDeep { depth: u32, limit: u32 },
}

impl std::fmt::Display for LimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FileTooLarge { actual_bytes, limit_bytes } =>
                write!(f, "PDF file too large: {} MB (limit: {} MB)",
                    actual_bytes / 1024 / 1024, limit_bytes / 1024 / 1024),
            Self::StreamTooLarge { actual_bytes, limit_bytes } =>
                write!(f, "Decompressed stream too large: {} MB (limit: {} MB)",
                    actual_bytes / 1024 / 1024, limit_bytes / 1024 / 1024),
            Self::ImageTooLarge { width, height, pixels, limit_pixels } =>
                write!(f, "Image too large: {}×{} ({} MP, limit: {} MP)",
                    width, height, pixels / 1_000_000, limit_pixels / 1_000_000),
            Self::ObjectDepthExceeded { depth, limit } =>
                write!(f, "Object reference depth exceeded: {} (limit: {})", depth, limit),
            Self::TooManyOperators { count, limit } =>
                write!(f, "Content stream has too many operators: {} (limit: {})", count, limit),
            Self::XfaNestingTooDeep { depth, limit } =>
                write!(f, "XFA template nesting too deep: {} (limit: {})", depth, limit),
            Self::FormCalcRecursionTooDeep { depth, limit } =>
                write!(f, "FormCalc recursion too deep: {} (limit: {})", depth, limit),
        }
    }
}

impl std::error::Error for LimitError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_limits() {
        let l = ProcessingLimits::default();
        assert_eq!(l.max_file_bytes, 500 * 1024 * 1024);
        assert_eq!(l.max_stream_bytes, 256 * 1024 * 1024);
        assert_eq!(l.max_image_pixels, 16384 * 16384);
    }

    #[test]
    fn test_wasm_limits_stricter_than_default() {
        let wasm = ProcessingLimits::wasm();
        let default = ProcessingLimits::default();
        assert!(wasm.max_file_bytes < default.max_file_bytes);
        assert!(wasm.max_stream_bytes < default.max_stream_bytes);
        assert!(wasm.max_image_pixels < default.max_image_pixels);
    }

    #[test]
    fn test_file_size_check() {
        let l = ProcessingLimits::default();
        assert!(l.check_file_size(100 * 1024 * 1024).is_ok());   // 100 MB: ok
        assert!(l.check_file_size(500 * 1024 * 1024).is_ok());   // 500 MB: ok (at limit)
        assert!(l.check_file_size(501 * 1024 * 1024).is_err());  // 501 MB: exceeded
    }

    #[test]
    fn test_image_pixel_check() {
        let l = ProcessingLimits::default();
        assert!(l.check_image_pixels(1920, 1080).is_ok());
        assert!(l.check_image_pixels(16384, 16384).is_ok());   // at limit
        assert!(l.check_image_pixels(16385, 16384).is_err());  // just over
    }

    #[test]
    fn test_stream_size_check() {
        let l = ProcessingLimits::default();
        assert!(l.check_stream_size(100 * 1024 * 1024).is_ok());
        assert!(l.check_stream_size(256 * 1024 * 1024).is_ok());  // at limit
        assert!(l.check_stream_size(257 * 1024 * 1024).is_err()); // exceeded
    }

    #[test]
    fn test_builder_pattern() {
        let l = ProcessingLimits::default()
            .max_file_bytes(10 * 1024 * 1024)
            .max_stream_bytes(5 * 1024 * 1024);
        assert_eq!(l.max_file_bytes, 10 * 1024 * 1024);
        assert_eq!(l.max_stream_bytes, 5 * 1024 * 1024);
    }

    #[test]
    fn test_limit_error_display() {
        let err = LimitError::FileTooLarge { actual_bytes: 600 * 1024 * 1024, limit_bytes: 500 * 1024 * 1024 };
        let msg = err.to_string();
        assert!(msg.contains("600 MB"));
        assert!(msg.contains("500 MB"));
    }
}
