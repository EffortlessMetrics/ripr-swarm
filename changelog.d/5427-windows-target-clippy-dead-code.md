<!-- section: Internal / CI -->
- `cargo clippy -p ripr -p xtask --all-targets -- -D warnings` is clean again
  for the Windows and macOS targets: the Linux-only mount-table helpers, an
  argument only the Unix hard-link check reads, and two test helpers whose
  callers are Unix-only no longer count as unused there (#5427).
