# Check that the server started
Run **ripr: Diagnose Setup**. It writes a read-only report to the ripr output
channel: the workspace root, which `ripr` server is in use (your
`ripr.server.path`, a bundled binary, a managed download, or `ripr` on
`PATH`), its version, and the next safe action. It changes no files and runs no tests.

If the status bar shows `ripr: server missing`, no usable server was found.
The report and the output channel name the cause. The usual fixes:

- No network, or a proxy blocks the download: set `ripr.server.path` to a
  `ripr` binary already on this machine, or point
  `ripr.server.downloadBaseUrl` at a mirror.
- No `ripr` installed and downloads are off: turn on
  `ripr.server.autoDownload`, or install with `cargo install ripr`.

Then run **ripr: Restart Server**.
