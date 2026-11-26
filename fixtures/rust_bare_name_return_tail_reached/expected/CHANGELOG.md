# Golden Output Changes

## Pending — rust_bare_name_return_tail_reached (1)

Reason:
RIPR-SPEC-0443: a bare parameter or local returned as the changed tail reaches the return sink (#13037)

Command:
`cargo xtask goldens bless rust_bare_name_return_tail_reached --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
