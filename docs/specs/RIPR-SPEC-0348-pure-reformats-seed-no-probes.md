# RIPR-SPEC-0348: pure reformats seed no probes

Status: proposed

Owner: product / analysis

Created: 2026-10-22

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #9450 (ux(check): formatting-only rewraps and reindents seed probes and lead Start-here)

Linked PRs:

- None yet

Support-tier impact:

- None. Probe seeding for Rust diffs narrows; no exposure state, language
  support tier, or output schema changes. Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- No threshold, schema, or runner change. Golden fixtures whose diffs are
  pure reformats may lose findings; each such fixture must be reviewed
  individually rather than re-blessed in bulk.

## Problem

`ripr check` seeded probes for formatting-only edits. Witnessed on current
main with a scratch crate: splitting

```rust
pub fn add(a: i64, b: i64) -> i64 { a + b }
```

across three lines with identical tokens produced two probes — one exposed,
one `static_unknown` — and Start-here led with "limited by static analysis".
`cargo fmt`-style rewraps are routine in real pull requests, so findings on
them train developers to ignore the tool.

The behavior-neutral machinery in
`crates/ripr/src/analysis/probes/diff.rs` almost covered this:
`is_behavior_neutral_rewrap` joins an uneven replacement block's token
streams, but deliberately required an actual respelling, leaving pure
reformats "to the rest of the analysis". Nothing downstream neutralizes a
pure rewrap: `identical_replacement_lines` requires byte-identical lines,
which a split or join never produces. The 1:1 path,
`is_behavior_neutral_respelling`, trims both sides and then requires the
trimmed texts to differ, so a reindent-only replaced line also seeded a
probe.

## Behavior

### Rule 1: an uneven replacement block with an identical joined token stream is neutral

Before: `is_behavior_neutral_rewrap` returned true only when some line also
respelled (`core::i32::MAX` -> `i32::MAX`, `Chunk` -> `Chunk<'_>`).
After: it returns true whenever the two sides join — under the existing
`join_tokens` normalization — to the same `neutral_spelling` result,
including when they join to byte-identical text (a pure reformat). No probe
is seeded on either side of the block.

`join_tokens` drops spaces only at wrap points, so a one-line body split at
its braces (`{ a + b }` over three lines) needs one more comparison: both
joined sides with bracket-adjacent spaces squeezed (`{a + b}`) must also
agree. That squeezed comparison fails closed when any line in the block
holds a single quote, because a space can be character-literal content;
interior spacing elsewhere is kept, so `a & &b` and `a && b` still differ.

### Rule 2: a 1:1 replaced pair differing only in surrounding whitespace is neutral

Before: `is_behavior_neutral_respelling` returned false for trim-equal
pairs. After: a pair whose raw texts differ while their trimmed texts are
equal is a pure reindent and is neutral. A byte-identical pair still returns
false there; identical lines are suppressed by the separate
`identical_replacement_lines` path, and the respelling predicate keeps its
"some respelling actually differs" meaning for its unit contract.

### Rule 3: every existing fail-closed guard is unchanged

Neutrality still requires all of:

- block length within the existing `MAX_REWRAP_LINES` bound on both sides;
- no line on either side holding a quote, comment marker, or
  token-observing macro (`stringify!`, `assert!`, `dbg!`, ...), judged by
  the existing per-line prose check plus the caller's whole-file
  `prose_lines` sets (multi-line string continuations and token-macro
  continuation lines stay probed);
- the `join_tokens` token-boundary conservatism: `a & &b` does not join to
  `a && b`, separated range dots stay apart, word boundaries are kept;
- no line holding a position-observing macro (`line!`, `column!`): each
  expands from the source position it sits at, so moving the line moves
  its value even when every token is unchanged. The caller's whole-file
  `prose_lines` sets carry each invocation's full span (name through
  closing delimiter), so a name and its `!` split across lines — where no
  single line names the macro — is guarded too;
