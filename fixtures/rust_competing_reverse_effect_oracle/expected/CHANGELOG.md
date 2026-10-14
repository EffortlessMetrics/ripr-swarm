# Golden Output Changes

## Pending — rust_competing_reverse_effect_oracle (1)

Reason:
RIPR-SPEC-0094: competing production sink writes cannot protect a changed effect whose compiling removal survives 3/3 tests

Command:
`cargo xtask goldens bless rust_competing_reverse_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
