<!-- section: Fixed -->
- VS Code on Windows: gap, first-PR and actionable-queue commands are
  checked against the console double-quote form the server prints there,
  so a workspace path with a space no longer drops the copy action. `%`
  and `!` inside a quoted span are accepted (literal in PowerShell and Git
  Bash; cmd.exe expansion is the documented limit), and a bare `^` is
  refused (#9526, #9659).
