# Golden Output Changes

## Pending — predicate_oracle_gated_mod_admitted (1)

Reason:
RIPR-SPEC-0197: unknown-availability gated module admits a predicate equality; the exact-value far oracle is retained as weak evidence (#14537)

Command:
`cargo xtask goldens bless predicate_oracle_gated_mod_admitted --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — predicate_oracle_gated_mod_admitted (2)

Reason:
RIPR-SPEC-0197: the gated mod admits a predicate equality only under the #14463 shared-gate rule, so the owner now lives inside the gated module (#14537, #14599)

Command:
`cargo xtask goldens bless predicate_oracle_gated_mod_admitted --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
