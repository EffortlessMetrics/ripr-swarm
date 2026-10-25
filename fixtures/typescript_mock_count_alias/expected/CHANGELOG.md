# Golden Output Changes

## Pending — typescript_mock_count_alias (1)

Reason:
RIPR-SPEC-0243 decision 22 (#10174): count-only aliases, optional chains and literal computed paths cannot see a wrong call argument; the argument pin is the retained positive control.

Command:
`cargo xtask goldens bless typescript_mock_count_alias --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_mock_count_alias (2)

Reason:
RIPR-SPEC-0243 decision 22 (#10174): add full human projection for independent classification/oracle parity; extracted equality metadata stays exact/strong while effective count evidence and the verdict stay weak.

Command:
`cargo xtask goldens bless typescript_mock_count_alias --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
