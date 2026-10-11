# Golden Output Changes

## Pending — python_unittest_list_equal (1)

Reason:
RIPR-SPEC-0028: issue 7312 independently authored exact [1, 2] list oracle rejects [1, 1]; baseline behavioral red and native/CLI controls justify one new Python fixture

Command:
`cargo xtask goldens bless python_unittest_list_equal --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
