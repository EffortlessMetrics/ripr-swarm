# Golden Output Changes

## Pending — typescript_then_inline_receiver_shadow (1)

Reason:
RIPR-SPEC-0246 rule 2 and RIPR-SPEC-0108: an inline receiver shadow cannot
confirm the imported owner's changed return. Actual wrong-owner Vitest controls
continue to pass; the matching plain resolved-value control fails (#6798).

Command:
`ripr check --root fixtures/typescript_then_inline_receiver_shadow/input --diff fixtures/typescript_then_inline_receiver_shadow/diff.patch --format json --mode fast`

The scoped CLI JSON, human and human-full projections use the repository's
cwd/newline and canonical-next-action normalization. Aggregate `goldens bless`
and `goldens check` were not executed in this lane.

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
