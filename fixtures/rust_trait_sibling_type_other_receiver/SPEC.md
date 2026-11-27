# Fixture: rust_trait_sibling_type_other_receiver

Spec: RIPR-SPEC-0108
Related spec: RIPR-SPEC-0197
Issue: #15276

## Given

`impl Shape for Square { fn area }` changes from `self.0 * self.0` to
`self.0 + self.0`. `Line` also implements `Shape::area`. The only test pins
`Line(3).area()` to `0`.

## When

```bash
cargo xtask fixtures rust_trait_sibling_type_other_receiver
```

## Then

Exactly one `return_value` finding is not promoted, and its related test is
not `direct_owner_call`. The call runs `Line::area`, never the changed
`Square::area`. The authored crate compiles and its one test passes on both
sides of the diff. `rust_trait_sibling_type_receiver` is the positive pair.

## Must Not

- Credit the `Line` receiver for the `Square` owner.
