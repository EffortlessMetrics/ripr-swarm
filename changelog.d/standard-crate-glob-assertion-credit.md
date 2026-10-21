<!-- section: Fixed -->
- A glob import from the standard crates (`use core::simd::prelude::*;`,
  `use std::io::prelude::*;`, `use std::collections::*;`) in any file of a crate no
  longer makes every plain `assert_eq!`, `assert!` and `assert_ne!` in that
  crate "not credited" (`rust_assertion_context_unestablished`). Such a glob
  can only import items the standard library defines, so it cannot rebind a
  trusted macro. A glob from any other crate, a standard root a local
  module, `extern crate` alias (in any file), Cargo rename or `use .. as`
  takes, `alloc` globs, and every real
  definition or import of the macro name stay ambiguous, and a credited
  assertion still leaves its probe's class unchanged until the changed
  behavior is independently established (#8388).
