<!-- section: Fixed -->
- Rust: a `to_string()` or formatting macro the test can never run (a literal `if false` / `while false` block, `#[cfg(any())]`, code after `return`, a closure or nested fn nothing calls) no longer counts as reaching a changed `Display`/`Debug` `fmt`; dropped results and unknown conditions still do (#8999, RIPR-SPEC-0297).
