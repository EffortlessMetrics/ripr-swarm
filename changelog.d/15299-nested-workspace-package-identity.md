<!-- section: Fixed -->
- A Cargo workspace below the repository root (`rust/crates/*`, as in a
  mixed-language monorepo) keeps each member's package identity: a crate's
  own test is no longer lost to `no_static_path` when a sibling crate has a
  same-named function, and two workspaces that both have `crates/<name>`
  read as two packages (#15299).
