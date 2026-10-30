# Fixture: assertion_shaped_control_production_owner

Spec: RIPR-SPEC-0133

## Given

A normal production owner (`discounted_total` in `src/lib.rs`) whose body is
not assertion-shaped, covered by a test that reaches it through a builder
helper.

The diff changes the discount predicate:

```rust
if amount > 100 {
```

to:

```rust
if amount >= 100 {
```

## When

```bash
cargo xtask fixtures assertion_shaped_control_production_owner
```

or:

```bash
ripr check --root fixtures/assertion_shaped_control_production_owner/input --diff fixtures/assertion_shaped_control_production_owner/diff.patch --mode fast
```

## Then

Control: the standard code-under-test guidance is emitted unchanged, and the
finding evidence contains no `owner_shape:` line. The only owner input is the
helper call `boundary_amount()`. The helper is a nullary function declared
once in the test file whose body is the literal `100`, so RIPR-SPEC-0158
(#9637) reads the call as the row `100`, which sits on the changed `>=`
boundary, and the exact `assert_eq!(.., 90)` pins it: the boundary reads
`exposed`. (Before RIPR-SPEC-0158 read such a helper, the same shape read
`infection_unknown`, RIPR-SPEC-0032, #6615.) The class comes from
RIPR-SPEC-0158, not RIPR-SPEC-0133. The assertion-shaped reframe must not
leak onto ordinary production owners.

## Must Not

- Reframe guidance for an owner that is not assertion-shaped.
- Change the exposure class or finding set because of RIPR-SPEC-0133: the
  class is whatever RIPR-SPEC-0158 gives the same shape without an
  assertion-shaped owner.
- Use mutation-runtime outcome vocabulary reserved for real mutation execution.
