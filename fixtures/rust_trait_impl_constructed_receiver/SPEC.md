# Fixture: rust_trait_impl_constructed_receiver

Spec: RIPR-SPEC-0108
Issue: #15956

## Given

The production `impl Shape for Square { fn area }` changes from
`self.side + self.side` to `self.side * self.side`. `Line` implements
`Shape::area` too, so the method name alone does not select an impl. The test
`square_area` builds its receiver with `Square::with_side(3)`, an associated
function whose indexed declaration returns `Square`, and pins `area()` to `9`.
The test `line_area` calls `area()` on a `Line::with_len(3)`.

## When

```bash
cargo xtask fixtures rust_trait_impl_constructed_receiver
```

## Then

Exactly one `return_value` finding. `square_area` is related by
`direct_owner_call`: its receiver is a `Square`, `Square` has no other method
named `area`, and no impl ripr cannot list could define one. `line_area` is
not related by a direct call.

The authored crate compiles. `square_area` fails before the diff (`6`) and
passes after it (`9`).

## Must Not

- Relate `line_area` to the changed method by a direct call.
- Read a receiver built by a function whose declared return is not the type
  (`Option<Square>`, an unindexed function) as a `Square`. The unit tests in
  `related_tests/trait_receiver/tests.rs` keep those name-only.
