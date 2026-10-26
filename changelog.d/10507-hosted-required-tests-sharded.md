<!-- section: Internal / CI -->
- The GitHub-hosted required Rust test phase now runs as four nextest partitions after the other hosted gates pass, and the required result fails unless all four shards passed and cover the full workspace test list (#10507).
