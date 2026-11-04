<!-- section: Fixed -->
- `ripr repair` no longer starts an attempt when its handoff card cannot be produced (a repair that would begin in a proposed new test file); it says there is no existing test to extend and to write the test by hand, and creates no attempt (#12867). `ripr agent start` and the MCP repair admission refuse the same seams, since they share the gate.
