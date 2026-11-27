# Fixture: rust_trait_impl_std_method_takes_receiver

Spec: RIPR-SPEC-0108
Issue: #15956

## Given

The production `impl Stock for Shelf { fn count(&self) }` changes from
`self.left * 2` to `self.left + 1`. `Shelf` also implements `Iterator`, whose
provided `count(self)` takes the receiver by value. The test `shelf_count`
pins `Shelf::stocked(3).count()` to `3`.

## When

```bash
cargo xtask fixtures rust_trait_impl_std_method_takes_receiver
```

## Then

Exactly one `return_value` finding is not promoted. The related test's
relation is `weak_token_substring`, not `direct_owner_call`: the receiver is
an owned `Shelf`, and a standard trait declares a method named `count`, so the
call is not read as running `Stock::count`.

The authored crate compiles and its one test passes on both sides of the
diff, because `Shelf::stocked(3).count()` runs `Iterator::count`.

## Must Not

- Claim the test reaches the changed trait impl method.
- Read the `Iterator` impl's absence of a written `count` as evidence that no
  other method takes the call.
