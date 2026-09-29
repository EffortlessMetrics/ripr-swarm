# Golden Output Changes

## Pending — ts_repair_packet_boundary_parameter_written (1)

Reason:
RIPR-SPEC-0087: judge a parameter-pair boundary through typescript_boundary_parameters evidence and fail closed without it (#4759)

Command:
`cargo xtask goldens bless ts_repair_packet_boundary_parameter_written --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
