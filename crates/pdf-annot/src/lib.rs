#![warn(missing_docs)]
//! PDF annotation engine.
//!
//! Provides typed access to all annotation types defined in ISO 32000-2 §12.5,
//! and annotation creation via the `write` feature (backed by lopdf).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

mod annotation;
mod appearance;
pub mod appearance_writer;
pub mod builder;
pub mod error;
#[cfg(feature = "write")]
pub mod flatten;
mod geometric;
mod link;
mod markup;
mod stamp;
mod types;

pub use annotation::*;
pub use appearance::*;
#[cfg(feature = "write")]
pub use flatten::flatten_annotations;
pub use geometric::*;
pub use link::*;
pub use markup::*;
pub use stamp::*;
pub use types::*;
