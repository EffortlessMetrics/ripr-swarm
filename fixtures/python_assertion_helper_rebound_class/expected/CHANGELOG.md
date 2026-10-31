# Golden Output Changes

## Pending — python_assertion_helper_rebound_class (1)

Reason:
RIPR-SPEC-0238: field-plus-literal helper credit preserves receiver, callable and expectation identity; native surviving-mutant controls and RIPR-SPEC-0108 reject false promotions (#9594)

Command:
`cargo xtask goldens bless python_assertion_helper_rebound_class --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_assertion_helper_rebound_class (2)

Reason:
RIPR-SPEC-0238 and RIPR-SPEC-0108: retain one weak finding for field-helper identity, literal-expected and activation controls through the landed resolver.

Command:
`cargo xtask goldens bless python_assertion_helper_rebound_class --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_assertion_helper_rebound_class (3)

Reason:
RIPR-SPEC-0209 / RIPR-SPEC-0238 (#4518, #11050): bind the Bash verification
action to the analyzed repository with `cd -P --`, retaining the existing
`tests/test_stats.py::test_writes` selector. This manual human-output
reconciliation leaves fixture inputs, `expected/check.json` and the
`weakly_exposed` classification unchanged.

Updated:
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_assertion_helper_rebound_class (4)

Reason:
#10697: plain wording for discriminator and seam in default human output; mirror the shared evidence/legend/no-seam rewording and reword TS preview discriminator labels to missing-check wording (safe, observed-advisory and exact-error-variant)

Command:
`cargo xtask goldens bless python_assertion_helper_rebound_class --reason "..."`

Updated:
- `expected/human.txt`
