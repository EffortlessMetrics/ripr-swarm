<!-- section: Internal / CI -->
- The Windows shard workflow warms the cargo builds the policy tests spawn, and the Linux-only contabo result-policy row is declared `covered_by_unix`, so the covered-by check no longer fails or times out on Windows (#9272).
