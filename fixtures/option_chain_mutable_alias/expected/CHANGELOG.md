# Golden Output Changes

## Pending — option_chain_mutable_alias (1)

Reason:
RIPR-SPEC-0158 and RIPR-SPEC-0108: native-valid generic and alias counterexamples must retain unknown, with nonempty independent refusal charters

Command:
`cargo xtask goldens bless option_chain_mutable_alias --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — option_chain_mutable_alias (2)

Reason:
RIPR-SPEC-0158 and RIPR-SPEC-0108: extra whitespace does not make a mutable identifier immutable; retain the native-valid alias refusal

Command:
`cargo xtask goldens bless option_chain_mutable_alias --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — option_chain_mutable_alias (3)

Reason:
RIPR-SPEC-0158 and RIPR-SPEC-0108: bind the mutable alias fixture diff to the exact extra-whitespace source while preserving unknown verdicts

Command:
`cargo xtask goldens bless option_chain_mutable_alias --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — option_chain_mutable_alias (4)

Reason:
RIPR-SPEC-0001 / RIPR-SPEC-0157 / RIPR-SPEC-0158: preserve the unresolved value-transfer refusal and remove unverified literal-grid advice; only the reviewed tone suffix and native human wrapping change on frozen f90c6d2104758630db9c026cd48e2e9222d15888.

Command:
`cargo xtask goldens bless option_chain_mutable_alias --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
