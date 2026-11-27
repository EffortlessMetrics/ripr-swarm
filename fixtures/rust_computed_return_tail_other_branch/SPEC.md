# Fixture: rust_computed_return_tail_other_branch

Spec: RIPR-SPEC-0443

## Given

The same change as `rust_computed_return_tail_reached` (`base * 3` to
`base * 2` under `if express`), but the only test pins the standard branch:
`shipping_cost(100, 10, false) == 400`. That call returns `base` and never
evaluates the changed expression.

## When

```bash
cargo xtask fixtures rust_computed_return_tail_other_branch
```

or:

```bash
ripr check --root fixtures/rust_computed_return_tail_other_branch/input --diff fixtures/rust_computed_return_tail_other_branch/diff.patch --mode fast
```

## Then

The changed return is not promoted to `exposed` (#13037): the exact pin
observes the other branch's value, so it cannot confirm the changed one.

```expected-classes
probe:src_lib.rs:return_value:f222f9b2 -> weakly_exposed
```

## Must Not

- Credit the reached-branch pin rule from a call that takes the other
  branch.
