# Fixture: rust_sibling_effect_oracle

Spec: RIPR-SPEC-0094

## Given

The exact 430-line `src/sensors/ripr_limits.rs` from
EffortlessMetrics/ub-review commit
`f28f8b094ce56586df2b14b8b11f9932fb44b38d` is retained without rewriting.
The enclosing module files make its original `pub(super)` scope compilable.
The targeted diff changes only `.or_insert_with(BTreeSet::new)` to
`.or_default()` in `module_limit_evidence`.

Real aggregation callers coexist with parser-only orientation assertions.
Those two equalities compare `parse_note` results and never call the changed
aggregation owner. The original aggregation assertions retain their indexed
observation limitations; this slice does not infer their JSON value flow.

## When

`cargo xtask fixtures rust_sibling_effect_oracle` analyzes the targeted effect.
The public API metamorphic control replaces only the two orientation equalities
with discarded `parse_note` calls, preserving both parser stimuli, malformed
parser checks and real owner callers.

## Then

The `.or_default()` SideEffect finding remains `weakly_exposed` with weak
discrimination in both orientations. Both nonempty parser equality rows stay
visible as `same_test_file` / `observation_unconfirmed` and lend no strong credit.
A direct persisted-state effect observer retains strong discrimination in the
public positive control.

## Must Not

- Delete proximity evidence or parser-only rows.
- Treat unknown flow or the owner's JSON observers as established.
- Broaden #7778's string-predicate scope or claim release artifact parity.
- Infer runtime mutation results from static findings. Native tests and a
  compiling effect-removal control are separate evidence.
