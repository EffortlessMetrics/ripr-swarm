# Golden Output Changes

## Pending — ts_trailing_whitespace_in_template (1)

Reason:
RIPR-SPEC-0027: new fixture for #10706 trailing comment/whitespace

Command:
`cargo xtask goldens bless ts_trailing_whitespace_in_template --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending - ts_trailing_whitespace_in_template (2)

Reason:
#10697: plain wording for discriminator and seam in default human output; mirror the shared evidence/legend/no-seam rewording and reword TS preview discriminator labels to observed/safe checks

Command:
`cargo xtask goldens bless ts_trailing_whitespace_in_template --reason "..."`

Updated:
- `expected/human.txt`
