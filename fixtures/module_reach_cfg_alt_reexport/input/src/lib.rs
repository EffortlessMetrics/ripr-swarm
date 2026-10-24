#[cfg(any())]
pub mod inner;
#[cfg(not(any()))]
pub use std::cmp as inner;
