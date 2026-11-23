# Fixture: rust_trait_qualified_associated_call

Spec: RIPR-SPEC-0108
Related spec: RIPR-SPEC-0197
Issue: #15277

## Given

The production `impl Make for Square { fn make(n: u32) -> Self }`, a trait
function without `self`, changes from `Square(n * 2)` to `Square(n * 3)`.
The test pins `<Square as Make>::make(2).0` to `4`.

## When

```bash
cargo xtask fixtures rust_trait_qualified_associated_call
```

## Then

Exactly one `return_value` finding reads `exposed`. The related test's
relation is `direct_owner_call`: the fully qualified path names `Square`'s
impl of `Make` outright, as `Square::make(2)` does.

The authored crate compiles, its test passes before the diff and fails after
it (`2 * 3` is `6`, not `4`).

## Must Not

- Credit the impl from a fully qualified call that names another self type
  (`<Line as Make>::make`) or another trait (`<Square as Build>::make`).
- Credit a trait-path call `Make::make(..)` of a function without `self`,
  whose impl the call's arguments do not settle. The unit test
  `given_fully_qualified_call_to_a_function_without_self_then_only_the_named_impl_is_direct`
  in `related_tests.rs` keeps the other impls name-only.
