<!-- section: Fixed -->
- Rust checks: an assertion in an inline const block inside a function
  (`const { assert!(N <= 249) };`) is a build-time check, so it no longer
  reads as a runtime predicate with a `predicate_boundary` gap ID and
  boundary-input advice. The line keeps one `static_unknown` finding (#14181).
