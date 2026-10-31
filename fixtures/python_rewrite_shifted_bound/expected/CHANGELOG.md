# Golden Output Changes

## Pending — python_rewrite_shifted_bound (1)

Reason:
RIPR-SPEC-0028 and RIPR-SPEC-0108: an integer-equivalent rewrite needs the row where the shifted bound differs, so a lone equality row is weakly_exposed (#10842)

Command:
`cargo xtask goldens bless python_rewrite_shifted_bound --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending - python_rewrite_shifted_bound (2)

Reason:
#10697: plain wording for discriminator and seam in default human output; mirror the shared evidence/legend/no-seam rewording and reword TS preview discriminator labels to observed/safe checks

Command:
`cargo xtask goldens bless python_rewrite_shifted_bound --reason "..."`

Updated:
- `expected/human.txt`
