# Golden Output Changes

## Pending

Reason:
Align new source-promotion verifier fixture with its intentional exact-join validation input

Command:
`cargo xtask goldens bless source_promotion_verification --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0122: the source/W7 join adopts the frozen W7 bounded human check renderer and RIPR-SPEC-0147 parser-shape probe canonicalization, so this source-authored fixture records the combined-tree analyzer output rather than the source-parent renderer output

Command:
`cargo xtask goldens bless source_promotion_verification --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — source_promotion_verification (2)

Reason:
RIPR-SPEC-0371: ripr-main merge sync re-bless to swarm analyzer output; ripr-side behavior not carried, see merge PR residuals

Command:
`cargo xtask goldens bless source_promotion_verification --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
