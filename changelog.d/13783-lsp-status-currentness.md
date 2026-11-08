<!-- section: Fixed -->
- `ripr.collectWorkspaceStatus` now matches hover and code actions after a
  save: while the refresh for the saved files is pending, it reports the
  previous analysis as stale and offers no top repair packet, instead of
  presenting that analysis as current (#13783).
