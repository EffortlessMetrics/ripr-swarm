# Golden Output Changes

## Pending

Reason:
RIPR-SPEC-0094: SideEffect probe with order_id token_match does NOT emit observation_unverified; initial golden for the anti-over-correction proof fixture

Command:
`cargo xtask goldens bless observation_verified_side_effect --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
