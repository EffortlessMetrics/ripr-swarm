<!-- section: Internal / CI -->
- The Rust verdict corpus gains ten runtime-labeled cases for RIPR-SPEC-0236:
  examples 16 to 19 and 21 to 24 and 26 (predicate-boundary seams reached
  through a `Display` impl, a direct assert, an unasserted caller, an
  integration-test chain, the same chain with tests named `snapshot_checkout`
  and `mock_checkout`, a macro witness, a `src/` test-module chain and an
  owner no test reaches), plus an uncited closure-in-a-`static` case. The
  spec's stage-tuple and pilot examples (1 to 15, 20 and 27) and the latent
  unresolved-owner example 25 are waived with reasons, so the spec is fully
  accounted for. Spec-example coverage moves from 112 of 144 to 121 of 126,
  and the ledger floor rises to 121 (#7957).
