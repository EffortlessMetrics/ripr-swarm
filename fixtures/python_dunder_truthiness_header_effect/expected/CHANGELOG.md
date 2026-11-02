# Golden Output Changes

## Pending — python_dunder_truthiness_header_effect (1)

Reason:
RIPR-SPEC-0238 rule 26: independent truthiness nonpromotion control for #9795

Command:
`cargo xtask goldens bless python_dunder_truthiness_header_effect --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_dunder_truthiness_header_effect (2)

Reason:
RIPR-SPEC-0238: preserve complete discriminator and analyzer-limit prose after landed renderer #10258; classifications and source scopes are unchanged

Command:
`cargo xtask goldens bless python_dunder_truthiness_header_effect --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

Verification note: This update also restores two previously stale full-human Next guidance lines. The frozen BE75 producer and integrated renderer produce identical full-human bytes after the fixture runner's path normalization; classifications and assertions are unchanged.

## Pending — python_dunder_truthiness_header_effect (3)

Reason:
RIPR-SPEC-0122: no-gap-ID refusal wording re-blessed now that the goldens gate compares it verbatim (#12003)

Command:
`cargo xtask goldens bless python_dunder_truthiness_header_effect --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_dunder_truthiness_header_effect (4)

Reason:
RIPR-SPEC-0118 / #10692: plain wording for seam and discriminator in default check and repair-start text; verdicts, identity and JSON fields unchanged

Command:
`cargo xtask goldens bless python_dunder_truthiness_header_effect --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_dunder_truthiness_header_effect (5)

Reason:
RIPR-SPEC-0122: explain missing input or assertion honestly

Command:
`cargo xtask goldens bless python_dunder_truthiness_header_effect --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
