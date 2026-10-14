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
RIPR-SPEC-0158 and #7805: independently audited additive source_file metadata only; all classification, oracle, evidence and human output preserved

Command:
`cargo xtask goldens bless rust_mixed_direct_macro_reach --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
