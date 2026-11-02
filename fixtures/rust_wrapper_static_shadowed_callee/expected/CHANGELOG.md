## 2026-11-02

RIPR-SPEC-0186 #10357 item 2: a `static` callee shadow inside the wrapper
withholds helper-chain forwarding; neither twin is promoted to `exposed`.

## Pending — rust_wrapper_static_shadowed_callee (1)

Reason:
RIPR-SPEC-0186 #10357 item 2 static callee shadow

Command:
`cargo xtask goldens bless rust_wrapper_static_shadowed_callee --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
