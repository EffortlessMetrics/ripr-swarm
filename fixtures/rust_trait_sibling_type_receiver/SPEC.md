# Fixture: rust_trait_sibling_type_receiver

Spec: RIPR-SPEC-0108
Related spec: RIPR-SPEC-0197
Issue: #15276

## Given

`impl Shape for Square { fn area }` changes from `self.0 * self.0` to
`self.0 + self.0`. Another type, `Line`, also implements `Shape::area`. The
test pins `Square(3).area()` to the literal `9`.

## When

```bash
cargo xtask fixtures rust_trait_sibling_type_receiver
```

## Then

Exactly one `return_value` finding reads `exposed`. The related test is
`direct_owner_call` with an `exact_value`/`strong` assertion. The receiver
`Square(3)` is typed as the owner's own impl type, and a method of another
named type (`Line::area`) cannot take a call on it. The authored crate
compiles; its test passes before the diff and fails after it (`6 != 9`).

`rust_trait_sibling_type_other_receiver` is the control: the same crate
shape, with a test that calls only `Line(3).area()`, is not promoted.

## Must Not

- Credit a receiver of another type, an untyped binding or a `dyn` receiver.
- Ignore a same-type rival: an inherent `Square::area`, another trait's
  `area` on `Square`, a blanket impl or a trait default body still refuses
  the pin (unit tests in `owner_pin/tests.rs`).
