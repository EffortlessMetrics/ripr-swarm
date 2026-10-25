# Fixture: python_dunder_truthiness_integer

Spec: RIPR-SPEC-0238

## Given

An authored plain integer constructor and a one-comparison `Money.__bool__`
change from `self.cents != 0` to `self.cents > 0` (#9795).
The test imports `Money` from `shapes.money` and asserts `Money(-1)`.

## When

`cargo xtask fixtures python_dunder_truthiness_integer`

## Then

The single finding is `exposed`: `assert Money(-1)` passes before the edit and fails after it.

## Must Not

Credit unresolved primitive values, unchanged truth values, or a class imported
from another module. Equality guards and string conversion stay outside this slice.

## Retained blocking review controls

The `evidence/cross-file-*.receipt.json` files retain exact source, raw-output
hashes and CPython 3.12.14 manual module-collection/test-call outcomes for
candidate `d4910e91698b1930fdc0fc6a4589ad686cfc179a`. Each paired program passes
before and after the edit, while that candidate's native CLI reports `exposed`.
The adjacent sanitized native reports are enrolled in RIPR-SPEC-0108 with
`must_not_promote` and expected `static_unknown`; their current failures are
intentional rejection witnesses, not expectations to re-bless.

These controls cover a separate collected module's direct writer, hidden member
names, Python-normalized identifiers, and a generated-name test omitted by source
discovery. The prototype remains `REPAIR_REQUIRED` until project mutation and
source-discovery admission can withhold those credits. They do not qualify a
general namespace or graph repair. No full pytest execution is claimed.
