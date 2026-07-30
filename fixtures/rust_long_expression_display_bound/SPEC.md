# Fixture: rust_long_expression_display_bound

Spec: RIPR-SPEC-0001

## Given

`blocking_reason` computes its whole result in one long chained expression —
several `.iter().filter(...).count()` calls plus a fallback, all on a single
source line well past 180 characters. One `#[test]` covers the
single-blocking-decision case with an exact `assert_eq!`.

This is the ordinary shape of the code that exposed the defect: a real-world
one-line summary expression (a chained iterator, a jq pipeline, a shell
heredoc), not an artificially padded string.

## When

The diff widens the branch structure from two arms to three, so both the
`before` and `after` sides of the probe are long single-line expressions.

## Then

`ripr check` reports the finding, and the human `Changed` block renders
`before` and `after` **bounded to the display budget with a trailing `…`**
rather than at full source width. No rendered `before`/`after`/`expr` line in
either `--format human` or `--format human-full` exceeds the budget plus its
field label.

The complete, untruncated expression remains available in `--format json`,
which is the lossless surface.

## Must Not

- Must not render the `before`/`after` expression at full source width; a 400+
  character line inside output whose every other line stays under ~100 makes
  the first read of a finding unusable (#2752).
- Must not truncate so aggressively that the leading source text no longer
  identifies which expression changed.
- Must not drop the field label, the `…` marker, or the one-line-per-field
  shape of the `Changed` block.
- Must not change the classification, evidence, or confidence for this finding:
  the display bound is a rendering concern only.
