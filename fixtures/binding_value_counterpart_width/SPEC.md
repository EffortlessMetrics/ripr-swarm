# Fixture: binding_value_counterpart_width

Spec: RIPR-SPEC-0158

## Given

The compared parameter constrains the local's unsuffixed arithmetic to u8
or i8 even though its initializer reads only a boolean parameter. Native
Rust 1.95.0 compiles these shapes. With overflow checks enabled they panic
before the predicate. With checks disabled the unsigned local wraps to 0
and the signed local to -64: both the old `<` and current `<=` assertions
pass, so neither test discriminates the changed equality boundary.

## When

Run `cargo xtask fixtures binding_value_counterpart_width`.

## Then

Both predicate findings remain infection_unknown with no fabricated equality
or missing discriminator. The independent RIPR-SPEC-0108 corpus pins exactly
two predicates and non-promotion. Public regressions retain genuine u32,
i32 and usize counterparts, unused narrow parameters and both orientations.

The two separate returned-comparison findings remain propagation_unknown
outside this predicate repair. Fixture tests are adversarial static
inputs, not claimed passing runtime tests. Native overflow-profile controls
are separate evidence; no arbitrary Rust type inference is added.

## Must Not

- Credit either narrow predicate from host-index arithmetic.
- Invent an equality observation or a missing boundary discriminator.
- Claim that the fixture's adversarial Rust tests pass at runtime.
