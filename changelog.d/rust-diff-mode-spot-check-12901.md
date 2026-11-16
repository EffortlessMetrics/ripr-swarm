<!-- section: Internal / CI -->
- `cargo xtask mutation-spot-check --rust-diff-mode` also scores cargo-mutants mutants through `ripr check --diff` from each mutant's recorded diff and reports a separate `rust_diff_mode` recall figure beside the unchanged span join (#12901).
