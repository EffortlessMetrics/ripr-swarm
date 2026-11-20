<!-- section: Fixed -->
- `ripr doctor` no longer prints "Detected languages: none detected" in a
  monorepo whose projects sit below the root while the loaded `ripr.toml`
  enables languages; it says where the marker scan looked and names the
  configured languages (#15195).
