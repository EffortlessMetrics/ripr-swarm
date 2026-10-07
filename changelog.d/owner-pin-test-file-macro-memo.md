<!-- section: Fixed -->
- Performance: `ripr check` no longer re-parses a related test's file once per
  probe, test and assertion to name a refused macro binding (a regression from
  #5416). A diff touching a large command catalog went from over 15 minutes to
  94 s, and the PR-evidence gate stops timing out on it (#6967).
