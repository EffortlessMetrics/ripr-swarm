# Golden Output Changes

## Pending — typescript_exported_expect_lookalike (1)

Reason:
RIPR-SPEC-0243 rule 3: exported local expect declarations retain their local identity and cannot supply runner oracle credit; four related tests stay unknown and the changed return stays weakly_exposed.

Command:
`cargo xtask goldens bless typescript_exported_expect_lookalike --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
