<!-- section: Added -->
- Verdict corpus: two authored subjects add 14 runtime-labeled cases for test
  shapes mined from real crates (bytes, httparse, byteorder, rust-base64,
  uuid, toml_writer, rust-url) and written as our own code.
  `authored-mined-wire` covers a doc example as the only test and its
  `ignore`-fenced twin, an assert inside a closure in a macro-generated test, a
  bool-returning property checked over a sample loop, a round trip that cannot
  tell a symmetric key apart, and a lookup table with an asserted and an
  unasserted row. `authored-mined-codec` covers a production `debug_assert!`
  as the only oracle and its twin, a field set in a one-line struct literal
  and its multi-line twin, a trait impl reached only through a blanket impl,
  and a table loop with an expected-failures allowlist (a skipped row and an
  asserted row). Five of the nine discriminated cases score
  `false_actionable` today (218 cases). Authored false actionable moves from
  46/86 to 51/95 and the overall rate from 49/106 to 54/115; upstream rates are
  unchanged. Doctest names (`src/lib.rs - item (line N)`) now pass the
  one-test-name rule for `failing_test` (#6644).
