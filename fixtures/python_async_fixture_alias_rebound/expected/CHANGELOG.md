# Golden Output Changes

## Pending — python_async_fixture_alias_rebound (1)

Reason:
RIPR-SPEC-0238: pin the refusal after plain pytest rebinds an async fixture alias (#14846)

Command:
`cargo xtask goldens bless python_async_fixture_alias_rebound --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_async_fixture_alias_rebound (2)

Reason:
RIPR-SPEC-0238: include the complete human report for the independent rebound provider check (#14846)

Command:
`cargo xtask goldens bless python_async_fixture_alias_rebound --reason "..."`

Updated:
- `expected/human-full.txt`
