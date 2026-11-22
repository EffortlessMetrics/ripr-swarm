# Golden Output Changes

## Pending — typescript_plain_class_entry_reach (1)

Reason:
RIPR-SPEC-0243: #15928 methods of a module-private class that an exported entry constructs and calls, and a function only such a method calls, are reached through the entry as static_unknown; a method off the call path and a class whose name the entry rebinds stay no_static_path

Command:
`cargo xtask goldens bless typescript_plain_class_entry_reach --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
