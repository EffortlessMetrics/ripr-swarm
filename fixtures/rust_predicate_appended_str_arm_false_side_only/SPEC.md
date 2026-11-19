# Fixture: rust_predicate_appended_str_arm_false_side_only

Spec: RIPR-SPEC-0197

Owner: analysis-fixtures

Issue: #15030

## Given

The same edit as `rust_predicate_appended_str_arm_truth_assert`. The only
test asserts `assert!(!is_verbose("-q"))`, which passes on the old code and on
the new.

## When

`cargo xtask fixtures rust_predicate_appended_str_arm_false_side_only` runs
the public diff analyzer.

## Then

Neither finding reads `exposed`. `"-q"` leaves the appended arm false, so the
assertion cannot tell the two versions apart.

## Must Not

- Promote the bool-owner truthiness pin when the pinned input leaves the
  added arm at its identity value (#14194).
