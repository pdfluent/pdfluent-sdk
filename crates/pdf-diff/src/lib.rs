//! Visual PDF comparison — page-level SSIM similarity and diff image generation.
//!
//! Compares two sets of rendered page images using Structural Similarity Index (SSIM)
//! and generates visual diff overlays highlighting changed regions.

mod ssim;

use thiserror::Error;

/// Errors from the diff comparison process.
#[derive(Debug, Error)]
pub enum DiffError {
    /// Image dimensions are invalid (zero width or height).
    #[error("invalid image dimensions: {0}x{1}")]
    InvalidDimensions(u32, u32),

    /// Pixel buffer size does not match declared dimensions.
    #[error("pixel buffer size {actual} does not match expected {expected} (w={w}, h={h})")]
    BufferSizeMismatch {
        expected: usize,
        actual: usize,
        w: u32,
        h: u32,
    },
}

/// A rendered page image for comparison.
#[derive(Debug, Clone)]
pub struct PageImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// RGBA pixel data, row-major, 4 bytes per pixel.
    pub pixels: Vec<u8>,
}

impl PageImage {
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, DiffError> {
        if width == 0 || height == 0 {
            return Err(DiffError::InvalidDimensions(width, height));
        }
        let expected = (width as usize) * (height as usize) * 4;
        if pixels.len() != expected {
            return Err(DiffError::BufferSizeMismatch {
                expected,
                actual: pixels.len(),
                w: width,
                h: height,
            });
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }
}

/// Options for the diff comparison.
#[derive(Debug, Clone)]
pub struct DiffOptions {
    /// SSIM threshold below which pages are considered "changed" (default: 0.98).
    pub similarity_threshold: f64,
    /// Whether to generate diff images for changed pages (default: true).
    pub generate_diff_images: bool,
}

impl Default for DiffOptions {
    fn default() -> Self {
        Self {
            similarity_threshold: 0.98,
            generate_diff_images: true,
        }
    }
}

/// Result of comparing a single page pair.
#[derive(Debug, Clone)]
pub struct PageDiff {
    /// Page index (0-based).
    pub page_index: usize,
    /// SSIM similarity score (0.0 = completely different, 1.0 = identical).
    pub similarity: f64,
    /// Whether this page is considered changed (below threshold).
    pub changed: bool,
    /// Diff image (RGBA pixels) with red overlay on changed regions.
    /// Only present if `generate_diff_images` is true and `changed` is true.
    pub diff_image: Option<PageImage>,
}

/// Result of comparing two PDF documents.
#[derive(Debug, Clone)]
pub struct DiffResult {
    /// Per-page comparison results (for pages present in both documents).
    pub pages: Vec<PageDiff>,
    /// Indices of pages only in document B (added pages).
    pub added_pages: Vec<usize>,
    /// Indices of pages only in document A (removed pages).
    pub removed_pages: Vec<usize>,
    /// Overall similarity (average SSIM across common pages).
    pub overall_similarity: f64,
}

/// Compare two sets of rendered page images.
///
/// Pages are matched by index. Extra pages in either set are reported
/// as added or removed.
///
/// # Example
/// ```
/// use pdf_diff::{compare_pages, PageImage, DiffOptions};
///
/// let white = vec![255u8; 32 * 32 * 4];
/// let page_a = PageImage::new(32, 32, white.clone()).unwrap();
/// let page_b = PageImage::new(32, 32, white).unwrap();
///
/// let result = compare_pages(&[page_a], &[page_b], &DiffOptions::default());
/// assert!((result.overall_similarity - 1.0).abs() < 1e-6);
/// ```
pub fn compare_pages(
    pages_a: &[PageImage],
    pages_b: &[PageImage],
    options: &DiffOptions,
) -> DiffResult {
    let common_count = pages_a.len().min(pages_b.len());
    let mut pages = Vec::with_capacity(common_count);
    let mut total_ssim = 0.0;

    for i in 0..common_count {
        let a = &pages_a[i];
        let b = &pages_b[i];

        let similarity =
            ssim::compute_ssim(&a.pixels, a.width, a.height, &b.pixels, b.width, b.height);
        let changed = similarity < options.similarity_threshold;

        let diff_image = if changed && options.generate_diff_images {
            let out_w = a.width.min(b.width);
            let out_h = a.height.min(b.height);
            if out_w > 0 && out_h > 0 {
                let diff_pixels =
                    ssim::generate_diff(&a.pixels, a.width, &b.pixels, b.width, out_w, out_h);
                Some(PageImage {
                    width: out_w,
                    height: out_h,
                    pixels: diff_pixels,
                })
            } else {
                None
            }
        } else {
            None
        };

        total_ssim += similarity;
        pages.push(PageDiff {
            page_index: i,
            similarity,
            changed,
            diff_image,
        });
    }

    let added_pages: Vec<usize> = (common_count..pages_b.len()).collect();
    let removed_pages: Vec<usize> = (common_count..pages_a.len()).collect();

    let overall_similarity = if common_count > 0 {
        total_ssim / common_count as f64
    } else {
        0.0
    };

    DiffResult {
        pages,
        added_pages,
        removed_pages,
        overall_similarity,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_solid_page(w: u32, h: u32, r: u8, g: u8, b: u8) -> PageImage {
        let pixels = vec![r, g, b, 255].repeat((w * h) as usize);
        PageImage::new(w, h, pixels).unwrap()
    }

    fn make_gradient_page(w: u32, h: u32) -> PageImage {
        let mut pixels = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let v = ((x + y) % 256) as u8;
                pixels.extend_from_slice(&[v, v, v, 255]);
            }
        }
        PageImage::new(w, h, pixels).unwrap()
    }

