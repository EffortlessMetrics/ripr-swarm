<!-- section: Internal / CI -->
- Add `cargo xtask goldens prepare [--offline]` and an offline fixture readiness
  check with a shared Cargo-home and toolchain binding, so missing locked
  dependencies receive preparation advice before golden comparison. Offline
  proxy launches also refuse a missing toolchain without auto-installation,
  including the preparation version probes (#7616, #7920).
