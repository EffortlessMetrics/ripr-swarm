<!-- section: Internal / CI -->
- `.github/workflows/rust-gates.yml` adds an 8 GB swap file before the two
  hosted Clippy passes and prints swap-out and OOM counters before and after
  them, because the `ripr` lib-test lint unit beside `xtask` outgrew the
  16 GiB runner and merge-queue groups were lost mid-step with exit 143
  (#15935).
