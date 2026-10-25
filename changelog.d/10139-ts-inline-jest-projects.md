<!-- section: Fixed -->
- A Jest `jest.config.js` whose `projects` array holds only inline objects (no path, glob, identifier or spread element, and no `rootDir`, `roots`, `root`, `workspace` or `projects` key inside a project) no longer makes the committed-snapshot reading refuse, so a snapshot test that catches the edit stops reading `weakly_exposed` (#10139).
