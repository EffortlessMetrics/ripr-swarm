<!-- section: Fixed -->
- `ripr repair` now says so when the changed Python or TypeScript code was not
  analyzed because the language adapter is not enabled (a flat-layout Python
  repo with no project marker and no `ripr.toml`), and prints the
  `ripr.toml` `[languages]` line to add. Before, it pointed at a `ripr check`
  that analyzes none of the change (#13571).
