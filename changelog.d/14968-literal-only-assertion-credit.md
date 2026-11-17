<!-- section: Fixed -->
- Rust checks: an assertion that compares only literals (`assert!(true)`,
  `assert!(false)`, `assert_eq!(2 + 2, 4)`) no longer gives weak credit to a
  changed function its test merely calls. Such a test now reads
  `reachable_unrevealed` instead of `weakly_exposed`. An assertion that reads
  the function's result keeps its class (#14968).
