# RIPR-SPEC-0117: Rust Macro-Reach Limitation

Status: accepted

Owner: product / swarm

Created: 2026-06-18

Linked issues:

- [#1292](https://github.com/EffortlessMetrics/ripr-swarm/issues/1292)
- [#7071](https://github.com/EffortlessMetrics/ripr-swarm/issues/7071) (proximity-only weak reach)
- [#7542](https://github.com/EffortlessMetrics/ripr-swarm/issues/7542) (weak direct test beside a macro witness)

Linked PRs:

- This PR

Support-tier impact:

- No tier change. `docs/status/SUPPORT_TIERS.md` remains unchanged. This spec
  adds one additive `static_limit_kind` value, `rust_macro_reach_unresolved`,
  and one additive `stop_reasons` value, `macro_reach_unresolved`.
- Classification stays `no_static_path` (or `weakly_exposed` on the
  proximity-only or mixed weak-direct path below). This is a named limitation, not a
  coverage claim, test relation, repair packet, release-readiness claim, or
  macro-expansion engine.
- No `schema_version` bump is required because `static_limit_kind` and
  `stop_reasons` already exist as additive optional finding metadata.

Policy impact:

- Register this spec in `policy/doc-artifacts.toml`.
- Register the new stop reason in `policy/output_contracts.txt`.
- Update `docs/OUTPUT_SCHEMA.md` and `.ripr/traceability.toml`.
- No new crates, dependencies, binaries, workflow permissions, process
  execution, runtime macro expansion, rustc driver integration, or LSP surface.

## Problem

RIPR-SPEC-0114 names bounded Rust helper-call chains when a test calls an entry
point that lexically reaches the changed owner. It intentionally stops at
macros. In real Rust repos, tests and public entry points often route through
`macro_rules!` helpers. Today that can leave a first-run user with bare
`no_static_path`, even though RIPR saw a plausible entry path and the unresolved
edge is specifically a macro boundary.

The honest first-run behavior is to name that boundary without pretending the
macro was expanded:

```text
no_static_path + static_limit_kind: rust_macro_reach_unresolved
```

## Behavior

### Trigger condition

After direct related-test classification returns `no_static_path` with no
related tests (or `weakly_exposed` with only proximity relations; see
"Proximity-only weak reach" below), the Rust adapter checks the RIPR-SPEC-0114
bounded transitive witness first. If it finds no lexical path, it may emit the
macro-reach limitation when all of these are true:

1. A test calls a non-owner Rust entry symbol, or directly invokes a macro.
2. A bounded BFS from that entry symbol reaches a production function that
   invokes a macro, or the test directly invokes the macro.
3. A same-repo `macro_rules!` definition with that macro name is visible in the
   indexed source.
4. That macro definition lexically mentions the changed owner name as an
   identifier.

The mixed weak-direct route below reuses these macro-witness conditions with
its own eligibility. It searches only macros and does not require the lexical
transitive search to fail.

### When found

Set `Finding.static_limit_kind = Some(StaticLimitKind::RustMacroReachUnresolved)`.
Push `StopReason::MacroReachUnresolved` to `Finding.stop_reasons`.
Push an honest limitation message and a concrete witness pointer into
`Finding.evidence`.

The witness pointer names:

- the witnessing test file and line;
- the entry symbol the test called;
- the macro invocation site;
- the macro name.

The pointer uses candidate language:

```text
The macro path may lead here. Inspect it to judge whether this change is observed.
```

### When not found

Leave the finding exactly as before. Do not emit the limitation merely because a
test file contains an unrelated macro, an external macro is invoked, or a macro
definition does not mention the changed owner name.

### Test generators (#6649)

A test written through a same-file `macro_rules!` generator has no indexed
body, so condition 1 has no test to start from. When no indexed test yields a
macro edge, the witness falls back to an invocation of a generator. Every
condition below must hold; any doubt keeps the gap:

1. The generator has a single `macro_rules!` definition in the index, with
   exactly one arm, whose body lexically mentions the owner. With several arms
   ripr cannot tell which one an invocation selects. The arm's transcriber
   carries `#[test]` at its own top level, outside any `$(...)` repetition or
   nested block, is followed by a literal `fn` item, and names no `cfg`,
   `cfg_attr`, `ignore` or nested `macro_rules`: each can leave an invocation
   with no running test. No attribute in the transcriber may contain a `$`
   fragment (`#[$m]`, `$(#[$m])*`), and no `$` fragment may precede the
   `#[test]` (`$($p)*`, `# $a`), sit between the test's name and its body
   brace (`fn $n() $b`), or open its body (`{ $($t)* .. }`): each would let
   the call site pass `ignore` or a cfg.
2. The invocation is in the definition's file, below it and inside the block
   that holds the definition (`macro_rules!` is textually scoped; a
   `#[macro_use]` module that widens the scope is not followed), and outside
   any macro body. Body exclusion is by whole
   line, so an invocation sharing a line with a body's first or last line is
   skipped too.
3. At the invocation's byte offset every enclosing block opens a `mod`: Rust
   registers no test for an item nested in a function, impl or other block.
   An invocation inside any parentheses or brackets (`stringify!(case!(x))`)
   is a token, not an item, and is skipped. An invocation written with
   whitespace before `!` is not found, which keeps the gap.
4. Neither the invocation's leading attributes nor any enclosing `mod`
   header carries a cfg other than `cfg(test)`, and none carries
   `cfg_attr`. Any inner attribute (`#![..]`) in the file with a cfg other
   than `cfg(test)` withholds every generator witness in that file; a crate
   root's `#![cfg_attr(docsrs, ..)]` does too, which costs only the witness.

The scan masks comments and strings first, so a commented-out invocation or a
`#[test]` inside a string does not count. The witness names the test only
when the transcriber fixes its name: `fn fixed()` gives `fixed`, and
`fn $name()` gives the invocation's first identifier argument when `$name:ident`
is the matcher's first fragment and that argument is a plain identifier. In
every other shape it names "the test from `name!`" rather than publish a name
it could not read. It gives the invocation line and the definition line, and
says the test "is generated by" the macro. It sets the same `RustMacroReachUnresolved`
kind and stop reason, and adds no related test and no credit. Bounded
same-file expansion that credits these tests is #6720's work; this fallback
names the limit for single-arm shapes that expansion refuses (closure
arguments, literal-token matchers).

### Proximity-only weak reach (#7071)

A `weakly_exposed` finding faces the same unresolved negative when its reach is
`weak` and every related test is proximity-only (`same_test_file`,
`same_module` or `weak_token_substring`): no test is seen calling the owner,
only sharing its file or a name token. For that finding the Rust adapter runs
the RIPR-SPEC-0114 transitive witness, then this macro witness, and names the
first one found as above. The witness walks match names, as they do for
`no_static_path`, so a same-named function elsewhere can name the limit; that
costs a reported gap and never adds credit. Withheld dependent files are
searched as for `no_static_path`, without the over-limit disclosure. The class
stays `weakly_exposed`. The recommended next step names the witnessing test and
entry symbol and says the limitation does not establish a missing test; the
gap's missing-discriminator lines are dropped, and human output offers no
test-writing route. Direct reach remains established when a test calls the
owner; the separate mixed weak-direct rule below can still name an independent
unresolved macro path. The subprocess-binary and property-macro limits stay
`no_static_path`-only.

### Weak direct test beside a macro witness (#7542)

A weak direct oracle must not erase a possible test path behind a macro. The
adapter searches the existing macro witness when all of these hold:

- Classification is `weakly_exposed` and no producer has already named a limit.
- Reach is `yes`, but discrimination is not `yes`.
- An admitted related-test oracle row has `direct_owner_call`; an examined
  `assertion_not_observing` miss is evidence only and does not admit this rule.

This route searches only the macro witness. It does not broaden the
transitive-first proximity rule to ordinary lexical helper paths. Diff-scope
index narrowing widens for the same predicate, so a withheld dependent test
can supply the candidate macro witness. Repo analysis uses the shared rule.

If the dependent caller closure exceeds the search threshold and the main index
supplies no macro witness, use the existing `rust_transitive_reach_unresolved`
unsearched-route disclosure. Keep direct reach, all stages, classification and
related rows; withhold canonical gap/repair advice and missing discriminators.
The next step names the unsearched dependent files and how to raise the bounded
search limit. It names no test or macro witness from files that were skipped.
A concrete macro witness in the main index keeps its named macro limitation;
the generic unsearched detail must not overwrite it. Existing no-path and
proximity precedence remains unchanged.

If the witness exists, retain every RIPR stage, classification and related-test
row. Name the existing macro limit and concrete witness; add no reach or oracle
credit. Use the existing limitation next step, clear unsupported missing gap
advice and missing discriminators, and withhold canonical repair/verify/receipt
routes through the existing typed-limit policy. The macro scan does not establish
that the witness has a stronger assertion or observes this change; it only names
the unresolved path to inspect.

A confirmed discriminator remains outside this rule even if weak infection or
propagation leaves the overall class `weakly_exposed`. Weak direct tests without
a valid macro witness keep their previous result when the search completed.
Unrelated, absent, ambiguous
or uninvoked macros supply no witness.

### Fail-closed boundaries

- Never change classification from `no_static_path`, or from `weakly_exposed`
  for a proximity-only or mixed weak-direct finding.
- Never add the witness to `related_tests`.
- Never claim the test reaches, covers, tests, or exercises the changed owner.
- Only same-repo `macro_rules!` definitions are considered.
- The macro definition scan is lexical owner-name matching only; it is not macro
  expansion, type resolution, hygiene, trait dispatch, visibility analysis, or
  rustc integration.
- If the macro definition is absent, ambiguous, generated, external, or does not
  lexically mention the owner, do not emit this limitation.
- RIPR-SPEC-0119 refines direct test-body macro witnesses to
  `rust_macro_wrapped_test_call_unresolved`; production-entry macro boundaries
  keep `rust_macro_reach_unresolved`.

## Same-File Test Generators

A test file can define a `macro_rules!` whose transcriber emits `#[test] fn`
and invoke it once per case. Those tests are real, but the parser sees only
opaque token trees, so they were never indexed and the code they call read as
unreached: `ungripped` in repo exposure and `no_static_path` in diff mode,
a false gap when the tests catch the mutant (#5334).

ripr indexes such a test when the generator fits one bounded shape, and
otherwise indexes nothing for it:

- the definition is the only `macro_rules!` of that name in the file, sits
  directly in the file or an inline module, and precedes the invocation, which
  sits at item position in the same scope or a nested one; neither the
  definition nor the invocation carries an attribute or sits under a cfg'd
  module or file;
- every arm up to the selected one has a matcher of comma-separated
  `$name:fragment` metavariables only; the selected arm is the first whose
  metavariable count matches the invocation's top-level arguments, and an
  `ident`, `literal`, `block` or `tt` argument must have that shape;
- the selected transcriber contains `#[test]` and no call to another macro
  defined in the file, and no argument invokes one; its only repetition is
  the table form below;
- no `use` in the file imports the generator's name (rustc rejects that
  invocation as ambiguous), and the expansion carries no `cfg` other than
  `cfg(test)` and no `cfg_attr`.

A generator that is a table, itoa's `test! { name(value, expected) ... }`
(#6925), is the one repetition shape: the whole matcher is
`$( row ) sep? *` (or `+`), where a row is `$name:ident`, `literal`, `tt`
or `block` variables, literal `,`, `;` or `:` tokens, and delimited groups of
comma-separated metavariables (an `expr` outside a group needs a `,`, `;` or
`:` after it, or the row separator); the transcriber is exactly one
separator-free `$( body )*` over those variables. The invocation must split
into rows with nothing left over, and every fragment must fit as for a plain
arm. Each row becomes one expansion of the body, so a table of ten rows
indexes ten tests, up to 512 rows and 128 KiB of expansion text per
invocation; a larger table indexes nothing. Any other row layout (`=>`,
nested repetition, a trailing separator, a leftover token, a group of the
wrong arity) indexes nothing. A matcher that ends in a literal `,`, or an
invocation that ends in one the matcher does not take, indexes nothing for
that arm, because rustc would match a different arm. A row group is the same:
`edge(10,)` does not fit `($v:expr)`.

Each metavariable is replaced by its argument's source text and `$crate` by
`crate`; an `expr` or `literal` argument of more than one element (`-1`) is
wrapped in parentheses. Substitution is textual and ignores hygiene, so a
block or expression argument can name a binding the transcriber declares;
such a test reads as its unhygienic text does. The expansion is
parsed by the ordinary file-fact producer, and each `#[test]` function becomes
a test of the invoking file with every line pinned to the invocation.
Assertion admission (RIPR-SPEC-0197 owner pins and equality admission) runs
over the expansion text with the same rules as a hand-written test, so a
deferred or escaping assertion inside the transcriber is refused exactly as it
would be written by hand.

This is not macro expansion in general. Generators defined in another file,
`#[macro_use]` imports, repetition other than the table form, and procedural
macros stay unindexed, and
reach through them keeps the limitations above. A generated test whose
transcriber calls a helper macro from another file is indexed for reach, but
an assertion inside that helper is refused like any untrusted macro.

## Macro Definition Edits

A changed line that lies wholly inside a `macro_rules!` definition (matcher or
transcriber text, including `#`, `use` and `mod` lines that the transcriber
emits) is macro token text, not a Rust expression. The probe seeder emits one
`static_unknown` probe per definition and diff side (keyed by name and start
line) and no expression-family probe, so a matcher arm such as
`($args:ident) => {` is never graded `weakly_exposed`. A line that also holds
code outside the definition's byte extent keeps normal seeding, and lines in an
inline test module stay test evidence. Only `macro_rules!` is covered (including
raw-identifier names); macros 2.0 are not. An unclosed definition runs to the
end of the file, which is an approximation for an edit in progress.

## Wire Format

| Field | Value when fires | Value when not fires |
|---|---|---|
| `classification` | unchanged (`no_static_path`, or `weakly_exposed` on the proximity-only or mixed weak-direct path) | unchanged |
| `static_limit_kind` | `rust_macro_reach_unresolved` | omitted / unchanged |
| `stop_reasons` | includes `macro_reach_unresolved` | unchanged |
| `related_tests` | unchanged (witness is not added) | unchanged |
| `evidence` | limitation prose plus concrete macro witness pointer | unchanged |

## Non-Goals

- Full macro expansion.
- Promoting to `weakly_exposed`, `reachable_unrevealed`, or `exposed`.
- Emitting a repair packet.
- Inferring macro hygiene, generated items, trait dispatch, or visibility.
- Solving all macro-heavy Rust reachability cases.

## Acceptance Examples

1. **Macro-boundary limitation fires**: an integration test calls `outer()`;
   `outer()` invokes `call_inner!`; same-repo `macro_rules! call_inner` mentions
   changed owner `inner`. Result: `no_static_path` plus
   `static_limit_kind: rust_macro_reach_unresolved`.
2. **No owner mention**: `outer()` invokes `call_other!`, but the macro
   definition does not mention changed owner `inner`. Result: unchanged bare
   `no_static_path`.
3. **External macro**: `outer()` invokes a macro whose definition is not in the
   indexed repo source. Result: unchanged bare `no_static_path`.
4. **Lexical transitive path exists**: the RIPR-SPEC-0114 witness fires first.
   Result remains a transitive/public-API limitation, not macro-reach. For
   integration-test witnesses, RIPR-SPEC-0118 refines the kind to
   `rust_integration_public_api_path_unresolved`; non-integration witnesses
   keep `rust_transitive_reach_unresolved`.
5. **Proximity-only finding** (#7071): the only related test shares the
   owner's file and calls `outer()`, which invokes `call_inner!`. Result:
   `weakly_exposed` plus `static_limit_kind: rust_macro_reach_unresolved` and a
   next step naming the witnessing test.
6. **A recognized weak direct oracle also calls the owner** (#7542): same macro shape plus
   `assert!(inner(2) > 0)`, while the opaque `outer_triples` route asserts
   `outer(4) == 12`. Reach stays `yes` and classification stays `weakly_exposed`,
   but the finding names `rust_macro_reach_unresolved` and `outer_triples`.
   It does not advise writing an assertion before that route is inspected.
   A direct confirmed discriminator gets no new limit, including when another
   weak stage leaves its overall class `weakly_exposed`.
   A direct call with no recognized oracle is outside this mixed gate. An
   unread assertion macro with the owner in its arguments remains governed by
   RIPR-SPEC-0120 (#7089), including when dependent reach exceeds its budget.
7. **Owner handed to a local macro as an argument** (#6614): `doubled()`
   holds `twice!(inner)` and never calls `inner`; the only same-repo
   `macro_rules! twice` is unique and every source mentioning `macro_rules`
   parses. The chain records a macro-argument hop that forwards no result,
   binds no activation and pairs no boundary. A related test calling
   `doubled()` that holds an exact oracle in any operand shape
   (`assert_eq!(doubled(), 14)`, `let d = doubled(); assert_eq!(d, 14)`)
   reads `propagation_unknown` (`helper_result_not_forwarded`), neither
   credit nor gap. A test whose oracles are all weak on their own terms
   (`assert!(doubled() > 0)`, under the RIPR-SPEC-0159 weak-oracle rule,
   #7549) keeps the prior evidence, and no edge forms
   for an owner named only in a string literal, a caller that binds a local
   or parameter of the owner's name, or a second invocation site.
8. **Same-file test table** (#6925): a file defines `macro_rules! edge_table`
   with `$($name:ident($width:expr, $limit:expr, $want:expr))*` whose
   transcriber emits one `#[test] fn $name` asserting `fits_edge($width,
   $limit)` per row, and invokes it with rows under, on and over the limit.
   The owner is reached through the generated tests and the boundary row
   discriminates it, so it reads `exposed`, exactly as the same three tests
   written by hand. Rows that never touch the boundary read `weakly_exposed`;
   rows that discard the result read `reachable_unrevealed`; a row layout
   outside the table form (`name => (..)`) registers no test and keeps
   `no_static_path`.

## Required Evidence

- `StaticLimitKind::RustMacroReachUnresolved` in
  `crates/ripr/src/domain/language.rs`.
- `StopReason::MacroReachUnresolved` in `crates/ripr/src/domain/probe.rs`.
- Macro-boundary witness logic in
  `crates/ripr/src/analysis/classify/transitive_reach.rs`.
- Rust adapter wiring in `crates/ripr/src/analysis/language/rust/mod.rs` for diff
  and repo modes.
- Pure fixture: `fixtures/rust_macro_reach_limitation/`.
- Mixed direct/macro fixture: `fixtures/rust_mixed_direct_macro_reach/`.
- Honesty corpus member in
  `fixtures/evidence-promotion-honesty-corpus/corpus.json` asserting
  `must_emit_limitation`, `expected_limit_kind: rust_macro_reach_unresolved`,
  `must_disclose_limitation_detail`, and `must_remain_non_promoted`.

## Test Mapping

- `crates/ripr/src/analysis/probes/diff.rs::source_currentness_tests::macro_rules_definition_lines_seed_one_static_unknown_not_expression_probes`
- `crates/ripr/src/analysis/probes/diff.rs::source_currentness_tests::removed_macro_rules_lines_do_not_seed_expression_probes`
- `crates/ripr/src/analysis/probes/diff.rs::source_currentness_tests::same_named_macro_definitions_each_seed_their_own_unknown`
- `crates/ripr/src/analysis/probes/diff.rs::source_currentness_tests::macro_inside_an_inline_test_module_seeds_no_probe`
- `crates/ripr/src/analysis/probes/diff.rs::source_currentness_tests::attribute_only_edit_inside_a_macro_seeds_unknown_on_both_sides`
- `crates/ripr/src/analysis/probes/diff.rs::source_currentness_tests::a_line_mixing_a_macro_definition_and_other_code_keeps_normal_seeding`
- `crates/ripr/src/analysis/probes/macro_definition.rs::tests::extents_cover_matcher_and_transcriber_lines`
- `crates/ripr/src/analysis/probes/macro_definition.rs::tests::prose_and_lookalikes_are_not_definitions`
- `crates/ripr/src/analysis/probes/macro_definition.rs::tests::an_unclosed_definition_runs_to_the_end_of_the_file`
- `crates/ripr/src/analysis/probes/macro_definition.rs::tests::an_unclosed_definition_with_trailing_non_ascii_does_not_panic`
- `crates/ripr/src/analysis/probes/macro_definition.rs::tests::paren_and_bracket_delimiters_are_definitions`
- `crates/ripr/src/analysis/probes/macro_definition.rs::tests::nested_braces_and_repetitions_close_at_the_outer_delimiter`
- `crates/ripr/src/analysis/probes/macro_definition.rs::tests::a_brace_char_literal_does_not_unbalance_the_extent`
- `crates/ripr/src/analysis/probes/macro_definition.rs::tests::spacing_around_the_bang_is_accepted`
- `crates/ripr/src/analysis/probes/macro_definition.rs::tests::two_same_named_definitions_are_both_found`
- `crates/ripr/src/analysis/probes/macro_definition.rs::tests::raw_identifier_macro_names_are_definitions`
- `crates/ripr/src/analysis/probes/macro_definition.rs::tests::a_mixed_line_is_owned_by_no_definition`
- `crates/ripr/src/analysis/classify/transitive_reach.rs::tests::given_entry_path_stops_at_owner_macro_then_macro_witness_is_captured`
- `crates/ripr/src/analysis/classify/transitive_reach.rs::tests::given_macro_definition_does_not_name_owner_then_macro_witness_is_none`
- `crates/ripr/src/analysis/classify/transitive_reach.rs::tests::given_test_invokes_owner_macro_directly_then_macro_witness_is_captured`
- `crates/ripr/src/analysis/classify/transitive_reach.rs::tests::macro_witness_pointer_uses_may_language_and_no_coverage_claim`
- `crates/ripr/src/analysis/classify/transitive_reach.rs::tests::macro_reach_limitation_detail_names_edges_route_and_non_claim`
- `crates/ripr/src/output/human.rs::tests::human_output_surfaces_static_limitation_detail`
- `crates/ripr/src/analysis/language/rust/proximity_reach_tests.rs::proximity_only_reach_through_a_macro_names_the_macro_limit`
- `crates/ripr/src/analysis/language/rust/proximity_reach_tests.rs::a_weak_direct_test_preserves_the_unresolved_macro_witness`
- `crates/ripr/src/analysis/language/rust/proximity_reach_tests.rs::proximity_only_reach_without_a_witness_keeps_the_gap`
- `crates/ripr/src/analysis/language/rust/proximity_reach_tests.rs::proximity_only_reach_through_a_helper_names_the_transitive_limit`
- `crates/ripr/src/analysis/syntax/local_test_macros.rs::tests::expands_each_invocation_with_its_arguments_and_lines`
- `crates/ripr/src/analysis/syntax/local_test_macros.rs::tests::repetition_shadowing_scope_and_order_refuse`
- `crates/ripr/src/analysis/syntax/local_test_macros.rs::tests::arm_selection_follows_count_and_fragment_shape`
- `crates/ripr/src/analysis/syntax/local_test_macros.rs::tests::a_table_invocation_expands_one_test_per_row`
- `crates/ripr/src/analysis/syntax/local_test_macros.rs::tests::separators_groups_and_plus_repetition_expand`
- `crates/ripr/src/analysis/syntax/local_test_macros.rs::tests::tables_the_expander_cannot_split_exactly_refuse`
- `crates/ripr/src/analysis/syntax/local_test_macros.rs::tests::a_local_macro_in_a_table_row_or_body_refuses`
- `crates/ripr/src/analysis/syntax/local_test_macros.rs::tests::a_table_arm_follows_simple_arms_that_do_not_fit`
- `crates/ripr/src/analysis/syntax/local_test_macros.rs::tests::a_trailing_comma_must_agree_between_matcher_and_invocation`
- `crates/ripr/src/analysis/syntax/local_test_macros.rs::tests::a_row_group_ending_in_a_comma_does_not_fit_a_group_without_one`
- `crates/ripr/src/analysis/syntax/local_test_macros.rs::tests::row_and_expansion_size_are_bounded`
- `crates/ripr/tests/macro_generated_tests.rs::a_table_generated_test_reads_like_the_hand_written_test`
- `crates/ripr/tests/macro_generated_tests.rs::a_table_the_expander_cannot_split_exactly_adds_no_test`
- Verdict corpus cases `macro-table-boundary-rows`, `macro-table-semicolon-rows`, `macro-table-far-rows`, `macro-table-smoke-rows` and `macro-table-arrow-rows-refused`.
- `crates/ripr/src/analysis/facts/macro_generated_tests.rs::tests::macro_generated_tests_are_indexed_at_their_invocations`
- `crates/ripr/tests/macro_generated_tests.rs::a_macro_generated_test_reads_like_the_hand_written_test`
- `crates/ripr/tests/macro_generated_tests.rs::refused_shapes_and_uninvoked_generators_add_no_test`
- `crates/ripr/tests/macro_generated_tests.rs::a_deferred_assertion_in_a_generated_test_is_refused_like_a_hand_written_one`
- `crates/ripr/tests/helper_wrapper_reach.rs::exact_pin_through_a_local_macro_argument_abstains_as_not_forwarded`
- `crates/ripr/tests/helper_wrapper_reach.rs::any_exact_oracle_on_a_macro_argument_caller_abstains`
- `crates/ripr/tests/helper_wrapper_reach.rs::macro_argument_without_the_owner_or_an_exact_pin_keeps_the_gap`
- `crates/ripr/tests/helper_wrapper_reach.rs::owner_named_only_in_a_wrapper_string_literal_does_not_abstain`
- Verdict corpus case `old-rate-macro-arg-no-mention`.
- `fixtures/rust_macro_reach_limitation/expected/check.json`
- `crates/ripr/src/analysis/language/rust/proximity_reach_tests.rs::mixed_macro_limit_preserves_confirmed_discrimination_and_examined_misses`
- `crates/ripr/src/analysis/language/rust/mod.rs::tests::mixed_macro_witness_survives_dependent_scope_narrowing`
- `crates/ripr/tests/mixed_macro_reach.rs::weak_direct_oracle_preserves_the_macro_witness_in_ready_and_draft`
- `crates/ripr/tests/mixed_macro_reach.rs::strong_direct_oracle_remains_credited_beside_the_macro`
- `crates/ripr/tests/mixed_macro_reach.rs::weak_direct_without_a_test_macro_route_keeps_the_gap`
- `crates/ripr/tests/mixed_macro_reach.rs::unrelated_macro_with_weak_direct_keeps_the_gap`
- `crates/ripr/tests/mixed_macro_reach.rs::lexical_transitive_route_with_weak_direct_keeps_the_gap`
- `fixtures/rust_mixed_direct_macro_reach/expected/check.json`
- `cargo xtask check-evidence-promotion-honesty`

## Implementation Mapping

| Component | Location |
|---|---|
| Static limit enum | `crates/ripr/src/domain/language.rs` |
| Stop reason enum | `crates/ripr/src/domain/probe.rs` |
| Macro witness producer | `crates/ripr/src/analysis/classify/transitive_reach.rs` |
| Same-file test generator expansion | `crates/ripr/src/analysis/syntax/local_test_macros.rs` |
| Generated test indexing | `crates/ripr/src/analysis/facts/macro_generated_tests.rs` |
| Macro definition extents | `crates/ripr/src/analysis/probes/macro_definition.rs` |
| Generated test assertion admission | `crates/ripr/src/analysis/classify/owner_pin.rs` |
| Classifier export | `crates/ripr/src/analysis/classify/mod.rs` |
| Diff/repo-mode wiring | `crates/ripr/src/analysis/language/rust/mod.rs` |
| Human witness and limitation-detail projection | `crates/ripr/src/output/human/sections.rs` |
| Output contract docs | `docs/OUTPUT_SCHEMA.md` |
| Pure fixture | `fixtures/rust_macro_reach_limitation/` |
| Mixed direct/macro fixture | `fixtures/rust_mixed_direct_macro_reach/` |
| Honesty corpus case | `fixtures/evidence-promotion-honesty-corpus/corpus.json` |

## CI Proof

- `cargo test -p ripr analysis::classify::transitive_reach`
- `cargo test -p ripr analysis::language::rust`
- `cargo xtask fixtures rust_macro_reach_limitation`
- `cargo xtask check-evidence-promotion-honesty`
- `cargo xtask check-output-contracts`
- `cargo xtask check-static-language`
- `cargo xtask check-spec-format`
- `cargo xtask check-spec-numbering`
- `cargo xtask check-traceability`
- `cargo fmt --check`

## Metrics

- Golden fixture pass: `fixtures/rust_macro_reach_limitation`.
- Mixed direct/macro golden fixture pass: `fixtures/rust_mixed_direct_macro_reach`.
- Corpus invariant: `rust_macro_reach_named_limitation` emits
  `rust_macro_reach_unresolved`, discloses a witness and limitation detail, and remains
  non-promoted at `no_static_path`.
- Existing transitive-reach fixture behavior remains unchanged.
- Corpus invariant: `rust_mixed_direct_macro_witness_not_a_gap` retains
  `weakly_exposed` and reach `yes`, names the macro limitation and witness,
  and removes unsupported repair, verify and receipt advice.
