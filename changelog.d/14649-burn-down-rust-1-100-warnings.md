<!-- section: Internal / CI -->
- The workspace builds warning-free on both the pinned toolchain and current
  stable: the duplicated `homepage` manifest field is gone, `repo-policy`'s
  `libc` dependency is Linux-only, and the `/proc/self/mountinfo`, symlink and
  Python-venv test helpers are gated to the platforms that use them, so rustc
  1.100 reports 0 warnings across `--workspace --all-targets` (#14649). The
  Windows-only `never_loop` clippy finding in the unindexed-member test is
  allowed with its cfg rationale, so `cargo clippy --workspace --all-targets`
  is clean on Windows as well.
