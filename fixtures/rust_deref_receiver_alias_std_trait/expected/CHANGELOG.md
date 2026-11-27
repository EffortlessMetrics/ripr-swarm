# Golden Output Changes

## Pending — rust_deref_receiver_alias_std_trait (1)

Reason:
RIPR-SPEC-0001: a Deref wrapper call is credited only when nothing on the wrapper may answer it first (#15277)

Command:
`cargo xtask goldens bless rust_deref_receiver_alias_std_trait --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_deref_receiver_alias_std_trait (2)

Reason:
RIPR-SPEC-0001: the Deref wrapper reading shares the closed-dispatch workspace verdict and its fixtures drop the trailing blank diff line and the probe member name (#15277)

Command:
`cargo xtask goldens bless rust_deref_receiver_alias_std_trait --reason "..."`

Updated:
- `expected/check.json`
