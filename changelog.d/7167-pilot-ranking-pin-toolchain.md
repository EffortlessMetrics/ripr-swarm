<!-- section: Internal / CI -->
- The Pilot Ranking workflow installs the exact Rust 1.95.0 toolchain, matching `rust-toolchain.toml` and `ci.yml`, instead of floating `stable`, so a compiler release cannot move pilot rankings without a source change (#7167).
