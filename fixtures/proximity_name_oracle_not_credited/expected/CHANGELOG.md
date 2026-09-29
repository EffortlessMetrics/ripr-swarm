# Golden Output Changes

## Pending — proximity_name_oracle_not_credited (1)

Reason:
RIPR-SPEC-0094 / #4486: new fixture; a test related only by name proximity cannot supply the credited oracle when another related test supplies reach

Command:
`cargo xtask goldens bless proximity_name_oracle_not_credited --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
