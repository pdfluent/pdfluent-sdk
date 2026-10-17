//! Many documents as markdown in one process, optionally over several threads
//! (#623's fair clock).
//!
//! The structured benchmark timed our column as one process per page, start-up
//! included, against competitors that ran as one batch with their start-up
//! amortised -- two different measurements in one table. This is the batch,
//! so that the next round can time ours the way it times theirs: serial, and
//! with pages shared over threads.
//!
//! Hidden rather than published: it exists for the benchmark's example
//! (`examples/structured_markdown.rs --batch`), and what a batch API should
//! look like for callers is a separate question.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use crate::{ExtractOptions, PdfDocument};

/// One document's result in a batch: its markdown or why there is none, and
/// the wall time it took on its own thread.
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct BatchItem {
    /// The markdown, or the error that stood in its way.
    pub markdown: Result<String, String>,
    /// Milliseconds this document took on its thread.
    pub ms: u128,
}

/// [`markdown_of_bytes`], with the caller's options: the window the text
/// pass reads and the marker hook a test of the fallback sets.
///
/// Always the rig's markdown: the rig measures what a caller of the
/// default gets.
#[doc(hidden)]
pub fn markdown_of_bytes_with_options(
    bytes: Vec<u8>,
    options: &ExtractOptions,
) -> Result<String, String> {
    let doc = PdfDocument::open(bytes).map_err(|err| format!("open: {err}"))?;
    let (blocks, rules) = doc
        .extract_blocks_and_rules(options)
        .map_err(|err| format!("blocks: {err}"))?;
    Ok(crate::markdown::to_markdown_with_rules(&blocks, &rules))
}

/// One document as markdown: the call `structured_markdown <pdf>` makes.
///
/// The default caller's markdown, the same `extract_markdown` writes with
/// default options, so the rig measures exactly what a caller of the
/// default gets.
#[doc(hidden)]
pub fn markdown_of_bytes(bytes: Vec<u8>) -> Result<String, String> {
    markdown_of_bytes_with_options(bytes, &ExtractOptions::default())
}

/// Every input through [`markdown_of_bytes`], over `jobs` threads, in input
/// order. `read` fetches one input's bytes, so a caller decides where they
/// come from. The same markdown whatever `jobs` is: each document is read by
/// one thread from beginning to end, and nothing is shared between them.
#[doc(hidden)]
pub fn batch_markdown<F>(count: usize, jobs: usize, read: F) -> Vec<BatchItem>
where
    F: Fn(usize) -> Result<Vec<u8>, String> + Sync,
{
    let next = AtomicUsize::new(0);
    let done: Mutex<Vec<(usize, BatchItem)>> = Mutex::new(Vec::with_capacity(count));
    std::thread::scope(|scope| {
        for _ in 0..jobs.max(1) {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= count {
                    break;
                }
                let started = Instant::now();
                let markdown = read(i).and_then(markdown_of_bytes);
                let item = BatchItem {
                    markdown,
                    ms: started.elapsed().as_millis(),
                };
                if let Ok(mut done) = done.lock() {
                    done.push((i, item));
                }
            });
        }
    });
    let mut done = done.into_inner().unwrap_or_default();
    done.sort_by_key(|(i, _)| *i);
    done.into_iter().map(|(_, item)| item).collect()
}
