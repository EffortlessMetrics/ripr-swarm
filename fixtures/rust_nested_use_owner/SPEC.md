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

The root call supplies the owner equality-boundary input. The exact
`x == 101` activation row is present and the infection-stage summary no longer
names a missing equality-boundary discriminator.

Final classification remains `static_unknown` on the integrated main because
the separate same-name path-qualifier false refusal (#10604) withholds final gaps
and clears final missing-discriminator arrays. These tests use direct calls, not
function values; that limitation is recorded as an inherited defect. Input
evidence, rather than final classification or the cleared array, discriminates
this re-export repair.

## Must Not

Credit a dependency input, a renamed owner leaf, or a cfg-disabled owner export
as the changed owner. Preserve explicit-leaf precedence and ambiguity refusals.
