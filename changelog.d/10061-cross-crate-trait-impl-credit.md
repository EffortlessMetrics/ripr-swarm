<!-- section: Fixed -->
- A method of an impl of a trait declared in a sibling workspace crate (imported
  with `use rcore::Stream;` or written `impl rcore::Stream for ..`) no longer
  borrows the oracle of a same-leaf-name file's doctest and reads
  `weakly_exposed`: the trait is the workspace's, so with nothing naming the
  method it reads `no_static_path`, as a same-crate trait already did (#9169).
  A trait imported from a crate that is not a workspace member keeps the
  foreign-trait reading (#10061).
  Not covered, and still able to borrow a same-leaf-name oracle: a glob import
  of an unindexed member's trait (`use rcore::*;`), a re-export of the trait
  (`pub use rcore::Stream;` then `use crate::Stream;`), and `[workspace]
  members` globs that are not a trailing `/*`. A member crate named `core`,
  `alloc` or `std` shadows the std traits of that root and reads as the
  workspace's.
