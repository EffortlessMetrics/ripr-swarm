<!-- section: Internal / CI -->
- The Rust verdict corpus gains eight runtime-labeled cases for RIPR-SPEC-0236
  examples 16 to 19 and 23 to 26 (predicate-boundary seams reached through a
  `Display` impl, a direct assert, an unasserted caller, an integration-test
  chain, a macro witness, a `src/` test-module chain, a closure in a `static`
  and an owner no test reaches). The spec's seam-table, pilot and test-name
  examples (1 to 15, 20 to 22 and 27) are waived with reasons, so the spec is
  fully accounted for. Spec-example coverage moves from 112 of 144 to 120 of
  125, and the ledger floor rises to 120 (#5411).
