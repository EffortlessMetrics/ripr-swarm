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

The return-value infection-stage summary reports the changed boundary input as
unresolved because call identity does not resolve to the owner, no exact
`x == 101` activation row is credited, and the finding classifies as
`infection_unknown`. Withheld call identity is never reported as a fabricated
missing equality-boundary discriminator.

## Must Not

Credit a dependency input, a renamed owner leaf, or a cfg-disabled owner export
as the changed owner. Preserve explicit-leaf precedence and ambiguity refusals.