    #[test]
    fn identical_pages_have_similarity_1() {
        let page = make_gradient_page(64, 64);
        let result = compare_pages(&[page.clone()], &[page], &DiffOptions::default());
        assert_eq!(result.pages.len(), 1);
        assert!(
            (result.pages[0].similarity - 1.0).abs() < 1e-6,
            "Expected ~1.0, got {}",
            result.pages[0].similarity
        );
        assert!(!result.pages[0].changed);
        assert!(result.pages[0].diff_image.is_none());
    }

    #[test]
    fn different_pages_have_low_similarity() {
        let white = make_solid_page(64, 64, 255, 255, 255);
        let black = make_solid_page(64, 64, 0, 0, 0);
        let result = compare_pages(&[white], &[black], &DiffOptions::default());
        assert!(
            result.pages[0].similarity < 0.1,
            "Expected low similarity, got {}",
            result.pages[0].similarity
        );
        assert!(result.pages[0].changed);
        assert!(result.pages[0].diff_image.is_some());
    }

    #[test]
    fn diff_image_has_correct_dimensions() {
        let white = make_solid_page(64, 64, 255, 255, 255);
        let black = make_solid_page(64, 64, 0, 0, 0);
        let result = compare_pages(&[white], &[black], &DiffOptions::default());
        let diff = result.pages[0].diff_image.as_ref().unwrap();
        assert_eq!(diff.width, 64);
        assert_eq!(diff.height, 64);
        assert_eq!(diff.pixels.len(), 64 * 64 * 4);
    }

    #[test]
    fn added_pages_detected() {
        let page = make_gradient_page(32, 32);
        let result = compare_pages(
            &[page.clone()],
            &[page.clone(), page],
            &DiffOptions::default(),
        );
        assert_eq!(result.pages.len(), 1);
        assert_eq!(result.added_pages, vec![1]);
        assert!(result.removed_pages.is_empty());
    }

    #[test]
    fn removed_pages_detected() {
        let page = make_gradient_page(32, 32);
        let result = compare_pages(
            &[page.clone(), page.clone(), page],
            &[make_gradient_page(32, 32)],
            &DiffOptions::default(),
        );
        assert_eq!(result.pages.len(), 1);
        assert!(result.added_pages.is_empty());
        assert_eq!(result.removed_pages, vec![1, 2]);
    }

    #[test]
    fn empty_documents() {
        let result = compare_pages(&[], &[], &DiffOptions::default());
        assert!(result.pages.is_empty());
        assert!(result.added_pages.is_empty());
        assert!(result.removed_pages.is_empty());
        assert_eq!(result.overall_similarity, 0.0);
    }

    #[test]
    fn overall_similarity_averaged() {
        let page = make_gradient_page(32, 32);
        let result = compare_pages(
            &[page.clone(), page.clone()],
            &[page.clone(), page],
            &DiffOptions::default(),
        );
        assert!(
            (result.overall_similarity - 1.0).abs() < 1e-6,
            "Expected ~1.0, got {}",
            result.overall_similarity
        );
    }

    #[test]
    fn no_diff_images_when_disabled() {
        let white = make_solid_page(64, 64, 255, 255, 255);
        let black = make_solid_page(64, 64, 0, 0, 0);
        let opts = DiffOptions {
            generate_diff_images: false,
            ..Default::default()
        };
        let result = compare_pages(&[white], &[black], &opts);
        assert!(result.pages[0].changed);
        assert!(result.pages[0].diff_image.is_none());
    }

    #[test]
    fn custom_threshold() {
        let page = make_gradient_page(32, 32);
        let opts = DiffOptions {
            similarity_threshold: 1.01, // Nothing can pass this threshold
            ..Default::default()
        };
        let result = compare_pages(&[page.clone()], &[page], &opts);
        assert!(result.pages[0].changed); // Even identical pages are "changed"
    }

    #[test]
    fn different_sized_pages() {
        let small = make_gradient_page(32, 32);
        let big = make_gradient_page(64, 64);
        let result = compare_pages(&[small], &[big], &DiffOptions::default());
        // Should compare the overlapping 32x32 region.
        assert!(result.pages[0].similarity > 0.5);
        if let Some(diff) = &result.pages[0].diff_image {
            assert_eq!(diff.width, 32);
            assert_eq!(diff.height, 32);
        }
    }

    #[test]
    fn page_image_validation() {
        let result = PageImage::new(0, 10, vec![]);
        assert!(result.is_err());

        let result = PageImage::new(2, 2, vec![0; 15]); // Wrong size
        assert!(result.is_err());

        let result = PageImage::new(2, 2, vec![0; 16]); // Correct
        assert!(result.is_ok());
    }
}
