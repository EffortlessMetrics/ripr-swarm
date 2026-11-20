<!-- section: Fixed -->
- `ripr init` in a root with no `Cargo.toml` and no source file ripr reads
  (a brand-new repository, or the parent of the real project) warns that
  `ripr check` has nothing to analyze there and names `--root`, instead of
  writing a Rust configuration in silence (#15050).
