//! Page-range parsing for `--pages`, `--every` and friends.
//!
//! Users count pages from 1 (`1-3,5`); the facade counts extraction windows
//! from 0. This module is the single place that translation happens, so a
//! fencepost error has exactly one home.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/// What went wrong parsing a page spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecError {
    /// The text is not a page spec at all (`--pages abc`). Usage error.
    Syntax(String),
    /// Well-formed, but names pages the document does not have. Input error.
    OutOfRange(String),
}

/// Parse `1-3,5` into sorted, deduplicated 1-based page numbers, checked
/// against `page_count`.
pub(crate) fn parse_list(spec: &str, page_count: usize) -> Result<Vec<usize>, SpecError> {
    let mut pages = Vec::new();
    for item in parse_ranges(spec)? {
        for page in item.0..=item.1 {
            pages.push(page);
        }
    }
    pages.sort_unstable();
    pages.dedup();
    if let Some(&last) = pages.last() {
        if last > page_count {
            return Err(SpecError::OutOfRange(format!(
                "page {last} is outside this document (1–{page_count}; pages count from 1)"
            )));
        }
    }
    Ok(pages)
}

/// Parse `1-3,5` into 1-based inclusive ranges, in the order written.
/// Each item is one range; `5` is `(5, 5)`. No document check here — the
/// caller applies the ranges and maps facade errors, or checks the top.
pub(crate) fn parse_ranges(spec: &str) -> Result<Vec<(usize, usize)>, SpecError> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err(SpecError::Syntax(
            "empty page spec (want e.g. `1-3,5`)".to_string(),
        ));
    }
    let mut out = Vec::new();
    for raw in spec.split(',') {
        let item = raw.trim();
        if item.is_empty() {
            return Err(SpecError::Syntax(format!(
                "empty item in page spec `{spec}` (want e.g. `1-3,5`)"
            )));
        }
        let (lo, hi) = match item.split_once('-') {
            Some((a, b)) => (parse_num(a, spec)?, parse_num(b, spec)?),
            None => {
                let n = parse_num(item, spec)?;
                (n, n)
            }
        };
        if lo == 0 || hi == 0 {
            return Err(SpecError::Syntax(format!(
                "pages count from 1, so `0` names nothing (in `{spec}`)"
            )));
        }
        if lo > hi {
            return Err(SpecError::Syntax(format!(
                "reversed range `{item}` in `{spec}` (want `lo-hi`)"
            )));
        }
        out.push((lo, hi));
    }
    Ok(out)
}

fn parse_num(text: &str, spec: &str) -> Result<usize, SpecError> {
    text.trim()
        .parse::<usize>()
        .map_err(|_| SpecError::Syntax(format!("`{text}` is not a page number (in `{spec}`)")))
}

/// Check the top of a range list against a document: the highest page named.
pub(crate) fn check_top(ranges: &[(usize, usize)], page_count: usize) -> Result<(), SpecError> {
    if let Some(top) = ranges.iter().map(|r| r.1).max() {
        if top > page_count {
            return Err(SpecError::OutOfRange(format!(
                "page {top} is outside this document (1–{page_count}; pages count from 1)"
            )));
        }
    }
    Ok(())
}

/// Split `page_count` pages into chunks of `every` (`--every N`).
pub(crate) fn chunks(page_count: usize, every: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 1;
    while start <= page_count {
        out.push((start, (start + every - 1).min(page_count)));
        start += every;
    }
    out
}

/// The complement of `delete` (sorted, deduplicated 1-based) within
/// `1..=page_count`, as merged contiguous ranges. Empty when everything
/// is deleted.
pub(crate) fn complement(page_count: usize, delete: &[usize]) -> Vec<(usize, usize)> {
    let mut keep: Vec<usize> = (1..=page_count).filter(|p| !delete.contains(p)).collect();
    keep.sort_unstable();
    let mut ranges = Vec::new();
    let mut iter = keep.into_iter();
    let Some(mut start) = iter.next() else {
        return ranges;
    };
    let mut prev = start;
    for page in iter {
        if page == prev + 1 {
            prev = page;
        } else {
            ranges.push((start, prev));
            start = page;
            prev = page;
        }
    }
    ranges.push((start, prev));
    ranges
}

/// Check that `order` is a permutation of `1..=page_count` (`--reorder`).
pub(crate) fn check_permutation(
    order: &[usize],
    page_count: usize,
) -> Result<Vec<usize>, SpecError> {
    let mut sorted = order.to_vec();
    sorted.sort_unstable();
    let want: Vec<usize> = (1..=page_count).collect();
    if sorted != want {
        return Err(SpecError::OutOfRange(format!(
            "--reorder must list every page exactly once (this document has {page_count} page(s))"
        )));
    }
    Ok(order.to_vec())
}

/// Render a 1-based inclusive range for file names and messages.
pub(crate) fn render_range((lo, hi): (usize, usize)) -> String {
    if lo == hi {
        format!("{lo}")
    } else {
        format!("{lo}-{hi}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_parses_and_dedups() {
        assert_eq!(parse_list("1-3,5", 8).unwrap(), vec![1, 2, 3, 5]);
        assert_eq!(parse_list("3,1-2,2", 8).unwrap(), vec![1, 2, 3]);
        assert_eq!(parse_list(" 2 ", 8).unwrap(), vec![2]);
    }

    #[test]
    fn list_rejects_syntax() {
        assert!(matches!(parse_list("", 8), Err(SpecError::Syntax(_))));
        assert!(matches!(parse_list("abc", 8), Err(SpecError::Syntax(_))));
        assert!(matches!(parse_list("1-", 8), Err(SpecError::Syntax(_))));
        assert!(matches!(parse_list("0", 8), Err(SpecError::Syntax(_))));
        assert!(matches!(parse_list("3-1", 8), Err(SpecError::Syntax(_))));
        assert!(matches!(parse_list("1,,2", 8), Err(SpecError::Syntax(_))));
    }

    #[test]
    fn list_rejects_out_of_range() {
        assert!(matches!(
            parse_list("1-9", 8),
            Err(SpecError::OutOfRange(_))
        ));
    }

    #[test]
    fn every_chunks_cover_without_overlap() {
        assert_eq!(chunks(5, 2), vec![(1, 2), (3, 4), (5, 5)]);
        assert_eq!(chunks(4, 2), vec![(1, 2), (3, 4)]);
        assert_eq!(chunks(2, 9), vec![(1, 2)]);
        assert!(chunks(0, 2).is_empty());
    }

    #[test]
    fn complement_merges_kept_ranges() {
        assert_eq!(complement(5, &[2, 4]), vec![(1, 1), (3, 3), (5, 5)]);
        assert_eq!(complement(5, &[1, 2]), vec![(3, 5)]);
        assert!(complement(3, &[1, 2, 3]).is_empty());
        assert_eq!(complement(3, &[]), vec![(1, 3)]);
    }

    #[test]
    fn permutation_requires_every_page_once() {
        assert_eq!(check_permutation(&[3, 1, 2], 3).unwrap(), vec![3, 1, 2]);
        assert!(check_permutation(&[1, 1, 2], 3).is_err());
        assert!(check_permutation(&[1, 2], 3).is_err());
        assert!(check_permutation(&[1, 2, 4], 3).is_err());
    }
}
