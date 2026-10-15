# Golden Output Changes

## Pending — inherent_receiver_free_helper_identity (1)

Reason:
RIPR-SPEC-0005 and RIPR-SPEC-0108: initialize the native foreign helper binding refusal with paired real helper controls. Existing goldens and runtime labels are unchanged.

Command:
`cargo xtask goldens bless inherent_receiver_free_helper_identity --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
