# Golden Output Changes

## Pending — rust_deref_receiver_macro_method (1)

Reason:
RIPR-SPEC-0001: a Deref wrapper call is credited only when nothing on the wrapper may answer it first (#15277)

Command:
`cargo xtask goldens bless rust_deref_receiver_macro_method --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_deref_receiver_macro_method (2)

Reason:
RIPR-SPEC-0001: the Deref wrapper reading shares the closed-dispatch workspace verdict and its fixtures drop the trailing blank diff line and the probe member name (#15277)

Command:
`cargo xtask goldens bless rust_deref_receiver_macro_method --reason "..."`

Updated:
- `expected/check.json`
