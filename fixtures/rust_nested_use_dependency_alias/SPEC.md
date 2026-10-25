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

The return-value finding retains the missing equality-boundary discriminator
and remains below `exposed`.

## Must Not

Credit a dependency input, a renamed owner leaf, or a cfg-disabled owner export
as the changed owner. Preserve explicit-leaf precedence and ambiguity refusals.
