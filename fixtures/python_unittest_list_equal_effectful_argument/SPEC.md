# Fixture: python_unittest_list_equal_effectful_argument

Spec: RIPR-SPEC-0028

## Given

The list return changes `[1, 1]` to `[1, 2]`, but the unittest assertion calls
`labels(abort())`. The imported argument function raises `SkipTest` before
`labels` runs. Native discovery registers one skipped test for both returns.
This independently authored #7312 review control shares the CPython source pin
and literal oracle described in `../python_unittest_list_equal/ORACLE.md`.

## When

`cargo xtask fixtures python_unittest_list_equal_effectful_argument`

## Then

One `static_unknown` finding retains an unknown custom-helper oracle. The new
zero-argument support boundary cannot infer owner reach through argument calls.

## Must Not

Report `exposed`, a strong exact oracle, or discrimination yes. The governed
RIPR-SPEC-0108 honesty corpus independently forbids promotion. A skipped test
does not supply a native rejection witness.
