# Fixture: rust_nested_use_dependency_alias

Spec: RIPR-SPEC-0197

## Given

A nested explicit dependency leaf renamed across a newline beats the owner glob. The owner threshold changes from `x > 100` to
`x > 101`. Far-side owner calls use 50 and 500; the root export is called at 101.

## When

```bash
cargo xtask fixtures rust_nested_use_dependency_alias
```

## Then

The return-value infection-stage summary retains the missing equality-boundary
discriminator, and no exact `x == 101` activation row is credited.

Final classification remains `static_unknown` on the integrated main because
the separate same-name path-qualifier false refusal (#10604) withholds final gaps
and clears final missing-discriminator arrays. These tests use direct calls, not
function values; that limitation is recorded as an inherited defect. Input
evidence, rather than final classification or the cleared array, discriminates
this re-export repair.

## Must Not

Credit a dependency input, a renamed owner leaf, or a cfg-disabled owner export
as the changed owner. Preserve explicit-leaf precedence and ambiguity refusals.
