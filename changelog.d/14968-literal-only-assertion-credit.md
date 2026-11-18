<!-- section: Fixed -->
- Rust checks: an unconditional assertion that compares only literals
  (`assert!(true)`, `assert!(false)`, `assert_eq!(2 + 2, 4)`) no longer gives
  weak credit to a changed function its test merely calls. The test now reads
  as it would without that assertion, so a bare call reads
  `reachable_unrevealed` and an `unwrap()` / `unwrap_err()` keeps its weak
  reading. A literal inside an `if`, `match` arm or other guard keeps its
  earlier reading. An assertion that reads the function's result keeps its
  class, and the dropped literal never hands credit to an unrelated assertion
  beside it (#14968).
