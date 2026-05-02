//! Async I/O wrappers built on Tokio.

use std::path::Path;

use crate::error::internal_error;
use crate::Result;

/// Open a PDF from a filesystem path on Tokio's blocking pool.
///
/// ```
/// use std::path::PathBuf;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
///     .join("tests/fixtures/sample.pdf");
/// let runtime = tokio::runtime::Builder::new_current_thread().build()?;
/// let doc = runtime.block_on(pdfluent::async_io::open(fixture))?;
/// assert_eq!(doc.page_count(), 1);
/// # Ok(())
/// # }
/// ```
pub async fn open<P>(path: P) -> Result<crate::PdfDocument>
where
    P: AsRef<Path> + Send + 'static,
{
    let path = path.as_ref().to_path_buf();
    tokio::task::spawn_blocking(move || crate::PdfDocument::open(path))
        .await
        .map_err(|err| internal_error(format!("async-tokio open task failed: {err}")))?
}
