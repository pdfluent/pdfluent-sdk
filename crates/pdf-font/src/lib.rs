//! PDF font handling: CFF/Type1 font parsing, CMap parsing, and PostScript scanning.
//!
//! This crate merges functionality from hayro-font, hayro-cmap, and hayro-postscript.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]
#![allow(missing_docs)]
#![allow(clippy::upper_case_acronyms)]

extern crate alloc;

pub mod cmap;
pub mod font;
pub mod postscript;

// Re-export key types at crate root for convenience
pub use font::*;
