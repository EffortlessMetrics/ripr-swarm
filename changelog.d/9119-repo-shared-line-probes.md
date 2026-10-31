<!-- section: Fixed -->
- Repo-mode probe inventory keeps a production fn that shares a physical
  line with a `#[test]` fn (same opening line, or ending where the test
  starts) instead of dropping it because `find_owner_function` returned
  the test. Later lines of a nested test body stay test evidence (#9119).
