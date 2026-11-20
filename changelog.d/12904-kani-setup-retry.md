<!-- section: Internal / CI -->
- The Kani lane retries `cargo install kani-verifier` / `cargo kani setup` up to four times with 5/15/30s backoff, so a transient GitHub release-bundle 5xx no longer skips the harness run; a persistent failure still fails the step (#12904).
