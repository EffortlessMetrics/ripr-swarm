<!-- section: Fixed -->
- `ripr check` from a workspace member now warns on stderr when implicit root
  resolution skips a closer `ripr.toml`. The line names the file that governed
  and the skipped directory entry, and recovery is `--root .` (#7468).
