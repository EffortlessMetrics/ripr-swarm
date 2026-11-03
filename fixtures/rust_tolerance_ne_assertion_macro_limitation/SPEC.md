# Fixture: rust_tolerance_ne_assertion_macro_limitation

Spec: RIPR-SPEC-0120

## Given

A Rust crate where an integration test directly reaches the changed owner, then
checks the result with an inequality tolerance macro spelled like
`approx::assert_relative_ne!`. RIPR does not classify the macro as an oracle.

## When

```bash
cargo xtask fixtures rust_tolerance_ne_assertion_macro_limitation
```

or:

```bash
ripr check --root fixtures/rust_tolerance_ne_assertion_macro_limitation/input --diff fixtures/rust_tolerance_ne_assertion_macro_limitation/diff.patch --mode fast
```

## Then

`ripr` keeps the finding reachable-but-undiscriminated with the named macro
limitation, and the next step is the inequality wording: the macro passes when
the values differ by more than the tolerance, so the original value must stay
outside the tolerance and the changed value must fall inside it (#6645).

## Must Not

- Advise a tighter tolerance for an inequality macro: that makes the assertion
  easier to pass and hides the change.
- Promote the tolerance macro as a strong or exact oracle from its spelling.
- Emit a repair packet for this limitation.
- Claim the result is clean.
