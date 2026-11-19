# Golden Output Changes

## Pending — predicate_oracle_gated_mod_ungated_owner_refused (1)

Reason:
RIPR-SPEC-0197: an ungated owner reached through a gated mod keeps the module-declaration refusal, so the predicate equality is not credited (#14537, #14599)

Command:
`cargo xtask goldens bless predicate_oracle_gated_mod_ungated_owner_refused --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — predicate_oracle_gated_mod_ungated_owner_refused (2)

Reason:
RIPR-SPEC-0197: re-bless the gated-mod ungated-owner-refused fixture onto the shortened unknown-class next-step hedge from #14780

Command:
`cargo xtask goldens bless predicate_oracle_gated_mod_ungated_owner_refused --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
