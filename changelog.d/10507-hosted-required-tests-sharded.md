<!-- section: Internal / CI -->
- The GitHub-hosted required Rust test phase now runs as four nextest partitions after the other hosted gates pass, and the required result fails unless `cargo xtask verify-test-shards` finds every shard's status, run context and JUnit testcases current, green and equal to its partition list, and the partitions cover the full workspace test list (#10507).
