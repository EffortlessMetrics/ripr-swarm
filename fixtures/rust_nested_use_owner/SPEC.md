# Fixture: rust_nested_use_owner

Spec: RIPR-SPEC-0197

## Given

A nested owner leaf remains local beside an unrelated absolute dependency leaf. The owner threshold changes from `x > 100` to
`x > 101`. Far-side owner calls use 50 and 500; the root export is called at 101.

## When

```bash
cargo xtask fixtures rust_nested_use_owner
```

## Then

The root call supplies the owner equality-boundary input; the return-value
finding retains its current assertion-strength classification. The exact
`x == 101` activation row is present and no equality-boundary gap is added.

## Must Not

Credit a dependency input, a renamed owner leaf, or a cfg-disabled owner export
as the changed owner. Preserve explicit-leaf precedence and ambiguity refusals.
