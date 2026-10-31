<!-- section: Fixed -->
- Rust field assertions no longer receive credit when a plain `#[macro_use]`
  import is bound by the selected manifest to a local library with a literal
  unconditional export of the assertion macro. The refusal names the import
  and export while preserving benign standard-alias and non-assertion
  proc-macro imports; general macro expansion remains unsupported (#11810).
