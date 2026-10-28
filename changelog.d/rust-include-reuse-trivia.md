<!-- section: Fixed -->
- Rust index reuse now recognizes whitespace-separated `include !` additions,
  so widening analysis scope cannot retain a fragment's former unique
  compilation-unit identity after another include parent becomes visible (#8547).
