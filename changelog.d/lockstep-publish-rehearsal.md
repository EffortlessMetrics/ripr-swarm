<!-- section: Internal / CI -->
- Release proof and ci-full now rehearse the ADR 0027 lockstep publish: `cargo xtask publish-dry-run` dry-runs
  `ripr-contract` and checks the packaged `ripr` against the packaged `ripr-contract` with a fresh
  lockfile, instead of a bare `cargo publish -p ripr --dry-run` that cannot resolve the unpublished
  contract crate from crates.io. Workflow shell controls isolate literal redirections from the source checkout (#13107).
