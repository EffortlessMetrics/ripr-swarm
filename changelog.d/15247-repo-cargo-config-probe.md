<!-- section: Security -->
- `ripr` no longer runs a `rustc`, wrapper, flag or `[env]` setting that an
  analyzed tree's own `.cargo/config.toml` names when it checks audited
  dependency identities through `cargo metadata`; it withholds that identity
  instead (#15247).
