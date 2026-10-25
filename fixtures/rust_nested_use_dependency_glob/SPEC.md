# Fixture: rust_nested_use_dependency_glob

Spec: RIPR-SPEC-0197

## Given

A nested dependency glob cannot supply the owner boundary input. The owner threshold changes from `x > 100` to
`x > 101`. Far-side owner calls use 50 and 500; the root export is called at 101.

## When

```bash
cargo xtask fixtures rust_nested_use_dependency_glob
```

## Then

The return-value finding retains the missing equality-boundary discriminator
and remains below `exposed`.

## Must Not

Credit a dependency input, a renamed owner leaf, or a cfg-disabled owner export
as the changed owner. Preserve explicit-leaf precedence and ambiguity refusals.
