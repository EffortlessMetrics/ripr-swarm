<!-- section: Fixed -->
- A crate-root `extern crate core as std;` (the common `no_std` shim) no
  longer makes every plain `assert_eq!` in the crate, doctests included,
  read "not credited ... imports a different `assert_eq`". An alias of a
  Cargo-renamed `core`, of `alloc`, or of any other crate still refuses
  (#14390).
