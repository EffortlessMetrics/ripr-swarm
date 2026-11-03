<!-- section: Internal / CI -->
- The `assert_ok_eq` / `assert_some_eq` import-scope rule (#12564) now has caller-level rows: a file-root import and one inside the test credit, the same import inside another function or under `#[cfg(any())]` does not (#12705). No product behavior changes.
