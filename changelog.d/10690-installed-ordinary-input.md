<!-- section: Internal / CI -->
- The existing ordinary developer-loop test accepts a strictly identified
  installed executable, retains command and source evidence outside fixture
  cleanup, and checks wrong input, wrong base, empty test selection, wrong
  expected output, and restoration. Its bounded commands use the existing
  process owner with explicit descendant containment; the default copied
  Cargo binary remains a separately labeled regression (#10690).
