# Golden Output Changes

## Pending — typescript_witness_dead_argument (1)

Reason:
RIPR-SPEC-0027 boundary witness guard #4102: literal in argument the single-param owner never reads no longer witnesses (was exposed, now weakly_exposed)

Command:
`cargo xtask goldens bless typescript_witness_dead_argument --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
