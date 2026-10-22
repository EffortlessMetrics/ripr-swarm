<!-- section: Fixed -->
- VS Code on Windows: gap, first-PR and actionable-queue commands are
  checked against the console double-quote form the server prints there,
  so a workspace path with a space no longer drops the copy action. `%`
  and `!` inside a quoted argument are accepted (literal in PowerShell and
  native argv; cmd.exe and interactive Git Bash expansion is the documented
  limit), but refused in a redirect target, where cmd.exe expansion could
  leave the workspace. An odd backslash run before a quote and a bare `^`
  are refused (#9526, #9659).
