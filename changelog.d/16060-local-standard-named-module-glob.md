<!-- section: Fixed -->
- A glob of a module the same file declares, such as `mod core;` followed
  by `use core::*;`, is no longer treated as an import from another crate
  when the module is named `core`, `std` or `alloc`. Before, the glob made
  every `assert!` and `assert_eq!` in that file, and in tests the scan linked
  to it, look like a possible macro rebinding (RIPR-SPEC-0369, #16060).
