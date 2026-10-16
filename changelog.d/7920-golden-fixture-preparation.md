<!-- section: Internal / CI -->
- Add `cargo xtask goldens prepare [--offline]` and an offline fixture readiness
  check with a shared Cargo-home and toolchain binding, so missing locked
  dependencies receive preparation advice before golden comparison (#7616, #7920).
