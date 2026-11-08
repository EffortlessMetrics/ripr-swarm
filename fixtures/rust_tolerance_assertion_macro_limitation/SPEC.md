# Fixture: rust_tolerance_assertion_macro_limitation

Spec: RIPR-SPEC-0120

## Given

A Rust crate where an integration test directly reaches the changed owner, then
checks the result with a floating-point tolerance macro spelled like
`approx::assert_relative_eq!`. RIPR does not classify the macro as an oracle.

## When

```bash
cargo xtask fixtures rust_tolerance_assertion_macro_limitation
```

or:

```bash
ripr check --root fixtures/rust_tolerance_assertion_macro_limitation/input --diff fixtures/rust_tolerance_assertion_macro_limitation/diff.patch --mode fast
```

## Then

`ripr` keeps the finding reachable-but-undiscriminated with the same named
limitation as any unresolved assertion macro, and the next step says the macro
reads like a floating-point tolerance comparison whose tolerance and the size of
the change ripr cannot establish (#6645).

## Must Not

- Promote the tolerance macro as a strong or exact oracle from its spelling.
- Claim the tolerance is tighter than the change.
- Emit a repair packet for this limitation.
- Claim the result is clean.
