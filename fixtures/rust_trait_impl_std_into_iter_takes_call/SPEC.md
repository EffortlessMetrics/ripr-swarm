# Fixture: rust_trait_impl_std_into_iter_takes_call

Spec: RIPR-SPEC-0108
Issue: #16040

## Given

The production `impl Peeky for Foo { fn into_iter(&self) }` changes from
`self.0 > 3` to `self.0 >= 3`. It is the only indexed `into_iter`. `Foo` is
an `Iterator`, so the standard blanket impl gives it
`IntoIterator::into_iter(self)`. The test `via_new` calls
`Foo::new(3).into_iter().count()` and pins it to `0`.

## When

```bash
cargo xtask fixtures rust_trait_impl_std_into_iter_takes_call
```

## Then

Exactly one `return_value` finding is not promoted. The related test's
relation is `weak_token_substring`, not `direct_owner_call`: method probing
tries the by-value receiver first, where `IntoIterator::into_iter(self)` takes
the call before the `&self` step reaches `Peeky::into_iter`.

The authored crate compiles and its one test passes on both sides of the
diff. `.count()` on the result only compiles because the call returns the
iterator, not `Peeky`'s `u32`.

## Must Not

- Claim the test reaches the changed `Peeky::into_iter`.
- Withhold the relation for a call through the owner's trait path
  (`Peeky::into_iter(&foo)`), an owner taking `self` by value, a name no
  standard trait has, or the standard trait's own impl. The unit tests in
  `related_tests/std_trait_rival/tests.rs` keep those direct.
