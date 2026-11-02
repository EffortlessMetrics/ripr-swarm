<!-- section: Fixed -->
- A `use claims::assert_ok;` (or `assert_some`, `assert_err`, `assert_none`) now resolves the macro only where it is in scope: at the file root, in a module that encloses the test, or in the test's own body. An import in another function, in a sibling module, or under `#[cfg(any())]` no longer credits a test that would not compile (#12462).
