# Golden Output Changes

## Pending — python_assertion_helper_method_rebinding (1)

Reason:
RIPR-SPEC-0238: field-plus-literal helper credit preserves receiver, callable and expectation identity; native surviving-mutant controls and RIPR-SPEC-0108 reject false promotions (#9594)

Command:
`cargo xtask goldens bless python_assertion_helper_method_rebinding --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_assertion_helper_method_rebinding (2)

Reason:
RIPR-SPEC-0238 and RIPR-SPEC-0108: retain one weak finding for field-helper identity, literal-expected and activation controls through the landed resolver.

Command:
`cargo xtask goldens bless python_assertion_helper_method_rebinding --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_assertion_helper_method_rebinding (3)

Reason:
RIPR-SPEC-0209 / RIPR-SPEC-0238 (#4518, #11050): bind the Bash verification
action to the analyzed repository with `cd -P --`, retaining the existing
`tests/test_stats.py::test_writes` selector. This manual human-output
reconciliation leaves fixture inputs, `expected/check.json` and the
`weakly_exposed` classification unchanged.

Updated:
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_assertion_helper_method_rebinding (4)

Reason:
RIPR-SPEC-0122 plain wording: non-Rust terminal says spot list / missing check, Python preview next step says missing check (#11619)

Command:
`cargo xtask goldens bless python_assertion_helper_method_rebinding --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_assertion_helper_method_rebinding (5)

Reason:
RIPR-SPEC-0118 / #10692: plain wording for seam and discriminator in default check and repair-start text; verdicts, identity and JSON fields unchanged

Command:
`cargo xtask goldens bless python_assertion_helper_method_rebinding --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_assertion_helper_method_rebinding (6)

Reason:
RIPR-SPEC-0122: explain missing input or assertion honestly

Command:
`cargo xtask goldens bless python_assertion_helper_method_rebinding --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
