<!-- section: Fixed -->
- `ripr check` from a workspace member now warns on stderr when implicit root
  resolution skips a closer `ripr.toml`. The line names the file that governed
  and the file that was ignored, and recovery is `--root` at the invocation
  directory (#7468).
