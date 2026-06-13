# Golden Output Changes

## Pending

Reason:
RIPR-SPEC-0094: CallDeletion probe with result_key token_match does NOT emit observation_unverified; initial golden for the anti-over-correction proof fixture

Command:
`cargo xtask goldens bless observation_verified_call_deletion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
