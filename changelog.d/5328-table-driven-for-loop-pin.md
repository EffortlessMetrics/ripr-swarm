<!-- section: Fixed -->
- A Rust `assert_eq!` inside a `for` loop over a non-empty literal table
  (`for (x, want) in [(1, 2), ..]`, or a local `let cases = [..]`) now
  counts as running, so a table-driven test that pins the owner's return
  value reads `exposed` instead of `weakly_exposed`. Ranges, empty or
  repeat arrays, `vec!`, constants and calls may run zero times and stay
  refused, as does a loop with an earlier `break` or `continue` (#5328).
