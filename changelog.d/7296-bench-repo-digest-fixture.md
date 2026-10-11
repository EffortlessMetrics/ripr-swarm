<!-- section: Fixed -->
- `bench-agent-surfaces` binds repository input digests to the corpus root.
  Its identity tests use their own committed Git history, so shallow harness
  checkouts retain the corpus tree and live-checkout identity controls (#7296).
