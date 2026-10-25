# Golden Output Changes

## Pending — out_parameter_named_empty_no_promote (1)

Reason:
RIPR-SPEC-0351 #10058: named empty payload adds no data; native deletion control leaves prefix assertion unchanged; independent corpus forbids promotion

Command:
`cargo xtask goldens bless out_parameter_named_empty_no_promote --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
