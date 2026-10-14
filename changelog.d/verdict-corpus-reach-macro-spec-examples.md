<!-- section: Internal / CI -->
- The Rust verdict corpus gains 15 authored cases, each in its own crate, for
  numbered acceptance examples of the reach, macro and miss-evidence specs
  that no case cited. Each case records its examples in `spec_examples`:
  RIPR-SPEC-0114 examples 2, 4 and 6; 0115 examples 1 to 3; 0117 examples 1 to
  4; 0118 examples 2 and 4; 0119 examples 1 and 2; 0120 examples 1 and 2; and
  0125 example 1. Spec-example coverage rises from 71 to 88, and the floor rises
  with it. The 0118 example 3 and 0224 example 1 cases are analogues whose
  edits mean something different from the spec's, so they do not claim those
  examples. 0114 example 3 (a crate with no tests) is waived, because
  `verdict-corpus relabel` needs at least one test to run. The labels come from
  41 mutants run against each crate's own `cargo test` and replayed with
  `verdict-corpus relabel`. On this base ripr reads 4 cases ideally and abstains
  on 11, with no false actionable verdict. One abstention,
  `spec0115-no-test-reaches-owner`, carries a
  `no_static_path_with_related_tests` contradiction (#6580).
