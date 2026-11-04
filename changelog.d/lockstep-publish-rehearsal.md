<!-- section: Internal / CI -->
- Release proof now rehearses the ADR 0027 lockstep publish: `cargo xtask publish-dry-run` dry-runs
  `ripr-contract` and checks the packaged `ripr` against the packaged `ripr-contract` with a fresh
  lockfile, instead of a bare `cargo publish -p ripr --dry-run` that cannot resolve the unpublished
  contract crate from crates.io.
