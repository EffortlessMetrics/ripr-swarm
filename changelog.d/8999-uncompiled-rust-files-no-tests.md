<!-- section: Fixed -->
- A Rust file that no Cargo target can compile (a benchmark or corpus haystack
  outside `src/`, `tests/`, `benches/` and `examples/` that no manifest,
  `#[path]`, `include!` or `mod` names) is no longer listed as a related test,
  and its parser refusal no longer makes the run `partial_with_limitations`;
  any doubt keeps the file as before (#8999).
