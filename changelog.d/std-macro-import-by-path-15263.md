<!-- section: Fixed -->
- A test module that imports a standard macro at its own path (`use std::vec;`, `use alloc::vec;`, `use core::assert_eq;`) no longer un-credits every `assert_eq!` that uses it; the import is read as the standard macro when no manifest, module or alias takes the root name (#15263).
