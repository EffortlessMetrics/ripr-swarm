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

## Required Evidence

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

## Test Mapping

Fixture coverage:

- `fixtures/boundary_gap` (baseline)
- `fixtures/weak_error_oracle` (baseline)
- `fixtures/smoke_assertion_only`
- `fixtures/no_static_path`
- `fixtures/infection_expected_value_literal`
- `predicate_infection_ignores_boundary_literal_used_only_as_expected_value`
- `predicate_infection_credits_the_same_literal_when_it_is_an_owner_input`
- `predicate_infection_credits_table_row_inputs_but_not_enum_variants`
- `let_bound_owner_argument_is_an_owner_input`
- `let_bound_owner_argument_fails_closed_on_mut_shadowed_or_computed_bindings`
- `owner_input_values_exclude_assertion_expected_values`
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
