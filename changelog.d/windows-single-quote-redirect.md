<!-- section: Fixed -->
- VS Code on Windows: a command whose single-quoted argument holds `<` or
  `>` is no longer offered, since cmd.exe does not treat `'` as a quote and
  would run them as redirects. The `shell_arg` documentation now states the
  remaining Windows shell limits (#9738).
