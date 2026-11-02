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

## Pending — typescript_then_inline_receiver_shadow (2)

Reason:
RIPR-SPEC-0246: re-rendered on the merged renderer (bounded-evidence legend, canonical_gap_id, renamed summary wording); classification unchanged

Command:
`cargo xtask goldens bless typescript_then_inline_receiver_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_then_inline_receiver_shadow (3)

Reason:
RIPR-SPEC-0246: re-render after merging main; plain wording for TypeScript preview check text (#11619)

Command:
`cargo xtask goldens bless typescript_then_inline_receiver_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_then_inline_receiver_shadow (4)

Reason:
RIPR-SPEC-0122: plain wording for the Evidence stage name and the stage and state legend in the check digest (#10692)

Command:
`cargo xtask goldens bless typescript_then_inline_receiver_shadow --reason "..."`

Updated:
- `expected/human.txt`
