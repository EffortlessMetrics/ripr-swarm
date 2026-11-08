# Golden Output Changes

## Pending — cross-surface annotation conformance (#12925)

Add the existing finding projection as `sarif.json` and `github.txt` under
RIPR-SPEC-0008 cross-surface conformance. Class, canonical identity,
configured severity and refusal guidance come from the existing producer;
the check/human goldens and analyzer behavior are unchanged.

## Pending — typescript_private_method_unknown (1)

Reason:
RIPR-SPEC-0243: pin truthful unknown reach and advisory output for private methods without tests

Command:
`cargo xtask goldens bless typescript_private_method_unknown --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_private_method_unknown (2)

Reason:
Regenerate from renderer after hand-edit; RIPR-SPEC-0243 private_member_reach_unknown guidance wording

Command:
`cargo xtask goldens bless typescript_private_method_unknown --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_private_method_unknown (3)

Reason:
RIPR-SPEC-0246: plain wording for TypeScript preview check text (#11619)

Command:
`cargo xtask goldens bless typescript_private_method_unknown --reason "..."`

Updated:
- `expected/check.json`
- `expected/human-full.txt`

## Pending — typescript_private_method_unknown (4)

Reason:
RIPR-SPEC-0122: drill-in and repair-start command lines carry consumer labels naming their id domain (#12431)

Command:
`cargo xtask goldens bless typescript_private_method_unknown --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_private_method_unknown (5)

Reason:
RIPR-SPEC-0088: retain source-file provenance for advisory TypeScript-family unknown findings; verdicts and repair authority unchanged (#12930)

Command:
`cargo xtask goldens bless typescript_private_method_unknown --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

The incidental new `human.txt` copied by the blessing command was discarded;
this update retains the existing human-surface inventory.

## Pending — typescript_private_method_unknown (6)

Reason:
RIPR-SPEC-0423: additive evidence_stages rows and agent stages= line; human rendering unchanged

Command:
`cargo xtask goldens bless typescript_private_method_unknown --reason "..."`

Updated:
- `expected/check.json`
- `expected/human-full.txt`
