# Golden Output Changes

## Pending — python_unittest_list_equal (1)

Reason:
RIPR-SPEC-0028: issue 7312 independently authored exact [1, 2] list oracle rejects [1, 1]; baseline behavioral red and native/CLI controls justify one new Python fixture

Command:
`cargo xtask goldens bless python_unittest_list_equal --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_unittest_list_equal (2)

Reason:
RIPR-SPEC-0028 composition: refresh this new leaf fixture for current config-path rendering; independent class/oracle expectations remain unchanged.

Command:
`cargo xtask goldens bless python_unittest_list_equal --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_unittest_list_equal (3)

Reason:
RIPR-SPEC-0028: preserve AST owner identity for labels (); native one-test wrong/correct witnesses and literal CLI expected exposed/strong/yes distinguish the retained615 failure from the repair. Only the existing positive fixture changes spelling.

Command:
`cargo xtask goldens bless python_unittest_list_equal --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
