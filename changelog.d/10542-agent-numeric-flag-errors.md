<!-- section: Fixed -->
- `ripr agent brief --max-seams`, `ripr agent verify-execute --cancel-after-ms`, and `ripr agent stub --at` numeric refusals now name the flag, the expected shape, and the typed value (`--max-seams requires a positive integer; got "many"`) instead of appending Rust-internal `ParseIntError` text such as `invalid digit found in string` (#10542).
