<!-- section: Fixed -->
- Verdict corpus: authored subjects that pin registry crates
  (`authored-spec-confirm`, `authored-spec-harness`) retain a hash-checked
  `Cargo.lock`. `verdict-corpus relabel` copies it into the rebuilt tree and
  runs cargo with `--locked`, so a drifted transitive graph fails instead of
  floating on the host (#7000).
