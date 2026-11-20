# Golden Output Changes

## Pending — typescript_array_is_array_guard_skipped_input (1)

Reason:
RIPR-SPEC-0243: a literal input that fails the Array.isArray / typeof guard never enters the changed arm (#15261)

Command:
`cargo xtask goldens bless typescript_array_is_array_guard_skipped_input --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
