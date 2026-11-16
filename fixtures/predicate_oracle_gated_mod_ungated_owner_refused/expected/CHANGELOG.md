# Golden Output Changes

## Pending — predicate_oracle_gated_mod_ungated_owner_refused (1)

Reason:
RIPR-SPEC-0197: an ungated owner reached through a gated mod keeps the module-declaration refusal, so the predicate equality is not credited (#14537, #14599)

Command:
`cargo xtask goldens bless predicate_oracle_gated_mod_ungated_owner_refused --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
