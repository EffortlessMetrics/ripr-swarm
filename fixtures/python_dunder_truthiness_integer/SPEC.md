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
This credit uses the fixture runner's explicit `--diff` live source view. Other
source modes retain abstention for this capability.

## Must Not

Credit unresolved primitive values, unchanged truth values, or a class imported
from another module. Equality guards and string conversion stay outside this slice.

## Retained historical rejection controls

The `evidence/cross-file-*.receipt.json` files retain exact source, raw-output
hashes and CPython 3.12.14 manual module-collection/test-call outcomes for
candidate `d4910e91698b1930fdc0fc6a4589ad686cfc179a`. Each paired program passes
before and after the edit, while that candidate's native CLI reports `exposed`.
The adjacent sanitized native reports and `d491-honesty-rejection.txt` preserve
the original four failing honesty cases/eight violations. Their observed
classifications and hashes remain unchanged.

These controls cover a separate collected module's direct writer, hidden member
names, Python-normalized identifiers, and a generated-name test omitted by source
discovery. They do not qualify a general namespace or graph repair. No full
pytest execution is claimed.

## Current-producer regression

The `python_dunder_truthiness_cross_file_{direct,hidden,unicode,generated}`
fixtures retain byte-identical source/diff inputs from those receipts. Their
fresh native outputs are `static_unknown` under the physical-isolation admission.
The same RIPR-SPEC-0108 case IDs retain `must_not_promote` and expected
`static_unknown`, now checking these executable fixtures. This isolated positive
remains `exposed`; the frozen historical reports have not been reclassified.
