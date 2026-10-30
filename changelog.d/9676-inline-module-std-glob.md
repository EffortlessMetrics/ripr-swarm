<!-- section: Fixed -->
- A standard-crate glob that is a direct item of an inline module
  (`mod core { pub use core::*; }`) no longer makes every assertion macro
  ambiguous workspace-wide: its root resolves against that module's own items,
  not the enclosing file's. The serde corpus subject now retains its three
  `#[macro_use]` module sources so macro bindings are indexed (#9676).
