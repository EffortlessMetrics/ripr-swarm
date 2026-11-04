<!-- section: Internal / CI -->
- The `assert_ok_eq` / `assert_some_eq` import-scope rule (#12564) now has caller-level rows: a file-root import and one inside the test credit, the same import inside another function, a sibling module, a nested block used outside it, or under `#[cfg(any())]` or `#[cfg(not(test))]` does not, while `#[cfg(test)]` still credits (#12705). No product behavior changes.
