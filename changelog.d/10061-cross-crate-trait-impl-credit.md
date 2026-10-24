<!-- section: Fixed -->
- A method of an impl of a trait declared in a sibling workspace crate (imported
  with `use rcore::Stream;` or written `impl rcore::Stream for ..`) no longer
  borrows the oracle of a same-leaf-name file's doctest and reads
  `weakly_exposed`: the trait is the workspace's, so with nothing naming the
  method it reads `no_static_path`, as a same-crate trait already did (#9169).
  A trait imported from a crate that is not a workspace member keeps the
  foreign-trait reading (#10061).
