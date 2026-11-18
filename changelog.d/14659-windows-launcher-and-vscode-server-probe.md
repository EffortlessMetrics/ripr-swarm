<!-- section: Fixed -->
- Windows: the VS Code extension accepts a `.cmd` or `.bat` shim in
  `ripr.server.path` (for example npm's `ripr.cmd`), which used to fail with
  `spawn_failure`; a direct `.exe` path is still spawned without a shell. The
  compatibility probe now runs with the configured `ripr.server.args` instead
  of a fixed `lsp --stdio`, and shell launches quote arguments so their
  boundaries and embedded quotes survive `cmd.exe`. The extension also
  activates on a workspace that contains `ripr.toml` or `pyproject.toml`. The
  npm launcher selects the x64 package on Windows ARM64, where it used to
  stop with `platform_unsupported`, and its reinstall hint names `--cpu=x64`
  there (#14659).
