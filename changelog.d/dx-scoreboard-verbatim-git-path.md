<!-- section: Fixed -->
- `cargo xtask dx-scoreboard` corpus preparation now works on Windows: the isolated corpus clone
  receives a plain drive or UNC path instead of the `\\?\` verbatim form produced by
  `fs::canonicalize`, which `git clone` parsed as a remote host and refused (#12947).
