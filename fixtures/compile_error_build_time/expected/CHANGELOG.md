# Golden Output Changes

## Pending — compile_error_build_time (1)

Reason:
RIPR-SPEC-0411: #15933 a changed compile_error! line, one-line or the message line of a split invocation, is one static_unknown finding with compile_time_invariant

Command:
`cargo xtask goldens bless compile_error_build_time --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
