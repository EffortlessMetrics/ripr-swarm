# Fixture: rust_constant_tail_change

Spec: RIPR-SPEC-0197

## Given

Several functions whose body is one returned expression change in one diff
(#14852, cargo-mutants' FnValue genre):

- `ready()` and `idle()` change `true` to `false`; `items()` changes
  `vec![1, 2]` to `vec![]`; `bytes()` changes `vec![b'a', b'b']` to
  `vec![b'x', b'y']`. Each base and candidate body is one constant, and the
  two constants differ.
- `enabled(flag)` changes the named tail `flag` to `false`.
- `gate(flag)` and `pick(x)` drop an early return and keep a constant tail,
  so the paired removed line is a constant but the base body is not.
- `off()` changes `false` to `!true`, which folds to the same value.
- `neg`, `both` and `px` change to tails that name a parameter or field
  (`-delta`, `a & b`, `p.x`).

Tests pin `ready`, `items`, `bytes` and every counterexample with an exact
`assert_eq!`; `idle` is only called.

## When

`cargo xtask fixtures rust_constant_tail_change` runs the public analyzer.

## Then

- `ready`, `items` and `bytes` read `exposed`; `idle` reads
  `reachable_unrevealed`.
- The `bytes` sink names the raw tail `vec![b'x', b'y']`.
- Every counterexample (`enabled`, `gate`, `pick`, `off`, `neg`, `both`,
  `px`) reads a class other than `exposed`.

Known pre-existing reading, outside this fixture's claim: the deleted
`if x == 0 { return Vec::new(); }` line of `pick` is its own base-side
`return_value` probe and reads `exposed` on the base revision too, although
`pick(0)` returns an empty vector on both sides. The constant-tail rule does
not produce that sink or witness; this fixture records the reading so a fix
for it shows as intended drift.

## Must Not

- Credit a tail that names a parameter, local, constant or field.
- Decide from the probe's paired removed line instead of the base-side body.
- Credit two constants that fold to the same value.
- Emit masked tail text (blanked char literals) in any output.
