<!-- section: Added -->
- The Rust verdict corpus gains 14 cases for numbered acceptance examples that
  no case covered yet, and spec-example coverage rises from 88 to 101 of 131,
  with the floor raised to match.
  - Nine authored crates cover RIPR-SPEC-0229 examples 1 to 8 on match-arm
    selection.
  - Two crates cover RIPR-SPEC-0117 examples 5 and 6 on proximity-only macro
    reach.
  - One crate covers RIPR-SPEC-0233 example 2 on a consistent boundary literal.
  - Two cases on the pinned semver 1.0.23 subject put the exact launch edits of
    RIPR-SPEC-0224 example 1 and RIPR-SPEC-0118 example 3 in the diff.
    `matches_greater` is byte-identical at the 0118 launch commit and at the
    corpus pin.

  Every label comes from mutants replayed with `verdict-corpus relabel`. Four
  examples are waived because no runtime truth exists for them:
  - RIPR-SPEC-0229 example 9: both arms return the same value, so every edit
    is equivalent.
  - RIPR-SPEC-0233 examples 1, 3 and 4: each hinges on a test whose literal
    is wrong, so the unedited suite fails.

  On this base ripr reads 9 of the new cases ideally, abstains on 3 and
  gives 2 false actionable verdicts:
  - `spec0229-range-widened-moved-input`: credit for an input that moves
    between range arms is not implemented.
  - `spec0117-proximity-macro-plus-direct`: a test that reaches the owner only
    through a macro kills every mutant, but ripr cannot see that test.
