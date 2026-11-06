<!-- section: Fixed -->
- LSP: clicking a code lens no longer always re-runs the saved-workspace
  refresh. When the lens row has a related test inside the workspace (and is
  not `no_static_path` or a file that did not parse), the click opens that test
  through the same negotiated route as the "open related test" code action, and
  the lens title ends `· open TEST_NAME` so adjacent rows read differently.
  Rows without a target, and clients that advertise no navigation route, keep
  the refresh action. Navigation adds no actionability or repair data.
  The opened test's name is whitespace-collapsed, length-capped, and replaced
  by `related test` when it contains forbidden title vocabulary.
