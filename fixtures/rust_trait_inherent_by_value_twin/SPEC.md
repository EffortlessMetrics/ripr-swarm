# Fixture: rust_trait_inherent_by_value_twin

Spec: RIPR-SPEC-0108
Related spec: RIPR-SPEC-0197
Issue: #15428

## Given

The production `impl Shape for Square { fn area(&self) }` changes from
`self.0 * self.0` to `self.0 + self.0`. `Square` also has an inherent
`pub fn area(self) -> u32` returning `9`. The test pins `Square(3).area()` to
`9`.

## When

```bash
cargo xtask fixtures rust_trait_inherent_by_value_twin
```

## Then

Exactly one `return_value` finding is not promoted. The related test's
relation is `weak_token_substring`, not `direct_owner_call`: the receiver is
an owned `Square`, so method probing starts at the by-value step and finds
the inherent `area(self)` before the `&` step reaches the trait impl.

The authored crate compiles and its one test passes on both sides of the
diff, because `Square(3).area()` runs the inherent method.

## Must Not

- Claim the test reaches the changed trait impl method.
- Treat a borrowed receiver (`(&Square(3)).area()`), which does reach the
  trait impl, the same way. The unit tests in
  `related_tests/dispatch_target/tests.rs` keep it direct.
