<!-- section: Fixed -->
- A Rust test that stores a check in a boolean local and compares it with `true` (`let ok = value > 0; assert_eq!(ok, true);`) is now only as strong as that check written in place. A comparison, several-value `matches!` or unpinned pattern stored this way no longer makes a change read `exposed`. A local that pins one value (`let ok = value == 90;`) stays exact (#16296).
