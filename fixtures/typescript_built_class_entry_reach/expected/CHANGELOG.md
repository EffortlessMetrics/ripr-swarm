# Golden Output Changes

## Pending — typescript_built_class_entry_reach (1)

Reason:
RIPR-SPEC-0243: #15928 a class built in an immediately invoked function and a function passed by reference to a synchronous array callback are reached through the exported entry as static_unknown; a deferred reference stays no_static_path

Command:
`cargo xtask goldens bless typescript_built_class_entry_reach --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_built_class_entry_reach (2)

Reason:
RIPR-SPEC-0243: #15928 the deferred control passes arguments to the timer so the fixture module runs as written; readings unchanged

Command:
`cargo xtask goldens bless typescript_built_class_entry_reach --reason "..."`

Updated:
- `expected/check.json`
