# Golden Output Changes

## Pending — typescript_finally_completion (1)

Reason:
RIPR-SPEC-0243: a finalizer that returns can discard an awaited matcher rejection, so the try body cannot supply exposure credit (#8508); native runtime and literal CLI controls establish the weak unknown oracle.

Command:
`cargo xtask goldens bless typescript_finally_completion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
