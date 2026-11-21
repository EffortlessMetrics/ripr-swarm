# Fixture: rust_tuple_guard_other_branch

Spec: RIPR-SPEC-0443

## Given

The edit swaps the operands in the then arm of `if a.0 > 100`. The
only assertion calls the free function with `Money(2)`.

## When

Run `cargo xtask fixtures rust_tuple_guard_other_branch`.

## Then

The test pins the zero returned by the else arm. The single return-value
finding reads `weakly_exposed`.

```expected-classes
probe:src_lib.rs:return_value:14002d8e -> weakly_exposed
```

## Must Not

- Give the assertion credit for running the edited addition.
