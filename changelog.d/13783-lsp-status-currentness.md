<!-- section: Fixed -->
- `ripr.collectWorkspaceStatus` and `ripr.collectTopLimitation` now match
  hover and code actions after a save: while the refresh for the saved files
  is pending, they report the previous analysis as stale and offer no top
  repair packet, instead of presenting that analysis as current (#13783).
