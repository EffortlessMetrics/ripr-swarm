<!-- section: Fixed -->
- Rust: `#[test]` functions written literally inside a same-file macro that only
  re-emits its input (`macro_rules! suite { ($($t:tt)*) => { $($t)* }; }` then
  `suite! { #[test] fn .. }`) are now indexed as tests of the invoking file, so
  the code they call no longer reads as unreached. Any other macro, an imported
  or later-defined one, metavariables, `include!`, nested macro items, raw
  identifiers and `cfg` gates stay unindexed (RIPR-SPEC-0117, #8999).
