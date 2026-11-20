<!-- section: Fixed -->
- An entry added to or removed from a multi-line Rust `use a::{ .. };` or
  `pub use a::{ .. };` declaration no longer seeds a `static_unknown` probe;
  it is import text, like the single-line form. A `pub(crate) use` re-export
  keeps its probes, and a declaration that does not close cleanly marks
  nothing (#8389).
