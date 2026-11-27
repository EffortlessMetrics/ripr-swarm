# Fixture: self_comparison_float_nan_stays_weak

Spec: RIPR-SPEC-0094

## Given

`add` pushes into a `Vec<f64>`. The change rewrites `v.push(x)` to
`v.push(x / x)`. The test calls `add(&mut b.0, 0.0)` and asserts
`assert_eq!(b.0, b.0.clone())`.

At runtime this assertion does discriminate the change: the old code pushes
`0.0` and the comparison holds, while the new code pushes `0.0 / 0.0`, which is
NaN, and `[NaN] == [NaN]` is false, so the test fails.

## When

```bash
ripr check --root fixtures/self_comparison_float_nan_stays_weak/input \
           --diff fixtures/self_comparison_float_nan_stays_weak/diff.patch --mode fast
```

## Then

The documented contrast for the #13037 self-comparison heuristic. ripr does
not establish whether equality on the compared place is reflexive, so it reads
the copy comparison conservatively: both findings stay `weakly_exposed` and the
related row reads `exact_value` / `weak`. This is a known underclaim
(#16227), never a credit the assertion did not earn.

The findings' expected classes (read against `expected/check.json` by
`check-fixture-contracts`, #12928):

```expected-classes
probe:src_lib.rs:call_deletion:ad2dbd4c -> weakly_exposed
probe:src_lib.rs:side_effect:962e7ca0 -> weakly_exposed
```

## Must Not

- Report `exposed` from the copy comparison.
- Claim the assertion cannot fail.
- Claim runtime mutation adequacy.
