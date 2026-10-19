# RIPR-SPEC-0001: Static Exposure Loop

Status: accepted

## Problem

Developers and coding agents can change Rust behavior while leaving tests that
execute nearby code but do not discriminate the changed behavior. Coverage does
not identify that oracle gap, and real mutation testing is often too expensive
for live draft feedback.

## Behavior

Given a Rust/Cargo workspace and a diff, `ripr` identifies changed Rust behavior,
creates mutation-shaped probes, and reports whether current tests appear to
contain a discriminator that would notice if that behavior were wrong.

The loop is:

```text
changed behavior
-> static probe
-> related tests
-> RIPR evidence
-> missing or weak discriminator
-> recommended targeted test intent
```

The explicit `check --diff FILE --output-dir NEW_DIRECTORY` output-set adapter
enters that diff pipeline once and projects native badge JSON, detail JSON and
human output with their existing policies and presentation context (#8169).
Its bounded input-custody observations and final manifest prevent admitting a
successful complete output set after observed input drift or render/write
failure. Native analysis/refusal states and exit behavior remain authoritative;
publication completeness cannot imply analysis completeness. The output-set
contract and its supported input boundaries are in
[Output Schema](../OUTPUT_SCHEMA.md#native-diff-output-set).


A changed line that only opens or closes a block (`}`, `});`, `} else {`,
`else {`) seeds no probe when its contiguous changed run also holds a
behavioral line and every block it closes was opened inside that run, as
with the braces of a newly added function or branch. A structural line that
closes a block opened outside its run (a lone inserted or removed
`} else {`, or one inserted with other lines between existing statements)
moves existing code between branches, so it keeps its probe and the change
never reads as "no behavioral candidates" (#4216 row 5).

A line that Git reports as both removed and added inside one replacement block
seeds no probe on either side when its text is byte-identical (leading
whitespace included, since it is content inside a multiline string), occurs
exactly once on each side of the block, contains an alphanumeric character,
does not begin as a continuation (`.call()`, `&&`, `?`), and sits under the
same nearest opener on both sides (#8389). The whole block keeps its probes
when paired lines change relative order, when an unpaired line opens or closes
a block, or when a paired statement line (ending in `;`) crosses an edited
line (unpaired removed lines before it and unpaired added lines after it, or
the reverse). Field lines such as `end: span.end(),` may move within their
literal.

An equal-sized replacement with no retained long identifier token does not
seed a second historical finding when an emitted candidate probe of the same
family already represents that exact old line (#7807). Pairing uses the Git
block's old/new coordinates and requires faithful `before`/`after` evidence:
the candidate covers the whole added line, or a byte-aligned cut of the old
line equals its `before`. A whole old line attached to a smaller constructor
does not establish that every edit was represented. Uneven blocks, unrepresented
families and ambiguous edits retain historical evidence. This rule changes
removed-side reconciliation only; parser selection and constructor identities
remain separate authorities.

## Required Evidence

Parser-confirmed Rust where clauses and generic type/lifetime bounds without
expression or macro descendants are compile-time syntax, not runtime field
construction (#7251). Declaration-only changed lines retain `static_unknown`
probes. An inline bound may use its enclosing struct, impl or type-alias item
as declaration authority only when the whole changed line is contained by that
item and the item has no expression or macro descendants. Const expressions
and macro types retain existing fallback evidence.
Real expressions on a shared line
keep their existing parser-owned runtime probes. Comments, strings and macro
token trees cannot establish declaration authority. Removed bounds require
old-side parser context and local source roles reconstructed from the diff.
Candidate line coincidences cannot exclude old production bounds or admit old
test bounds. External module/include exclusion retains resolved candidate-index
composition authority. Test attributes use the shared normalizer and active
harness registrations; external parent edges, configuration and Cargo manifests
are not reconstructed.
Unavailable or malformed source retains existing uncertainty rather than
borrowing a candidate line.

Each finding should carry:

- changed behavior
- probe family
- RIPR stage evidence
- related tests, if any
- oracle evidence, if any
- observed activation values, if statically visible
- local flow sink family when the changed behavior reaches an observable
  returned value, error variant, output field, match result, event/outbound
  call, state write, persistence write, log message, configuration change, or
  generic call effect
- missing discriminator
- recommended next step
- stop reason for unknowns

Exhaustive human Changed source fragments preserve the complete source and wrap
printable Unicode by display cells between extended grapheme clusters. The
fixed payload budget, oversized-cluster exception, ASCII/multiline compatibility
and literal consumer controls are specified by
`fixtures/rust_long_expression_display_bound/SPEC.md` (#7595).

## Inputs

- Rust/Cargo workspace root
- Git base or explicit unified diff
- analysis mode
- optional repository configuration

## Outputs

- human findings
- versioned JSON findings
- GitHub annotations
- LSP diagnostics and hover content when used through the editor
- agent context packet for a selected finding

## Classifications

Static findings may use only these exposure classes:

- `exposed`
- `weakly_exposed`
- `reachable_unrevealed`
- `no_static_path`
- `infection_unknown`
- `propagation_unknown`
- `static_unknown`

`exposed` requires reach evidence that a test runs the changed owner: a
captured call, a helper chain, or assertion affinity. A test linked only by
sharing the changed file, module, or a name token stays listed as the likely
place to add a test, but reach is `weak` and the finding cannot be `exposed`.
When a `weakly_exposed` finding has weak reach and every related row is a
canonical proximity relation, its next step asks the reader to inspect existing
tests, doctests and examples before changing assertions. Proximity does not
establish that an assertion is broad or that a test is missing. Exact-sink
advice suppression, assertion-shaped owner guidance and specific wrapper-error
limitations retain their existing precedence. This guidance does not index or
credit documentation examples, change classifications, or admit a repair.
For a predicate probe, `exposed` also requires one test that both feeds a
boundary input to the owner and holds a discriminating oracle on that call's
result; otherwise the finding is at most `weakly_exposed` and names
`same_test_pairing_missing` (#4828).
For a field-construction probe whose every exact pin sits beside a pin of a
sibling field, bound to one operand of the changed initializer, at the same
value, the finding is at most `weakly_exposed` and names
`field_pinned_equal_to_operand` (#7077, RIPR-SPEC-0197 rule 7).

An unguarded whole wildcard pattern (`assert!(matches!(value, _))` or
`assert_matches!(value, _)`) accepts every value. Its oracle is
`relational_check` / `weak`, so it cannot establish exact discrimination or
promote a finding to `exposed` (#5397). Exact patterns and wildcard patterns
with an explicit guard retain their existing classification. This bounded
rule does not solve general pattern exhaustiveness or compound conditions.

A standalone `matches!` invocation computes a boolean without failing the
test. Discarded expression statements and unconsumed bindings therefore
provide no assertion oracle, whether the pattern is wildcard, exact or
guarded (#5713). Parser, lexical and registered-harness admission share this
boundary. Actual asserting wrappers retain their existing classification;
an unrelated observer on the same line cannot acquire the discarded
matcher's pattern. The lexical extractor retains a recognized observer
within its scrutinee, such as smoke credit for `result.unwrap()`.
The dedicated terminal Result-guard scanner still recognizes matchers that
participate in its failure decision. This rule does not infer boolean
dataflow across bindings.
Lexical projection is bounded to 16 nested matcher/block steps and omits
oracle credit on exhaustion; parser-rejected deep source cannot recreate an
unbounded recursive fallback.
Unrecognized compound computations with a surviving matcher and no supported
asserting wrapper receive no oracle credit. Existing terminal Err-return and
guarded Result failure authorities retain consumed matcher evidence.

## Non-Goals

This spec does not require:

- running mutants
- proving adequacy
- generating complete tests
- whole-workspace semantic proof
- coverage reporting

## Acceptance Examples

Boundary example:

```rust
if amount >= discount_threshold {
    apply_discount(...)
}
```

If existing tests use `50` and `10_000`, and only assert
`quote.total > Money::zero()`, `ripr` should report weak exposure and name the
missing equality-boundary value and exact assertion shape.

Returned-comparison boundary example:

```rust
pub fn ships_free(items: u32) -> bool {
    items >= 10
}
```

When the changed comparison is the whole tail expression of an owner with a
return type (no `;`, `if`, `let`, `return`, or continuation line; comments are
ignored when matching the line), its boolean is the returned value, so the
predicate propagates to the returned value without a branch. If tests reach the owner, directly or through a wrapper such
as `shipping(items)`, only at `20` and `2` items, `ripr` should report weak
exposure naming `items == 10` with the boundary-test next step, not
`propagation_unknown`. A predicate retargeted from a changed `let` initializer
keeps its RIPR-SPEC-0158 operand-value limitation and does not gain this sink.

Error example:

```rust
Err(Error::InvalidCurrency)
```

If a related test only checks `result.is_err()`, `ripr` should distinguish that
from an exact variant assertion.

Side-effect example:

```rust
events.publish(DiscountApplied { amount })
```

`ripr` should report the propagation sink as an event or outbound call rather
than a generic call effect. Similar syntax-first labels should be available for
state writes, persistence writes, log messages, and configuration changes. When
the sink family is not statically obvious, `ripr` should keep the older
`call_effect` fallback or report propagation as unknown with a stop reason.

Field-assignment boundary example:

```rust
let mut file = File::default();
file.body_model_version = HIR_BODY_MODEL_VERSION - 1;
load(&file);
```

For a predicate comparing `file.body_model_version == HIR_BODY_MODEL_VERSION`,
RIPR may credit the direct field assignment as activation evidence only when
the object, field, same-file literal constant, bounded `+/-` integer offset,
and source order are exact and unambiguous. The equality value and its adjacent
integer boundaries must remain distinct. Other objects, other fields,
similarly named constants, helper-only assignments, and opaque right-hand
expressions must not be credited. Assignments nested under control flow are not
unconditional evidence, and an intervening explicit mutable borrow invalidates
an earlier field value. An otherwise related test with an unsupported or
invalidated direct field assignment must stop as
`field_assignment_value_unresolved` instead of receiving a repair
recommendation that the analyzer cannot credit.

Boundary-literal infection example:

```rust
// changed owner
if weight_grams > 2_000 { 400 } else { 150 }

// related test
assert_eq!(fragile_fee(parcel_weight("crate")), 400);
assert_eq!(tax_bps("EU"), 2000);
```

A literal that matches the changed boundary counts as infection evidence only
when it flows into the changed owner's inputs: an argument of a direct call to
the owner, a number or boolean bound by the test's single immutable
`let name = <literal>;` declaration and passed to the owner (read through the
shared let-binding scan in `analysis/value_resolution.rs`), a table-row cell,
or a builder-method argument. The activation authority
(`analysis/classify/activation.rs`) separates these inputs from assertion
arguments, and the infection stage reads that separation instead of scanning
every literal in the test. An assertion's expected value (the `2000` above) is
an oracle, not an input, so it must not produce `infection yes`; RIPR reports
`infection weak` and says the boundary literal appears only outside the
owner's inputs. A `mut`, shadowed, string, or computed binding is not an
exact input value.

An edit that adds or removes a `&&` or `||` operand (`other > 0 && 100 <=
points` to `100 < points`, in an `if` or a match guard) changes more than the
surviving comparison. An input at that comparison's boundary says nothing
about the operand that came or went, so the predicate reads `infection
unknown` and names the old predicate (#7898). The rule compares the masked
sequence of `&&`/`||` connectives in the probe's old and new text (a `||`
that opens an empty closure, as in `unwrap_or_else(|| 0)`, is not one), the
same reading the guard-only arm filter uses. It replaces only boundary
credit: an unresolved boundary keeps its own reason, and a predicate with no
input at its boundary keeps its weak reading. An edit inside one operand of
an unchanged chain keeps its boundary credit. Because it compares only the
connectives, swapping one operand for another (`a && 100 <= p` to `100 < p
&& b`) or regrouping a chain is not yet caught. The rule reads one changed
line: a chain that rustfmt splits across lines (`if other > 0` over `&&
100 <= points`) pairs only its first old line as `before`, so it is not yet
covered (#7945).

A computed argument or operand is not the literal it contains. An owner-call,
table-row, or builder-method argument that applies a binary arithmetic,
bitwise, or shift operator (`order_discount(base + 1)`, `.amount(base + 10)`,
`1 << 4`, `16usize - 1`; a sign is a float exponent only directly after the
`e`/`E` of a decimal mantissa such as `1e-5`; an operand may end in a call,
index, block or `?`, as in `name.len() + 10`, `{ x } + 1` or `parse(s)? - 1`)
or repeats an array element (`&[b'f'; 16]`, whose `16` is a length) yields no
input value. A table-row or builder line is split at top-level commas with
full `()`/`[]`/`{}` depth, and a char or byte literal such as `','` or `b'('`
is one value, never a delimiter, so `(',', true)` passes `','`. Below the
line itself, only a constructor or variant whose last path segment is
UpperCamel (`Some(10)`, `Token::Num(5)`, `Case { n: 1 }`), `vec![..]`, or a
bare tuple or array passes its contents through; a method call
(`x.min(10)`, `10.min(x)`), a lowercase function or macro call, an index, or
a block computes its argument. A table-row or builder line nested deeper than
32 levels is not read. It leaves the changed boundary unresolved only when its
test also calls the owner (or the helper chain's entry) with an argument that
is not an exact value, and only after the exact owner calls show no boundary
input, so a deep row feeding something else hides neither a credited
`score(10)` nor a missing input beside `score(20)`. When the owner is called
but every call passes only computed arguments, a compared operand that is
neither a parameter, a literal, nor a constant (a local counter `count`) is
unresolved, so a stray test literal such as an array length never reads as a
weak gap. A comparison
operand that computes its value (`CURRENT - 2 > version`, `s.len() < 2 + 2`)
contributes no boundary literal, so `CURRENT - 2` is never read as the
boundary `2`. A compound condition
(`&&`, `||`) is read one top-level comparison at a time, so a literal inside a
computed operand of one comparison is not the boundary of the whole condition.

Unresolved boundary input example:

```rust
let mut count = 0;
for _ in hex {
    count += 1;
    if 16 < count { return Err(TooLong); }
}
```

An input RIPR cannot read is not a missing input. When a changed comparison's
operand is neither a parameter, a value the bounded evaluator folds from a
related test's exact inputs, a literal, nor a named constant (a local
accumulator such as `count`, `s.len()`, `name.split_whitespace().count()`),
including when neither operand is readable (`out.len() != data.len() / 2`), or
when a related test feeds the compared parameter a computed argument, directly
or through a resolved helper hop whose call-site argument is computed
(`score(y + 1)`), no input row says which side of the boundary a test reaches.
A computed argument counts only when it is one definite value: its variables
are all bound to exact values or constants (`base + 1` with `let base = 9;`,
`[b'f'; 16]`), or, at a hop, to the caller's parameters under exact entry
rows. A computation over a value RIPR cannot bind (the loop variable in
`can_retire(age + 1)`) is no more readable than that bare variable, which
yields no input row without unresolving the boundary. RIPR does not name a
missing equality discriminator for it; `ripr check` reports `infection
unknown` with `Changed boundary input is unresolved` and the operand or
parameter it could not read. A test input literal that merely equals a
literal in the comparison does not credit such a boundary; only an exact
observed equality does. Exact inputs that all sit off the boundary keep
the missing equality discriminator.

One bounded cardinality family reads a direct immutable slice parameter's
`.len()` compared with a plain decimal integer. Every reach-bearing related
test must make one binding-resolved ordinary owner call through an eager,
nonempty immutable constant-row table. Each argument row must be an immutable
reference to a literal array, and the complete index must contain no non-test
caller of the owner. Tables have at most 64 rows and 256 scalar literal
elements per slice (4 KiB source per slice). Cardinality comes from the array
shape, never its elements or neighboring assertions: `[0, 0]` has length two.
The existing assertion-context authority must admit the table's assertion
macros. Equality keeps the matching call's source and line; otherwise all
observed lengths support the missing equality. Computed rows, extra calls,
helper or alias reach, opaque macros, mutable or shadowed bindings, skipped
loops, asynchronous owners and custom `.len()` receivers remain outside this
family. Other ordinary calls, method calls, closure invocation, suspension and
yielding, explicit `loop` and `while` constructs remain outside the family. Standard assertion operands are parsed too:
only the direct immutable result binding and intrinsic `iter().min()` /
`iter().max()` observations rooted at the exact
immutable slice column are admitted. Their `unwrap()` requires every row to be
nonempty; `unwrap_or(&literal)` also admits empty slices. Same-named methods on
other receivers and unresolved calls inside assertion operands are refused.
Owners must be nongeneric with a primitive scalar result. User-defined types,
traits, implementations, type aliases and imports other than `use super::*`
in the indexed files withhold this family; it does not resolve operator dispatch
or primitive-name aliases. Other assertion value paths and local operand
bindings remain unsupported.
A try expression anywhere in the test function withholds this producer:
a discarded fallible closure can exit before the call or skip later rows, and
this family does not resolve that control flow. The refusal also applies when
no assertion invokes the independent assertion-context authority. Every source
use of the owner's identifier must be accounted for by its declaration and the
admitted direct calls; recursion, function pointers, explicit imports and opaque
uses withhold this bounded producer. Documentation
references, explicit `doc` attributes, documentation code blocks, workspace
macro expansion, possible standard-macro rebinding and attributes other than
ordinary test and `cfg(test)` withhold this producer too. Plain documentation
comments remain eligible subject to reference accounting. It reuses
existing unseen-reach, macro operand and binding authorities only as refusals;
it does not resolve expansion or award evidence from it. Unreadable
inputs never disappear from an absence claim (#7553). This producer records
activation, while existing oracle and same-test pairing gates retain their
limitations; it does not independently qualify strong direct-table exposure.
This does not evaluate buffer arithmetic or similarity algorithms and does
not change classification precedence or unresolved macro/dispatch authority.

The human report's "Why unknown" line for `infection_unknown` says the change
"reaches a sink" only when the propagation stage is `yes`; otherwise it says
no sink the change reaches was established.

Named-constant boundary example:

```rust
pub const DISCOUNT_THRESHOLD: u64 = 10_000;

if amount >= DISCOUNT_THRESHOLD { /* discount */ }
```

When a changed comparison's boundary operand names a constant (an upper-case
identifier, optionally `Self::` qualified), RIPR looks the constant up in the
owner's own source file through the shared named-constant lookup in
`analysis/value_resolution.rs`, for both `ripr check` and repo-seam grip:

- a constant declared exactly once as an integer literal resolves to that
  value, so a related test input equal to it (`discounted_total(10_000)`)
  observes the equality boundary;
- a test argument that names the constant itself
  (`discounted_total(DISCOUNT_THRESHOLD)`, `pricing::DISCOUNT_THRESHOLD`)
  observes the boundary by identity when the owner's file declares that
  constant exactly once, even when its initializer is computed. A test whose
  own file (other than the owner's) declares a constant of the same name, or
  whose file source is unavailable, is not credited by identity, because its
  argument may name its own constant;
- declarations are counted after any same-line `#[...]` attributes, so
  `#[cfg(a)] const LIMIT: u32 = 10;` beside a second `LIMIT` makes the lookup
  ambiguous rather than resolving to the unattributed one;
- a constant offset by an integer literal (`CURRENT - 2`, `LIMIT + 1`)
  resolves to the constant's value plus the offset when the constant itself
  resolves (`const CURRENT: u32 = 7;` makes `CURRENT - 2` the boundary `5`);
  the offset literal alone is never the boundary, and a test argument naming
  the constant is not the offset boundary by identity; when the constant's
  value is not a plain decimal literal (`10u32`, `0x10`, a computed
  initializer), the offset boundary is unresolved (`infection unknown`), not a
  missing input;
- a constant declared once with a computed or suffixed initializer keeps the
  missing equality-boundary discriminator, and its reason says RIPR cannot see
  the constant's value and that passing the constant itself is recognized;
- a constant the owner's file does not declare (imported) or declares more
  than once is not named as a missing discriminator, because no test could
  satisfy it. `ripr check` reports `infection unknown` and says it cannot see
  the constant's value; repo-seam grip reports activation unknown with the
  boundary-constant limitation and no actionable repair packet.

## Test Mapping

Fixture coverage:

- `unindexed_doc_assertion_prompts_inspection_without_credit_or_gap_claim`
- `weak_direct_owner_assertion_keeps_exact_value_advice`
- `wrapper_error_retains_specific_guidance_for_proximity_reach`
- `assertion_helper_retains_oracle_inspection_guidance`
- `fixtures/boundary_gap` (baseline)
- `fixtures/weak_error_oracle` (baseline)
- `fixtures/smoke_assertion_only`
- `fixtures/no_static_path`
- `discarded_matches_are_not_lexical_oracles`
- `deeply_nested_discarded_matchers_fail_closed`
- `discarded_matchers_cannot_supply_an_unrelated_observers_pattern`
- `repeated_matcher_text_in_type_trivia_cannot_move_the_observer`
- `discarded_matches_in_a_parsed_owner_test_are_not_oracles`
- `registered_trials_do_not_credit_discarded_matcher_computations`
- `asserting_wrappers_keep_consumed_pattern_oracles`
- `consumed_matcher_failure_guards_keep_their_result_oracle`
- `consumed_matcher_err_return_guard_keeps_its_exact_oracle`
- `asserted_bound_matcher_keeps_the_actual_assertion`
- `matcher_computation_and_asserted_result_have_different_runtime_grip`
- `discarded_matcher_cli_controls_reject_false_credit_and_retain_consumers`
- `fixtures/infection_expected_value_literal`
- `predicate_infection_ignores_boundary_literal_used_only_as_expected_value`
- `predicate_infection_credits_the_same_literal_when_it_is_an_owner_input`
- `predicate_infection_credits_table_row_inputs_but_not_enum_variants`
- `let_bound_owner_argument_is_an_owner_input`
- `let_bound_owner_argument_fails_closed_on_mut_shadowed_or_computed_bindings`
- `owner_input_values_exclude_assertion_expected_values`
- `computed_value_expressions_are_told_apart_from_spelled_values`
- `computed_owner_argument_yields_no_exact_input_value`
- `given_computed_argument_for_compared_parameter_then_boundary_is_unresolved_not_missing`
- `given_literal_inputs_off_the_boundary_then_missing_boundary_is_still_reported`
- `given_local_accumulator_boundary_then_boundary_is_unresolved_not_missing`
- `given_length_of_string_input_boundary_then_boundary_is_unresolved_not_missing`
- `unresolved_boundary_input_is_unknown_not_weak`
- `computed_comparison_operand_is_not_its_contained_literal`
- `given_constant_minus_offset_boundary_then_boundary_is_the_offset_value_not_the_offset`
- `constant_offset_operands_name_the_constant_and_signed_offset`
- `offset_integer_values_add_only_to_plain_decimal_constants`
- `given_computed_helper_hop_argument_then_boundary_is_unresolved_not_missing`
- `given_both_boundary_operands_unreadable_then_boundary_is_unresolved_not_missing`
- `given_offset_of_opaque_constant_then_boundary_is_unresolved_not_missing`
- `free_identifiers_name_only_the_variables_an_expression_reads`
- `unresolved_boundary_input_is_not_credited_by_a_matching_input_literal`
- `literal_kind_filter_reads_only_the_spelled_boundary_literals`
- `suffixed_or_shifted_arguments_are_computed_end_to_end`
- `table_and_builder_lines_skip_computed_arguments`
- `char_literal_table_rows_are_owner_inputs_end_to_end`
- `computed_builder_argument_is_not_credited_end_to_end`
- `deeply_nested_table_line_is_unreadable_not_a_crash`
- `computed_only_call_with_local_counter_boundary_is_unknown_not_weak`
- `method_call_builder_argument_is_not_credited_end_to_end`
- `all_dropped_boundary_literals_read_the_no_literal_boundary_reason`
- `given_counted_local_boundary_then_boundary_is_unresolved_not_missing`
- `retained_property_table_names_the_missing_empty_slice`
- `slice_elements_are_not_slice_lengths_and_empty_rows_are_observed`
- `unreadable_or_skippable_slice_tables_withhold_missing_empty_claims`
- `helpers_aliases_and_opaque_macros_cannot_supply_partial_slice_evidence`
- `equality_keeps_the_matching_table_witness`
- `recursive_empty_input_cannot_disappear_from_cardinality_evidence`
- `production_function_pointer_input_cannot_disappear`
- `hidden_documentation_inputs_withhold_missing_empty_claims`
- `unrelated_documentation_does_not_hide_known_table_inputs`
- `workspace_expansion_limits_withhold_partial_slice_evidence`
- `ignored_fallible_closure_cannot_credit_skipped_slice_rows`
- `fully_executed_slice_table_needs_no_assertion_for_activation`
- `unresolved_calls_cannot_credit_skipped_slice_rows`
- `fallible_slice_observations_cannot_credit_later_rows`
- `fixtures/rust_slice_table_boundary`
- `fixtures/rust_slice_table_zero_elements`
- `fixtures/rust_slice_table_try_exit`
- `fixtures/rust_slice_table_call_exit`
- `infection_unknown_hint_claims_a_sink_only_when_propagation_is_yes`
- `fixtures/boundary_named_constant`
- `same_file_constant_boundary_is_observed_at_its_literal_value`
- `argument_naming_the_constant_is_the_boundary_by_identity`
- `constant_not_pinned_to_the_owner_file_fails_closed`
- `argument_naming_a_test_file_constant_of_the_same_name_is_not_the_boundary`
- `given_test_file_declaring_its_own_same_name_constant_then_identity_is_not_credited`
- `a_test_file_declaring_the_same_constant_name_may_shadow_the_owner`
- `named_constant_counts_attribute_prefixed_declarations`
- `predicate_infection_names_an_unmatched_constant_boundary`
- `named_constant_reads_one_same_file_integer_declaration`
- `named_constant_fails_closed_on_computed_duplicate_or_mutable_declarations`
- `constant_operand_names_and_arguments_match_by_identity`
- `given_literal_input_at_same_file_constant_value_then_equality_boundary_is_observed`
- `given_inputs_off_the_same_file_constant_value_then_equality_boundary_names_its_value`
- `given_argument_naming_a_declared_constant_then_equality_boundary_is_observed`
- `given_constant_not_declared_in_owner_file_then_boundary_is_a_named_limitation`
- unit coverage for local flow sink families: predicate-to-return,
  predicate-to-error, match-arm result, output field, event/outbound call,
  state write, persistence write, log message, configuration change, and
  unknown propagation fallback
- `fixtures/tail_comparison_boundary`
- `predicate_that_is_the_owner_tail_flows_to_the_returned_value`
- `predicate_tail_sink_fails_closed_off_the_bare_returned_comparison`
- `predicate_tail_with_a_trailing_comment_still_flows_to_the_returned_value`
- `given_direct_field_assignments_from_named_constant_boundaries_then_values_are_observed`
- `given_other_object_or_field_assignments_then_boundary_value_is_not_credited`
- `given_similarly_named_constant_then_equality_boundary_stays_missing`
- `given_assignment_only_in_unrelated_helper_or_test_then_value_is_not_credited`
- `given_direct_field_assignment_with_opaque_rhs_then_names_field_assignment_limitation`
- `given_control_flow_nested_field_assignment_then_boundary_value_is_not_credited`
- `given_mutable_borrow_after_field_assignment_then_stale_value_is_not_credited`
- `structural_delimiter_lines_are_recognized`
- `probes_for_file_skips_brace_and_else_lines_of_new_function`
- `probes_for_file_keeps_lone_structural_line_hunks`
- `probes_for_file_keeps_structural_line_closing_outside_block_in_mixed_run`
- `lone_else_split_hunk_is_not_no_behavioral_candidates`
- `an_identical_line_inside_a_replacement_block_seeds_no_probe`
- `identical_line_suppression_is_fail_closed`
- `identical_line_pairing_never_hides_reorders_or_block_moves`

## Implementation Mapping

Current and planned modules:

- `analysis`: diff loading, file facts, probe generation, classification
- `domain`: probes, RIPR evidence, oracle strength, exposure class
- `output`: human, JSON, GitHub, future SARIF rendering
- `lsp`: diagnostics, hover, actions

## Metrics

- fixture pass rate
- unknowns with stop reasons
- oracle kind recognition rate
- flow sink identification rate
- activation value extraction rate
- static runtime by mode
