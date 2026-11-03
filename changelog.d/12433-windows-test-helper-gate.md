<!-- section: Internal / CI -->

- Gate three LSP test helpers to `cfg(all(test, unix))`, matching their only callers, so `cargo clippy --workspace --all-targets -- -D warnings` no longer fails on Windows with `dead_code` (#12433).
