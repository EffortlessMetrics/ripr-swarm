<!-- section: Internal / CI -->
- Hosted Rust shards run the covered-by check and prepare the verifier from the
  normal `xtask` binary produced by workspace test preparation, avoiding the
  separate early dev build while retaining nested Cargo checks (#8194).
