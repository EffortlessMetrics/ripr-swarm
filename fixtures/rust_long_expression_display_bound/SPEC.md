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

`ripr check --format human-full` reports the finding, and its `Changed` block
renders `before` and `after` **bounded to the display budget by wrapping across
continuation lines** rather than at full source width. No rendered
`before`/`after`/`expr` line exceeds the budget plus its field label.

The fragment stays **complete**: wrapping preserves every character, so
stripping the label/indent column and rejoining the lines reproduces the
original expression exactly. Nothing is elided, and no `…` appears.

Completeness is the point rather than a nicety, because this is the only surface
that carries these values at all: `--format json` serializes `probe.expression`
only, never `probe.before` or `probe.after`. Truncating here would leave the
long preimage in no ripr output. This fixture's own `expected/check.json` shows
that — it contains the short predicate, not the long `before`.

The default `--format human` digest is a different contract: it shows one
selected finding, collapses and truncates for width, and routes the reader here.

### Printable Unicode source

The exhaustive source-fragment budget is 180 display cells including the
10-cell label or continuation indent. Printable Unicode wraps between extended
grapheme clusters: CJK characters use two cells, a combining accent stays with
its letter, and a joined emoji stays intact. The source payload keeps the
existing 170-cell budget on every original line, including later lines that
start at the left margin. A single cluster wider than the budget stays intact
as an exception so wrapping always preserves the source and makes progress.

The literal CLI cases in `crates/ripr/tests/cli_smoke/source_unicode.rs` exercise
the same owner with CJK, combining accents, emoji and ASCII, and the public
finding renderer checks the `before` and `expr` branches. Stripping each label
and continuation indent must reproduce the original source bytes. Expectations
are literal boundaries, independent of the renderer's width helpers (#7595).

This is the fixed source-fragment policy, not terminal-width discovery (#7442).
Tabs keep their previous chunking cost; tab stops and final control escaping
can expand displayed width. Terminal/font differences and other human sections
are outside this bound.

## Must Not

- Must not render the `before`/`after` expression at full source width; a 400+
  character line inside output whose every other line stays under ~100 makes
  the first read of a finding unusable (#2752).
- Must not elide any part of the expression in the full form — no `…`, no
  dropped tail. The wrapped block must reconstruct the original exactly.
- Must not normalize whitespace inside the fragment. A diff whose only delta is
  whitespace in a string literal (`"a  b"` → `"a b"`) must still render `before`
  and `after` differently.
- Must not drop the field label or the continuation indent that aligns wrapped
  lines under the value.
- Must not change the classification, evidence, or confidence for this finding:
  the display bound is a rendering concern only.