- no line holding a whitespace character literal (`' '` or a real tab
  between quotes): joining collapses whitespace runs and would erase the
  literal's content from the comparison;
- neither comparison ever merges two puncts into one joint punct that
  macro matching distinguishes: the squeezed comparison touches only
  delimiters (a delimiter bounds its token group, so spacing beside it is
  invisible even in a token tree), and `join_tokens` keeps a wrap-point
  space before `,`/`;` when the preceding character is another punct
  (`-` `,` never becomes `-,`); numeric-dot wrap points retain their
  whitespace so separate `1` / `.0` or `1.` / `0` tokens never become a
  floating-point literal. Ambiguous numeric-dot contexts stay probed.

Known residual: position sensitivity that no token names — a
`#[track_caller]` callee returning `Location::caller()`, or the location a
`panic!` captures — still reads as neutral under a rewrap. Detecting it
needs call-graph attribute resolution, which this claim does not build;
the guards above cover the position-observing macros whose values are
computed directly from source position.

A token change inside a rewrap (`a + b` -> `a + c`) still seeds its probe.

## Required Evidence

- Unit: `is_behavior_neutral_rewrap` accepts `["let a = b;"]` against
  `["let a =", "b;"]` (was rejected as a pure reformat).
- Unit: `is_behavior_neutral_respelling` accepts a reindent-only pair and
  still rejects the byte-identical pair.
- Block level: a one-line function split across three lines, and the reverse
  join, seed no probes; the same split with one token changed still seeds a
  probe.
- Live: the scratch-crate witness in #9450 reports no findings for the
  formatting-only edit.

## Non-Goals

- No change to probe classification, exposure states, or Start-here ranking.
- No whole-file formatting detection; only the diff block already judged.
- Blocks beyond `MAX_REWRAP_LINES` stay probed.

## Acceptance Examples

1. `pub fn add(a: i64, b: i64) -> i64 { a + b }` split over three lines:
   no probes, no findings.
2. The reverse join of a three-line body into one line: no probes.
3. The same split with `a + b` changed to `a + c`: the probe is kept.
4. A line changed only by indentation inside an even replacement run:
   no probe.
5. A rewrap whose block contains a string literal or a `stringify!`
   continuation: still probed.

## Test Mapping

- `crates/ripr/src/analysis/probes/diff.rs`:
  `a_pure_reformat_rewrap_seeds_no_probe_but_a_changed_token_does`
  (examples 1-4 at block level),
  `a_function_body_split_at_its_braces_seeds_no_probe` (example 1),
  `rewrap_squeezes_bracket_spaces_but_fails_closed_on_quotes` (Rule 1
  squeezed comparison and the single-quote fail-closed),
  `rewrap_normalization_keeps_word_boundaries` (pure-reformat acceptance
  plus word-boundary and operator controls),
  `position_literal_and_joint_punct_guards_keep_reformats_probed`
  (position-macro, whitespace-literal, and joint-punct controls),
  `a_split_position_macro_keeps_its_rewrap_probed` and
  `a_reindented_split_position_macro_keeps_its_probe` (a name and `!`
  split across lines stays probed through the rewrap and reindent
  paths).
- Existing guards keep their tests: `a_respelling_inside_stringify_is_not_neutral`,
  `a_prose_line_on_one_side_keeps_a_shifted_rewrap_probed`,
  `rewrap_keeps_separated_dots_apart` (example 5 family).
- `numeric_dot_split_and_join_keep_their_probes` keeps changed numeric-dot
  tokens probed in both directions through the production block route.

## Implementation Mapping

- `crates/ripr/src/analysis/probes/diff.rs`: `is_behavior_neutral_rewrap`
  drops the respelling requirement and adds the bracket-squeezed
  comparison via `squeeze_bracket_spaces`; `is_behavior_neutral_respelling`
  treats raw-differing, trim-equal pairs as neutral. Call sites in
  `probes_for_file_with_relations` are unchanged.

## Metrics

- `unit_test_pass_rate`: the mapped tests pass. Target: 100 percent.
