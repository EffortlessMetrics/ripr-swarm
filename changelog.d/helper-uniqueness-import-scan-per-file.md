<!-- section: Fixed -->
- The helper-uniqueness test-entry check no longer re-parses a test file once
  per test in it, which made `ripr check` superlinear on a diff touching a
  file with many tests (a `quickcheck` file turned a sub-second diff into
  minutes). The import answer is now computed at most once per file and only
  when a test actually calls the queried name; findings are unchanged (#9086).
