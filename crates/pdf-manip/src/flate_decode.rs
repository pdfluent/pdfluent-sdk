// `decode_zlib*` are only consumed by `downsample` and `font_subset`,
// which are feature-gated; without those features cargo flags them as
// dead code. The functions stay compiled so feature combinations don't
// have to be plumbed in.
#![allow(dead_code)]

use crate::error::{ManipError, Result};
use flate2::read::ZlibDecoder;
use std::io::Read;

/// Conservative default output cap for FlateDecode streams.
pub(crate) const MAX_DEFLATE_BYTES: u64 = 256 * 1024 * 1024;

pub(crate) fn decode_zlib(
    data: &[u8],
    io_error: impl FnOnce(std::io::Error) -> ManipError,
) -> Result<Vec<u8>> {
    decode_zlib_with_limit(data, MAX_DEFLATE_BYTES, io_error)
}

pub(crate) fn decode_zlib_with_limit(
    data: &[u8],
    max_deflate_bytes: u64,
    io_error: impl FnOnce(std::io::Error) -> ManipError,
) -> Result<Vec<u8>> {
    let decoder = ZlibDecoder::new(data);
    // Read one byte past the cap so exact-limit payloads remain valid while
    // any overflow maps to a dedicated error instead of truncating silently.
    let mut limited_decoder = decoder.take(max_deflate_bytes.saturating_add(1));
    let mut decoded = Vec::new();
    limited_decoder
        .read_to_end(&mut decoded)
        .map_err(io_error)?;
    if decoded.len() as u64 > max_deflate_bytes {
        return Err(ManipError::DecompressionLimitExceeded(max_deflate_bytes));
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    #[test]
    fn decode_zlib_errors_when_output_exceeds_limit() {
        let raw = vec![0u8; 8 * 1024];
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(&raw).unwrap();
        let compressed = encoder.finish().unwrap();

        assert!(compressed.len() < raw.len());

        let err = decode_zlib_with_limit(&compressed, 1024, |e| {
            ManipError::Other(format!("unexpected FlateDecode failure: {e}"))
        })
        .unwrap_err();

        assert!(matches!(err, ManipError::DecompressionLimitExceeded(1024)));
    }

    #[test]
    fn decode_zlib_allows_payload_at_exact_limit() {
        let raw = vec![0u8; 1024];
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(&raw).unwrap();
        let compressed = encoder.finish().unwrap();

        let decoded = decode_zlib_with_limit(&compressed, 1024, |e| {
            ManipError::Other(format!("unexpected FlateDecode failure: {e}"))
        })
        .unwrap();

        assert_eq!(decoded, raw);
    }
}
