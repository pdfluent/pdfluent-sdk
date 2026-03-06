#![no_main]

use libfuzzer_sys::fuzz_target;
use pdf_syntax::content::TypedIter;

fuzz_target!(|data: &[u8]| {
    // Fuzz the content stream parser directly.
    // Content streams contain drawing operators (text, paths, images).
    let mut iter = TypedIter::new(data);
    while let Some(_instruction) = iter.next() {}
});
