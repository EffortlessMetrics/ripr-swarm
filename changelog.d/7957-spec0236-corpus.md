<!-- section: Internal / CI -->
- The Rust verdict corpus gains ten runtime-labeled cases built from the
  RIPR-SPEC-0236 probes (predicate-boundary seams reached through a `Display`
  impl, a direct assert, an unasserted caller, an integration-test chain, the
  same chain with tests named `snapshot_checkout` and `mock_checkout`, a macro
  witness, a `src/` test-module chain, a closure in a `static` and an owner no
  test reaches). RIPR-SPEC-0236 itself is out of spec-example scope: its
  examples claim seam grip classes, evidence-record categories and pilot
  output, which corpus rows do not score, so the cases are uncited and
  coverage stays 112 of 117 (#7957).
