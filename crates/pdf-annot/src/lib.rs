//\! PDF annotation engine.
//\!
//\! Provides typed access to all annotation types defined in ISO 32000-2 §12.5.

mod annotation;
mod appearance;
mod geometric;
mod link;
mod markup;
mod stamp;
mod types;

pub use annotation::*;
pub use appearance::*;
pub use geometric::*;
pub use link::*;
pub use markup::*;
pub use stamp::*;
pub use types::*;
