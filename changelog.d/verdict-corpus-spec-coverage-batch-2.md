<!-- section: Internal / CI -->
- The Rust verdict corpus gains 13 cases for numbered acceptance examples that
  no case covered yet. Spec-example coverage was 88 of 135 and is now 100 of
  130, with the floor raised to 100; the denominator shrinks because five
  examples are waived.
  - Nine authored crates cover RIPR-SPEC-0229 examples 1 to 8 on match-arm
    selection.
  - Two crates cover RIPR-SPEC-0117 examples 5 and 6 on proximity-only macro
    reach.
  - Two cases on the pinned semver 1.0.23 subject put the exact launch edits of
    RIPR-SPEC-0224 example 1 and RIPR-SPEC-0118 example 3 in the diff.

  Every label comes from mutants replayed with `verdict-corpus relabel`. The
  five waivers:
  - RIPR-SPEC-0229 example 9: both arms return the same value, so every edit
    is equivalent.
  - RIPR-SPEC-0233 examples 1, 3 and 4: each hinges on a test whose literal is
    wrong, so the unedited suite fails.
  - RIPR-SPEC-0233 example 2: a repo-scope seam-credit rule that a diff case
    cannot isolate.

  On this base ripr reads 8 of the new cases ideally, abstains on 3 and gives
  2 false actionable verdicts:
  - `spec0229-range-widened-moved-input`: credit for an input that moves
    between range arms is not implemented.
  - `spec0117-proximity-macro-plus-direct`: ripr keeps the gap as RIPR-SPEC-0117
    example 6 requires, but the macro-routed test it does not credit kills every
    mutant (#7544).
