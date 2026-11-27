# Fixture: rust_trait_impl_alias_macro_method

Spec: RIPR-SPEC-0108
Issue: #15956

## Given

The production `impl Shape for Square { fn area(&self) }` changes from
`self.side * self.side` to `self.side + self.side`. `pub type Tile = Square;`
names the same type, and `impl Tile { by_value!(); }` calls a macro that
writes `pub fn area(self) -> u32 { 9 }`. A module `unrelated` declares its own
`struct Tile`. The test `square_area` pins
`Square::with_side(3).area()` to `9`.

## When

```bash
cargo xtask fixtures rust_trait_impl_alias_macro_method
```

## Then

Exactly one `return_value` finding is not promoted. The related test's
relation is `weak_token_substring`, not `direct_owner_call`: a macro called
inside an impl whose self type is not a declared type may write an inherent
`area` on `Square`, and here it does.

The authored crate compiles and its one test passes on both sides of the
diff, because `Square::with_side(3).area()` runs the macro-written inherent
method.

## Must Not

- Claim the test reaches the changed trait impl method.
- Treat the impl on the alias as an impl on a different type, including
  because another module declares a `struct` named `Tile`.
