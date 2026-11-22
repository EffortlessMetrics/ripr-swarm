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
- #14760 (Start here picks a line that only uses a table renamed in the diff)
- #15931 (writing `Self` for the implementing type is reported as changed behavior)

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

### Rule 4: a line that only applies an in-file rename the diff establishes is neutral

A 1:1 replaced pair whose only differences are identifiers renamed at their
declaration in the same file's diff seeds no probe on either side, and the
declaration line itself seeds none (`static HEADER_NAME_MAP` ->
`static TOKEN_MAP` with `HEADER_NAME_MAP[b as usize]` -> `TOKEN_MAP[b as
usize]`). `crates/ripr/src/analysis/probes/identifier_rename.rs` owns the
decision. The rename is established only when all of these hold:

- exactly one `static`, `const`, `fn` or `type` declaration line changes
  only its name (`struct`/`enum` names reach derived `Debug`; `main` and
  foreign items inside an `extern "ABI" { .. }` block are bound by
  spelling);
- the whole item, through its item-level `;` or a `fn` body's closing `}`,
  is byte-identical after the respelling;
- the old name no longer appears in the new file's code and the new name
  did not appear in the old file's code;
- every attribute directly above the declaration, across blank lines, is
  name-blind (`inline`, `cfg`, `allow`, doc, ...);
- neither name is spelled after `.` or `::`, as a field, inside any macro
  invocation or `macro_rules!` body, or inside a `struct`/`enum`/`union`
  body, where the same spelling may name something else or be printed;
- neither file names an identifier starting with `stringify`,
  `concat_idents`, `type_name`, `paste` or `function_name`.

A use-site pair is neutral only when every differing identifier is an
established rename and the rest of the raw line is byte-identical. A
rename whose declaration and use sites sit in different files stays
probed.

### Rule 5: `Self` written for the implementing type's own name is neutral

Inside `impl Shell { .. }` or `impl Generator for Shell { .. }`, `Shell::Bash`
and `Self::Bash` name the same item (the `clippy::use_self` cleanup). A 1:1
replaced pair whose only differences trade `Self` with one type name seeds
no probe on either side, in both directions.
`crates/ripr/src/analysis/probes/self_type_respelling.rs` owns the decision
and reads both files with the syntax parser. The pair is neutral only when
all of these hold on each side:

- the two lines are byte-identical apart from the traded identifiers, and
  every trade on the line uses the same type name;
- the token is code, not comment, string or macro input (`dbg!(Shell::new())`
  prints its tokens), and is the unqualified first segment of a path with no
  generic arguments;
- the innermost enclosing `impl` implements exactly that bare name: not
  `Shell<T>`, `&Shell`, `Box<Shell>` or a qualified path;
- only the associated item that holds the token sits between it and that
  `impl`, no enclosing block declares an item or `use`, no generic
  parameter or binding pattern in that item spells the type name, and the
  item, the `impl` and every module around it carry only token-blind
  attributes (`inline`, `cfg`,
  `allow`, `doc`, ...), so no attribute macro receives the spelling;
- the scope that holds the `impl` (file, module or block) itself declares
  the name as a `struct`, `enum` or `union` without generic parameters, and
  declares or imports nothing else under that name. A defaulted parameter
  lets `Wrap::new()` infer what `Self::new()` fixes, and a type alias or a
  renamed import can hide one;
- a path that is the name alone in value position (`Shell(n)`, a `Shell(x)`
  pattern) additionally needs that declaration to be a tuple or unit
  `struct`. Beside a type alias, a function of the same name makes
  `Shell(n)` another item than `Self(n)`;
- both files parse without error and neither line holds a position macro.

Non-claim: a type declared in another file than its `impl` is not read, so
its respelling stays probed.

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
6. A table renamed at its declaration with its values unchanged and its use
   site respelled: no probe on either line; the diff's real change keeps
   its probe.
7. The same rename with a changed body, a pre-existing new name, a remaining
   old-name use, a name-observing attribute, a same-spelled field, method,
   variant or macro token: the use site keeps its probe.
8. `Shell::Bash =>` respelled `Self::Bash =>` inside `impl Generator for
   Shell`, and the reverse: no probe on either line; a real change in the
   same diff keeps its probe.
9. The same respelling for another type's name, with another token changed,
   inside `dbg!`, in an `impl` for `&Shell`, in a nested `impl`, under a
   shadowing generic parameter, under an attribute macro, outside any
   `impl`, for a type declared with generic parameters, behind an alias or
   an import, or in another file, or as the bare value `Shell(n)` where no
   tuple or unit `struct` beside the `impl` owns the name: still probed.

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
- `crates/ripr/src/analysis/probes/identifier_rename_tests.rs` (Rule 4, real
  Git fixtures): `a_proven_rename_seeds_no_probe_but_the_real_change_does`
  (example 6), `a_renamed_function_and_its_callers_seed_no_probe`,
  `a_use_site_that_changes_more_than_the_name_stays_probed`,
  `the_same_use_site_edit_without_a_proven_rename_is_probed` and
  `a_rename_that_may_not_mean_the_same_item_stays_probed` (example 7),
  `renaming_an_entry_point_is_never_proven`.
- `crates/ripr/tests/self_type_respelling.rs` (Rule 5,
  real Git fixtures):
  `self_for_the_implementing_type_seeds_no_probe_in_either_direction` and
  `a_real_change_beside_the_respelling_keeps_its_probe` (example 8),
  `a_spelling_the_files_do_not_prove_equivalent_stays_probed`,
  `a_name_the_scope_does_not_declare_as_a_plain_type_stays_probed` and
  `a_bare_value_path_needs_the_struct_that_owns_the_name` (example 9).

## Implementation Mapping

- `crates/ripr/src/analysis/probes/diff.rs`: `is_behavior_neutral_rewrap`
  drops the respelling requirement and adds the bracket-squeezed
  comparison via `squeeze_bracket_spaces`; `is_behavior_neutral_respelling`
  treats raw-differing, trim-equal pairs as neutral. Call sites in
  `probes_for_file_with_relations` are unchanged.
- `crates/ripr/src/analysis/probes/identifier_rename.rs`:
  `rename_only_lines` (Rule 4), applied in `probes_for_file_with_relations`
  beside the respelling and whitespace replacement-pair filters.
- `crates/ripr/src/analysis/probes/self_type_respelling.rs`:
  `self_type_respelled_lines` (Rule 5), applied in
  `probes_for_file_with_relations` after the rename filter.

## Metrics

- `unit_test_pass_rate`: the mapped tests pass. Target: 100 percent.
