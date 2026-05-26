//! Database and collections layer — Phase 2 target.
//!
//! Will replace src/common/{collection,image,tags,history,metadata,film}.c.
//! Phase 0: module stubs that define the public API surface only.

pub mod collection;
pub mod image;
pub mod tags;

pub use darkroom_sys::dt_imgid_t;
