<!-- section: Internal / CI -->
- The nightly `first-run-main` job writes its first-run walk under the runner temp directory instead of `target/`, so a restored rust-cache can no longer leave an unmarked output directory that the walk refuses to clear (#15190).
