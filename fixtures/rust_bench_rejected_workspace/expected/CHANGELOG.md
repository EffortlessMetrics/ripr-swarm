# Golden Output Changes

## Pending — rust_bench_rejected_workspace (1)

Reason:
RIPR-SPEC-0356: Cargo rejects this bare package from its enclosing workspace, so its bench cannot supply a runnable test (#15620)

Command:
`cargo xtask goldens bless rust_bench_rejected_workspace --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
