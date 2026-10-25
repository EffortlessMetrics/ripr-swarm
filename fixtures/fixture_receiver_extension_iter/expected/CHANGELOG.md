# Golden Output Changes

## Pending — fixture_receiver_extension_iter (1)

Reason:
RIPR-SPEC-0240 #9766: an extension iterator discards the literal receiver data before the changed projection; preserve the independently confirmed no-credit control

Command:
`cargo xtask goldens bless fixture_receiver_extension_iter --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — fixture_receiver_extension_iter (2)

Reason:
RIPR-SPEC-0240 #9766: normalize raw module and method identity, and retain associated macro dispatch refusal before crediting literal fixture data

Command:
`cargo xtask goldens bless fixture_receiver_extension_iter --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — fixture_receiver_extension_iter (3)

Reason:
RIPR-SPEC-0240 #9766: retain the full human rendering required by the independent honesty corpus expected-class assertion. The retained producer output has one static_unknown finding and no exposed promotion.

Source:
The human-full output from the fixture replay at a8fc17b9d385e0d153a6beb80c3435d02945f1c1, whose analyzer admission code is unchanged in this evidence-only repair.

Updated:
- `expected/human-full.txt`
