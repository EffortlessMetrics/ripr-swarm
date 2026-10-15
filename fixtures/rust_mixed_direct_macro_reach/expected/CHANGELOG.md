# Golden Output Changes

## Pending — rust_mixed_direct_macro_reach (1)

Reason:
RIPR-SPEC-0117 example 6: preserve the unresolved macro witness beside a weak direct test without promotion or gap advice (#7542); include the reviewed full human witness.

Command:
`cargo xtask goldens bless rust_mixed_direct_macro_reach --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_mixed_direct_macro_reach (2)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless rust_mixed_direct_macro_reach --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
